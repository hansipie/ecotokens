//! Space Invaders mini-game: every filtered command spawns an enemy whose
//! strength scales with the tokens it saved. Arrow keys move, space fires.
//!
//! Commands loaded at startup form the classic marching formation. Commands
//! filtered *live* while the game is running instead spawn as free-roaming
//! "snakes" that slither around inside the bounding box of the formation.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{poll, read, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::{Frame, Terminal};

use crate::metrics::report::{filter_by_period, Period};
use crate::metrics::store::{read_from, CommandFamily, FilterMode, Interception};

const SPRITE_W: u16 = 3;
const COL_SPACING: u16 = 5;
const ROW_SPACING: u16 = 2;
const MAX_COLS: u16 = 10;
const MAX_ROWS: u16 = 4;
const MAX_PLAYER_BULLETS: usize = 4;

/// How often free-roaming snakes advance one cell.
const SNAKE_STEP: Duration = Duration::from_millis(110);

/// Legend sidebar: width, and the minimum total width before we reserve it.
const SIDEBAR_W: u16 = 22;
const SIDEBAR_MIN_WIDTH: u16 = 60;

/// One sprite per enemy level (all glyphs are single-width).
const SPRITES: [&str; 4] = [" ▿ ", "‹▼›", "<◆>", "«◈»"];
const PLAYER_SPRITE: &str = "◢W◣";

/// Every command family in canonical order — drives the legend row order.
const ALL_FAMILIES: [CommandFamily; 17] = [
    CommandFamily::Git,
    CommandFamily::Cargo,
    CommandFamily::Cpp,
    CommandFamily::Fs,
    CommandFamily::Markdown,
    CommandFamily::Python,
    CommandFamily::ConfigFile,
    CommandFamily::Go,
    CommandFamily::Js,
    CommandFamily::Gh,
    CommandFamily::Container,
    CommandFamily::Grep,
    CommandFamily::Aws,
    CommandFamily::Network,
    CommandFamily::Db,
    CommandFamily::Generic,
    CommandFamily::NativeRead,
];

/// Human-readable token band that maps to each enemy level (1..=4).
const LEVEL_BANDS: [&str; 4] = ["≤1k tok", "1k–5k", "5k–20k", ">20k"];

/// Enemy level 1..=4 from the tokens saved by the interception.
fn enemy_level(saved: u64) -> u8 {
    match saved {
        0..=999 => 1,
        1000..=4999 => 2,
        5000..=19999 => 3,
        _ => 4,
    }
}

fn level_points(level: u8) -> u32 {
    level as u32 * 100
}

/// Stable color per command family, shared by every level of that family.
pub fn family_color(family: &CommandFamily) -> Color {
    match family {
        CommandFamily::Git => Color::LightRed,
        CommandFamily::Cargo => Color::Yellow,
        CommandFamily::Cpp => Color::Blue,
        CommandFamily::Fs => Color::Cyan,
        CommandFamily::Markdown => Color::White,
        CommandFamily::Python => Color::LightGreen,
        CommandFamily::ConfigFile => Color::Gray,
        CommandFamily::Go => Color::LightCyan,
        CommandFamily::Js => Color::LightYellow,
        CommandFamily::Gh => Color::LightMagenta,
        CommandFamily::Container => Color::LightBlue,
        CommandFamily::Grep => Color::Green,
        CommandFamily::Aws => Color::Magenta,
        CommandFamily::Network => Color::LightBlue,
        CommandFamily::Db => Color::Red,
        CommandFamily::Generic => Color::Gray,
        CommandFamily::NativeRead => Color::White,
    }
}

fn family_label(family: &CommandFamily) -> &'static str {
    match family {
        CommandFamily::Git => "git",
        CommandFamily::Cargo => "cargo",
        CommandFamily::Cpp => "cpp",
        CommandFamily::Fs => "fs",
        CommandFamily::Markdown => "markdown",
        CommandFamily::Python => "python",
        CommandFamily::ConfigFile => "config",
        CommandFamily::Go => "go",
        CommandFamily::Js => "js",
        CommandFamily::Gh => "gh",
        CommandFamily::Container => "container",
        CommandFamily::Grep => "grep",
        CommandFamily::Aws => "aws",
        CommandFamily::Network => "network",
        CommandFamily::Db => "db",
        CommandFamily::Generic => "generic",
        CommandFamily::NativeRead => "read",
    }
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

struct Enemy {
    col: u16,
    row: u16,
    hp: u8,
    level: u8,
    color: Color,
    command: String,
    family: &'static str,
    saved: u64,
    alive: bool,
}

impl Enemy {
    fn from_interception(item: Interception, col: u16, row: u16) -> Self {
        let saved = (item.tokens_before as u64).saturating_sub(item.tokens_after as u64);
        let level = enemy_level(saved);
        let command = item.command.replace(['\n', '\r'], " ");
        Enemy {
            col,
            row,
            hp: level,
            level,
            color: family_color(&item.command_family),
            command,
            family: family_label(&item.command_family),
            saved,
            alive: true,
        }
    }
}

/// A command filtered live during play: a free-roaming enemy whose body
/// trails its head as it slithers inside the formation's bounding box.
struct Snake {
    /// Head is the front element; the tail follows behind it.
    body: VecDeque<(u16, u16)>,
    dx: i32,
    dy: i32,
    hp: u8,
    level: u8,
    color: Color,
    command: String,
    family: &'static str,
    saved: u64,
    alive: bool,
}

#[derive(PartialEq)]
enum Phase {
    Playing,
    Paused,
    GameOver,
    Victory,
    Empty,
}

pub struct Game {
    source: Vec<Interception>,
    pending: VecDeque<Interception>,
    enemies: Vec<Enemy>,
    snakes: Vec<Snake>,
    total_in_wave: usize,
    origin_x: i32,
    origin_y: u16,
    dir: i32,
    wave: u32,
    score: u64,
    lives: u8,
    player_x: u16,
    player_placed: bool,
    player_bullets: Vec<(u16, u16)>,
    enemy_bullets: Vec<(u16, u16)>,
    explosions: Vec<(u16, u16, Instant)>,
    last_kill: Option<(String, Color)>,
    phase: Phase,
    rng: u64,
    area: Rect,
    last_enemy_step: Instant,
    last_player_bullet_step: Instant,
    last_enemy_bullet_step: Instant,
    last_snake_step: Instant,
    invuln_until: Instant,
    paused_at: Option<Instant>,
}

impl Game {
    pub fn new(mut items: Vec<Interception>) -> Self {
        items.retain(|i| i.mode != FilterMode::Passthrough);
        // Newest commands first: the most recent activity fills the first waves.
        items.reverse();
        let phase = if items.is_empty() {
            Phase::Empty
        } else {
            Phase::Playing
        };
        let now = Instant::now();
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            | 1;
        Game {
            source: items.clone(),
            pending: items.into(),
            enemies: Vec::new(),
            snakes: Vec::new(),
            total_in_wave: 0,
            origin_x: 0,
            origin_y: 0,
            dir: 1,
            wave: 0,
            score: 0,
            lives: 3,
            player_x: 0,
            player_placed: false,
            player_bullets: Vec::new(),
            enemy_bullets: Vec::new(),
            explosions: Vec::new(),
            last_kill: None,
            phase,
            rng: seed,
            area: Rect::default(),
            last_enemy_step: now,
            last_player_bullet_step: now,
            last_enemy_bullet_step: now,
            last_snake_step: now,
            invuln_until: now,
            paused_at: None,
        }
    }

    fn restart(&mut self) {
        *self = Game::new(self.source.clone());
    }

    /// Append freshly recorded interceptions (live reload). While a game is in
    /// progress each new command becomes a free-roaming snake; otherwise it
    /// joins the queue that fills the next formation wave.
    pub fn enqueue(&mut self, items: Vec<Interception>) {
        for item in items {
            if item.mode != FilterMode::Passthrough {
                self.source.push(item.clone());
                if self.phase == Phase::Playing && self.area.width >= 20 {
                    self.spawn_snake(item);
                } else {
                    self.pending.push_front(item);
                }
            }
        }
        if self.phase == Phase::Empty && !self.pending.is_empty() {
            self.phase = Phase::Playing;
        }
    }

    /// Bounding box the snakes roam within: the extent of the alive formation
    /// enemies, or the whole playfield when no formation is on screen.
    fn group_bounds(&self) -> Rect {
        let field = self.field();
        let mut min_x = i32::MAX;
        let mut max_x = i32::MIN;
        let mut min_y = u16::MAX;
        let mut max_y = 0u16;
        for e in self.enemies.iter().filter(|e| e.alive) {
            let (x, y) = self.enemy_pos(e);
            min_x = min_x.min(x);
            max_x = max_x.max(x + SPRITE_W as i32);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        if min_x == i32::MAX {
            // No formation on screen. `field()` derives its height from
            // `area.height - 2`, so on a terminal two rows tall or shorter it is
            // zero-sized — and callers take a modulo of these extents
            // (`spawn_snake`), which would panic on a zero divisor. Clamp to the
            // same minimum the computed branch below already guarantees.
            return Rect {
                width: field.width.max(1),
                height: field.height.max(1),
                ..field
            };
        }
        let field_right = (field.x + field.width) as i32;
        let field_bottom = field.y + field.height;
        let x = min_x.clamp(field.x as i32, field_right) as u16;
        let right = max_x.clamp(x as i32, field_right) as u16;
        let y = min_y.clamp(field.y, field_bottom);
        let bottom = max_y.clamp(y, field_bottom);
        Rect {
            x,
            y,
            width: right.saturating_sub(x).max(1),
            height: bottom.saturating_sub(y).max(1),
        }
    }

    /// Spawn a snake at a random spot inside the current formation bounds.
    fn spawn_snake(&mut self, item: Interception) {
        let saved = (item.tokens_before as u64).saturating_sub(item.tokens_after as u64);
        let level = enemy_level(saved);
        let bounds = self.group_bounds();
        let hx = bounds.x + (self.next_rand() % bounds.width as u64) as u16;
        let hy = bounds.y + (self.next_rand() % bounds.height as u64) as u16;
        // Body length grows with the enemy level (head + trailing segments).
        let len = 2 * (1 + level as usize);
        let mut body = VecDeque::with_capacity(len);
        for _ in 0..len {
            body.push_back((hx, hy));
        }
        let dx = if self.next_rand() % 2 == 0 { 1 } else { -1 };
        let dy = if self.next_rand() % 2 == 0 { 1 } else { -1 };
        self.snakes.push(Snake {
            body,
            dx,
            dy,
            hp: level,
            level,
            color: family_color(&item.command_family),
            command: item.command.replace(['\n', '\r'], " "),
            family: family_label(&item.command_family),
            saved,
            alive: true,
        });
    }

    /// Advance every snake one cell, bouncing off the group bounds.
    fn step_snakes(&mut self) {
        let b = self.group_bounds();
        let left = b.x as i32;
        let right = (b.x + b.width).saturating_sub(1) as i32;
        let top = b.y as i32;
        let bottom = (b.y + b.height).saturating_sub(1) as i32;
        for s in self.snakes.iter_mut().filter(|s| s.alive) {
            let (hx, hy) = *s.body.front().expect("snake body is never empty");
            let mut nx = hx as i32 + s.dx;
            let mut ny = hy as i32 + s.dy;
            if nx < left || nx > right {
                s.dx = -s.dx;
                nx = hx as i32 + s.dx;
            }
            if ny < top || ny > bottom {
                s.dy = -s.dy;
                ny = hy as i32 + s.dy;
            }
            let head = (nx.clamp(left, right) as u16, ny.clamp(top, bottom) as u16);
            s.body.push_front(head);
            s.body.pop_back();
        }
    }

    fn snakes_alive(&self) -> bool {
        self.snakes.iter().any(|s| s.alive)
    }

    fn next_rand(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    /// Playfield between the top status line and the bottom status bar,
    /// minus the legend sidebar when the terminal is wide enough for it.
    fn field(&self) -> Rect {
        let reserved = if self.area.width >= SIDEBAR_MIN_WIDTH {
            SIDEBAR_W
        } else {
            0
        };
        Rect {
            x: self.area.x,
            y: self.area.y + 1,
            width: self.area.width.saturating_sub(reserved),
            height: self.area.height.saturating_sub(2),
        }
    }

    /// Right-hand legend column, or `None` when the terminal is too narrow.
    fn sidebar(&self) -> Option<Rect> {
        if self.area.width < SIDEBAR_MIN_WIDTH {
            return None;
        }
        Some(Rect {
            x: self.area.x + self.area.width - SIDEBAR_W,
            y: self.area.y + 1,
            width: SIDEBAR_W,
            height: self.area.height.saturating_sub(2),
        })
    }

    /// Families present in the loaded interceptions, in canonical order, each
    /// with its color and the number of enemies it contributes.
    fn family_legend(&self) -> Vec<(&'static str, Color, usize)> {
        ALL_FAMILIES
            .iter()
            .filter_map(|fam| {
                let count = self
                    .source
                    .iter()
                    .filter(|i| &i.command_family == fam)
                    .count();
                (count > 0).then(|| (family_label(fam), family_color(fam), count))
            })
            .collect()
    }

    fn player_y(&self) -> u16 {
        let f = self.field();
        f.y + f.height.saturating_sub(1)
    }

    fn enemy_pos(&self, e: &Enemy) -> (i32, u16) {
        (
            self.origin_x + (e.col * COL_SPACING) as i32,
            self.origin_y + e.row * ROW_SPACING,
        )
    }

    fn alive_count(&self) -> usize {
        self.enemies.iter().filter(|e| e.alive).count()
    }

    fn spawn_wave(&mut self, field: Rect) -> bool {
        if self.pending.is_empty() {
            return false;
        }
        let cols = (field.width.saturating_sub(6) / COL_SPACING).clamp(1, MAX_COLS);
        let max_rows = (field.height / ROW_SPACING)
            .saturating_sub(3)
            .clamp(1, MAX_ROWS);
        let capacity = (cols * max_rows) as usize;
        let n = capacity.min(self.pending.len());
        let mut batch: Vec<Interception> = self.pending.drain(..n).collect();
        // Strongest enemies on the top rows, like the classic formation.
        batch.sort_by_key(|i| {
            std::cmp::Reverse((i.tokens_before as u64).saturating_sub(i.tokens_after as u64))
        });
        self.enemies = batch
            .into_iter()
            .enumerate()
            .map(|(i, item)| Enemy::from_interception(item, i as u16 % cols, i as u16 / cols))
            .collect();
        self.total_in_wave = n;
        self.origin_x = (field.x + 2) as i32;
        self.origin_y = field.y + 1;
        self.dir = 1;
        self.wave += 1;
        self.player_bullets.clear();
        self.enemy_bullets.clear();
        true
    }

    fn enemy_interval(&self) -> Duration {
        let alive = self.alive_count() as u64;
        let total = self.total_in_wave.max(1) as u64;
        let ms = (90 + 420 * alive / total).saturating_sub(self.wave as u64 * 15);
        Duration::from_millis(ms.max(70))
    }

    pub fn on_key(&mut self, code: KeyCode) {
        match self.phase {
            Phase::Playing => {
                let field = self.field();
                if field.width < 10 {
                    return;
                }
                let min_x = field.x + 1;
                let max_x = (field.x + field.width).saturating_sub(SPRITE_W + 1);
                match code {
                    KeyCode::Left => {
                        self.player_x = self.player_x.saturating_sub(2).max(min_x);
                    }
                    KeyCode::Right => {
                        self.player_x = (self.player_x + 2).min(max_x);
                    }
                    KeyCode::Char(' ') => {
                        if self.player_bullets.len() < MAX_PLAYER_BULLETS {
                            self.player_bullets
                                .push((self.player_x + 1, self.player_y().saturating_sub(1)));
                        }
                    }
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        self.phase = Phase::Paused;
                        self.paused_at = Some(Instant::now());
                    }
                    _ => {}
                }
            }
            Phase::Paused => {
                if matches!(code, KeyCode::Char('p') | KeyCode::Char('P')) {
                    // Shift every timer forward by the pause duration so the
                    // game resumes exactly where it left off, with no burst
                    // of "catch-up" movement or an unfairly short invuln window.
                    if let Some(elapsed) = self.paused_at.take().map(|t| t.elapsed()) {
                        self.last_enemy_step += elapsed;
                        self.last_player_bullet_step += elapsed;
                        self.last_enemy_bullet_step += elapsed;
                        self.last_snake_step += elapsed;
                        self.invuln_until += elapsed;
                        for e in &mut self.explosions {
                            e.2 += elapsed;
                        }
                    }
                    self.phase = Phase::Playing;
                }
            }
            Phase::GameOver | Phase::Victory => {
                if code == KeyCode::Char('r') {
                    self.restart();
                }
            }
            Phase::Empty => {}
        }
    }

    pub fn tick(&mut self) {
        if self.phase != Phase::Playing || self.area.width < 20 || self.area.height < 8 {
            return;
        }
        let field = self.field();
        let player_y = self.player_y();
        let now = Instant::now();

        if !self.player_placed {
            self.player_x = field.x + field.width / 2;
            self.player_placed = true;
        }
        self.explosions
            .retain(|(_, _, t)| now.duration_since(*t) < Duration::from_millis(350));

        // Next wave, or victory once the queue is empty and nothing roams.
        if self.alive_count() == 0 && self.explosions.is_empty() {
            if self.spawn_wave(field) {
                self.last_enemy_step = now;
            } else if !self.snakes_alive() {
                self.phase = Phase::Victory;
                return;
            }
            // Otherwise the formation is clear but snakes remain: keep playing
            // so the player can hunt them down below.
        }

        // Free-roaming snakes slither inside the formation bounds.
        if now.duration_since(self.last_snake_step) >= SNAKE_STEP {
            self.last_snake_step = now;
            self.step_snakes();
        }

        // Formation step: slide sideways, descend and reverse at the edges.
        if now.duration_since(self.last_enemy_step) >= self.enemy_interval() {
            self.last_enemy_step = now;
            let xs: Vec<i32> = self
                .enemies
                .iter()
                .filter(|e| e.alive)
                .map(|e| self.enemy_pos(e).0)
                .collect();
            if let (Some(&min_x), Some(&max_x)) = (xs.iter().min(), xs.iter().max()) {
                let left_bound = (field.x + 1) as i32;
                let right_bound = (field.x + field.width) as i32 - (SPRITE_W + 1) as i32;
                if (self.dir > 0 && max_x + 1 > right_bound)
                    || (self.dir < 0 && min_x - 1 < left_bound)
                {
                    self.origin_y += 1;
                    self.dir = -self.dir;
                } else {
                    self.origin_x += self.dir;
                }
            }
            // The lowest alive enemy in a random column returns fire.
            if self.next_rand() % 100 < 20 + (self.wave as u64).min(20) {
                let alive: Vec<usize> = (0..self.enemies.len())
                    .filter(|&i| self.enemies[i].alive)
                    .collect();
                if !alive.is_empty() {
                    let pick = self.next_rand() as usize % alive.len();
                    let col = self.enemies[alive[pick]].col;
                    if let Some(&shooter) = alive
                        .iter()
                        .filter(|&&i| self.enemies[i].col == col)
                        .max_by_key(|&&i| self.enemies[i].row)
                    {
                        let (x, y) = self.enemy_pos(&self.enemies[shooter]);
                        if x >= 0 {
                            self.enemy_bullets.push((x as u16 + 1, y + 1));
                        }
                    }
                }
            }
            // Invasion: the formation reached the player row.
            let reached = self
                .enemies
                .iter()
                .filter(|e| e.alive)
                .any(|e| self.enemy_pos(e).1 >= player_y.saturating_sub(1));
            if reached {
                self.phase = Phase::GameOver;
                return;
            }
        }

        // Player bullets fly up.
        if now.duration_since(self.last_player_bullet_step) >= Duration::from_millis(30) {
            self.last_player_bullet_step = now;
            for b in &mut self.player_bullets {
                b.1 = b.1.saturating_sub(1);
            }
            self.player_bullets.retain(|b| b.1 > field.y);
        }
        self.resolve_hits(now);

        // Enemy bullets fall down.
        if now.duration_since(self.last_enemy_bullet_step) >= Duration::from_millis(80) {
            self.last_enemy_bullet_step = now;
            for b in &mut self.enemy_bullets {
                b.1 += 1;
            }
            let mut hit_player = false;
            let px = self.player_x;
            self.enemy_bullets.retain(|&(x, y)| {
                if y == player_y && x >= px && x < px + SPRITE_W {
                    hit_player = true;
                    false
                } else {
                    y <= player_y
                }
            });
            if hit_player && now >= self.invuln_until {
                self.lives = self.lives.saturating_sub(1);
                self.invuln_until = now + Duration::from_secs(2);
                self.enemy_bullets.clear();
                if self.lives == 0 {
                    self.phase = Phase::GameOver;
                }
            }
        }
    }

    /// Match player bullets against enemy sprites; record kills in the status bar.
    fn resolve_hits(&mut self, now: Instant) {
        let bullets = std::mem::take(&mut self.player_bullets);
        let mut surviving = Vec::with_capacity(bullets.len());
        'bullets: for (bx, by) in bullets {
            for idx in 0..self.enemies.len() {
                if !self.enemies[idx].alive {
                    continue;
                }
                let (ex, ey) = self.enemy_pos(&self.enemies[idx]);
                if ey == by && (bx as i32) >= ex && (bx as i32) < ex + SPRITE_W as i32 {
                    let e = &mut self.enemies[idx];
                    e.hp = e.hp.saturating_sub(1);
                    if e.hp == 0 {
                        e.alive = false;
                        let points = level_points(e.level);
                        self.score += points as u64;
                        let msg = format!(
                            "✸ {}  [{} · lvl {}]  +{} pts  ·  {} tokens saved",
                            e.command,
                            e.family,
                            e.level,
                            points,
                            thousands(e.saved)
                        );
                        let color = e.color;
                        self.last_kill = Some((msg, color));
                        self.explosions.push((bx, by, now));
                    }
                    continue 'bullets;
                }
            }
            // Then the free-roaming snakes: any body segment counts as a hit.
            for sidx in 0..self.snakes.len() {
                if !self.snakes[sidx].alive {
                    continue;
                }
                let hit = self.snakes[sidx]
                    .body
                    .iter()
                    .any(|&(sx, sy)| sx == bx && sy == by);
                if !hit {
                    continue;
                }
                let s = &mut self.snakes[sidx];
                s.hp = s.hp.saturating_sub(1);
                if s.hp == 0 {
                    s.alive = false;
                    let points = level_points(s.level);
                    self.score += points as u64;
                    let msg = format!(
                        "∿ {}  [{} · lvl {}]  +{} pts  ·  {} tokens saved",
                        s.command,
                        s.family,
                        s.level,
                        points,
                        thousands(s.saved)
                    );
                    let color = s.color;
                    self.last_kill = Some((msg, color));
                    self.explosions.push((bx, by, now));
                }
                continue 'bullets;
            }
            surviving.push((bx, by));
        }
        self.player_bullets = surviving;
        self.snakes.retain(|s| s.alive);
    }

    /// Legend column: which color/shape maps to which command family and level.
    fn render_sidebar(&self, buf: &mut Buffer, area: Rect, bar: Rect) {
        // Vertical divider on the left edge of the sidebar.
        for y in bar.y..bar.y + bar.height {
            put(
                buf,
                area,
                bar.x as i32,
                y,
                "│",
                Style::default().fg(Color::DarkGray),
            );
        }
        let cx = bar.x as i32 + 2;
        let bottom = bar.y + bar.height;
        let mut y = bar.y + 1;
        let heading = |b: &mut Buffer, x: i32, y: u16, s: &str| {
            put(
                b,
                area,
                x,
                y,
                s,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
        };

        // Families: a color swatch, the family name and its enemy count.
        heading(buf, cx, y, "FAMILIES");
        y += 2;
        for (label, color, count) in self.family_legend() {
            if y >= bottom.saturating_sub(7) {
                break;
            }
            put(buf, area, cx, y, "▉▉", Style::default().fg(color));
            put(
                buf,
                area,
                cx + 3,
                y,
                &format!("{label} ×{count}"),
                Style::default().fg(Color::Gray),
            );
            y += 1;
        }

        // Levels: the sprite shape and the token band that spawns it.
        y += 1;
        if y < bottom {
            heading(buf, cx, y, "LEVELS");
            y += 2;
        }
        for (i, sprite) in SPRITES.iter().enumerate() {
            if y >= bottom {
                break;
            }
            put(buf, area, cx, y, sprite, Style::default().fg(Color::White));
            put(
                buf,
                area,
                cx + 4,
                y,
                &format!("lvl {} {}", i + 1, LEVEL_BANDS[i]),
                Style::default().fg(Color::Gray),
            );
            y += 1;
        }

        // Specials: the free-roaming snake spawned by live-filtered commands.
        y += 1;
        if y < bottom {
            heading(buf, cx, y, "SPECIALS");
            y += 2;
        }
        if y < bottom {
            put(buf, area, cx, y, "●•", Style::default().fg(Color::White));
            put(
                buf,
                area,
                cx + 4,
                y,
                "snake: live cmd",
                Style::default().fg(Color::Gray),
            );
        }
    }

    pub fn render(&mut self, f: &mut Frame) {
        self.area = f.area();
        let area = self.area;
        if area.width < 20 || area.height < 8 {
            return;
        }
        let field = self.field();
        let player_y = self.player_y();
        let buf = f.buffer_mut();

        // Top status line.
        let hearts = "♥".repeat(self.lives as usize);
        let top = format!(
            " ECOTOKENS INVADERS   SCORE {:>8}   {}   WAVE {}   ENEMIES {:>3}   ∿ {:>2}   QUEUE {:>4} ",
            thousands(self.score),
            hearts,
            self.wave,
            self.alive_count(),
            self.snakes.len(),
            self.pending.len(),
        );
        put(
            buf,
            area,
            area.x as i32,
            area.y,
            &top,
            Style::default().fg(Color::Cyan),
        );

        // Enemies: color = family, shape = level; damaged ones are dimmed.
        for e in self.enemies.iter().filter(|e| e.alive) {
            let (x, y) = self.enemy_pos(e);
            if y >= field.y && y < field.y + field.height {
                let mut style = Style::default().fg(e.color);
                if e.hp < e.level {
                    style = style.add_modifier(Modifier::DIM);
                }
                put(buf, area, x, y, SPRITES[(e.level - 1) as usize], style);
            }
        }

        // Free-roaming snakes: a bright head trailing a dimmer body.
        for s in self.snakes.iter().filter(|s| s.alive) {
            let base = Style::default().fg(s.color);
            for (i, &(x, y)) in s.body.iter().enumerate() {
                if y < field.y || y >= field.y + field.height {
                    continue;
                }
                let (glyph, style) = if i == 0 {
                    ("●", base.add_modifier(Modifier::BOLD))
                } else {
                    ("•", base.add_modifier(Modifier::DIM))
                };
                put(buf, area, x as i32, y, glyph, style);
            }
        }

        let now = Instant::now();
        for &(x, y, _) in &self.explosions {
            put(
                buf,
                area,
                x as i32,
                y,
                "✸",
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD),
            );
        }
        for &(x, y) in &self.player_bullets {
            put(
                buf,
                area,
                x as i32,
                y,
                "│",
                Style::default().fg(Color::White),
            );
        }
        for &(x, y) in &self.enemy_bullets {
            put(
                buf,
                area,
                x as i32,
                y,
                "•",
                Style::default().fg(Color::LightRed),
            );
        }

        // Player cannon (blinks while invulnerable).
        let blinking = now < self.invuln_until
            && (self.invuln_until.duration_since(now).as_millis() / 150) % 2 == 0;
        if self.player_placed && !blinking && self.phase != Phase::Empty {
            put(
                buf,
                area,
                self.player_x as i32,
                player_y,
                PLAYER_SPRITE,
                Style::default()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            );
        }

        // Bottom status bar: last destroyed command, or the controls.
        let bar_y = area.y + area.height - 1;
        match &self.last_kill {
            Some((msg, color)) => {
                put(
                    buf,
                    area,
                    area.x as i32 + 1,
                    bar_y,
                    msg,
                    Style::default().fg(*color),
                );
            }
            None => {
                put(
                    buf,
                    area,
                    area.x as i32 + 1,
                    bar_y,
                    "← → move   SPACE fire   p pause   q quit",
                    Style::default().fg(Color::DarkGray),
                );
            }
        }

        // Legend sidebar (drawn under the centered overlays).
        if let Some(bar) = self.sidebar() {
            self.render_sidebar(buf, area, bar);
        }

        // Centered overlays.
        let overlay: Option<Vec<String>> = match self.phase {
            Phase::Paused => Some(vec![
                "PAUSED".to_string(),
                "[p] resume   [q] quit".to_string(),
            ]),
            Phase::GameOver => Some(vec![
                "GAME OVER".to_string(),
                format!("Score: {}", thousands(self.score)),
                "[r] restart   [q] quit".to_string(),
            ]),
            Phase::Victory => Some(vec![
                "VICTORY — every filtered command destroyed!".to_string(),
                format!("Score: {}", thousands(self.score)),
                "[r] restart   [q] quit".to_string(),
            ]),
            Phase::Empty => Some(vec![
                "No filtered commands yet.".to_string(),
                "Enemies spawn from ecotokens interceptions —".to_string(),
                "run some commands through the hooks first.".to_string(),
            ]),
            Phase::Playing => None,
        };
        if let Some(lines) = overlay {
            let cy = area.y + area.height / 2;
            let start = cy.saturating_sub(lines.len() as u16 / 2);
            for (i, line) in lines.iter().enumerate() {
                let x = area.x as i32
                    + (area.width.saturating_sub(line.chars().count() as u16) / 2) as i32;
                put(
                    buf,
                    area,
                    x,
                    start + i as u16,
                    line,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
            }
        }
    }
}

/// Bounds-checked string write into the frame buffer.
fn put(buf: &mut Buffer, area: Rect, x: i32, y: u16, s: &str, style: Style) {
    if y < area.y || y >= area.y + area.height || x >= (area.x + area.width) as i32 {
        return;
    }
    let (x, s) = if x < area.x as i32 {
        let skip = (area.x as i32 - x) as usize;
        let trimmed: String = s.chars().skip(skip).collect();
        (area.x, trimmed)
    } else {
        (x as u16, s.to_string())
    };
    let max_w = (area.x + area.width - x) as usize;
    let clipped: String = s.chars().take(max_w).collect();
    buf.set_string(x, y, clipped, style);
}

/// Game loop: draw, tick, poll keys. New interceptions are reloaded every 10 s
/// on a background thread and delivered over a channel, so the blocking SQLite
/// read (a full table scan pulling every content blob, and up to 5 s of lock
/// contention with the hooks) can never stall the render loop.
pub fn run<B: Backend>(terminal: &mut Terminal<B>, metrics_path: &Path, period: &Period) {
    let all = read_from(metrics_path).unwrap_or_default();
    let known = all.len();
    let mut game = Game::new(filter_by_period(&all, period));
    drop(all); // release the initial content blobs before the loop starts

    // Background reloader: polls the metrics DB and forwards only the rows added
    // since the last read. Kept entirely off the render thread.
    let (tx, rx) = mpsc::channel::<Vec<Interception>>();
    let stop = Arc::new(AtomicBool::new(false));
    let reloader = {
        let path = metrics_path.to_path_buf();
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut known = known;
            let mut last_reload = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                if last_reload.elapsed() >= Duration::from_secs(10) {
                    if let Ok(all) = read_from(&path) {
                        if all.len() > known {
                            let fresh = all[known..].to_vec();
                            known = all.len();
                            if tx.send(fresh).is_err() {
                                break;
                            }
                        }
                    }
                    last_reload = Instant::now();
                }
                // Short sleeps so the stop flag is honoured promptly on quit.
                std::thread::sleep(Duration::from_millis(200));
            }
        })
    };

    loop {
        while let Ok(fresh) = rx.try_recv() {
            game.enqueue(fresh);
        }
        let _ = terminal.draw(|f| game.render(f));
        game.tick();
        if poll(Duration::from_millis(30)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = read() {
                if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
                    continue;
                }
                let quit = matches!(
                    key.code,
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                ) || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL));
                if quit {
                    break;
                }
                game.on_key(key.code);
            }
        }
    }

    // Tear down the reloader before returning so it never outlives the session.
    stop.store(true, Ordering::Relaxed);
    let _ = reloader.join();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::store::HookType;

    fn item(saved: u32) -> Interception {
        Interception {
            id: format!("id-{saved}"),
            timestamp: "2026-07-03T00:00:00Z".to_string(),
            command: format!("cmd-{saved}"),
            command_family: CommandFamily::Git,
            git_root: None,
            tokens_before: saved + 10,
            tokens_after: 10,
            savings_pct: 0.0,
            mode: FilterMode::Filtered,
            redacted: false,
            duration_ms: 1,
            content_before: None,
            content_after: None,
            hook_type: HookType::PreToolUse,
        }
    }

    #[test]
    fn enemy_level_thresholds() {
        assert_eq!(enemy_level(0), 1);
        assert_eq!(enemy_level(999), 1);
        assert_eq!(enemy_level(1000), 2);
        assert_eq!(enemy_level(4999), 2);
        assert_eq!(enemy_level(5000), 3);
        assert_eq!(enemy_level(19999), 3);
        assert_eq!(enemy_level(20000), 4);
    }

    #[test]
    fn passthrough_items_do_not_spawn_enemies() {
        let mut passthrough = item(500);
        passthrough.mode = FilterMode::Passthrough;
        let game = Game::new(vec![item(100), passthrough]);
        assert_eq!(game.pending.len(), 1);
    }

    #[test]
    fn spawn_wave_puts_strongest_on_top_row() {
        let mut game = Game::new(vec![item(50), item(30_000), item(200)]);
        let field = Rect::new(0, 1, 80, 20);
        assert!(game.spawn_wave(field));
        assert_eq!(game.enemies.len(), 3);
        let top = &game.enemies[0];
        assert_eq!(top.row, 0);
        assert_eq!(top.level, 4);
        assert_eq!(top.saved, 30_000);
    }

    #[test]
    fn live_enqueue_during_play_spawns_a_snake() {
        let mut game = Game::new(vec![item(100)]);
        game.area = Rect::new(0, 0, 80, 24);
        assert!(game.phase == Phase::Playing);
        assert_eq!(game.pending.len(), 1); // the startup command awaits its wave
        game.enqueue(vec![item(30_000)]);
        assert_eq!(game.snakes.len(), 1);
        // The live command becomes a snake, not another queued formation enemy.
        assert_eq!(game.pending.len(), 1);
        let snake = &game.snakes[0];
        assert_eq!(snake.level, 4);
        assert!(snake.body.iter().all(|&(x, y)| x < 80 && y < 24));
    }

    #[test]
    fn snake_bounces_and_stays_within_bounds() {
        let mut game = Game::new(vec![item(100)]);
        game.area = Rect::new(0, 0, 80, 24);
        game.enqueue(vec![item(100)]);
        let bounds = game.group_bounds();
        for _ in 0..200 {
            game.step_snakes();
            for &(x, y) in &game.snakes[0].body {
                assert!(x >= bounds.x && x < bounds.x + bounds.width);
                assert!(y >= bounds.y && y < bounds.y + bounds.height);
            }
        }
    }

    #[test]
    fn wave_exhaustion_returns_false() {
        let mut game = Game::new(vec![item(100)]);
        let field = Rect::new(0, 1, 80, 20);
        assert!(game.spawn_wave(field));
        assert!(!game.spawn_wave(field));
    }
}
