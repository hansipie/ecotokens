//! Full-screen setup assistant for `ecotokens install` (no option, on a terminal).
//!
//! Three steps: pick the harnesses, switch the features they support (and pick
//! their models, the Jev key, the embedding provider), review a summary.
//! Nothing is written here: [`run`] returns [`Exit::Apply`] with the plan, or
//! [`Exit::Cancel`]. The state machine ([`InstallWizard::on_key`]) is separate
//! from drawing so both are testable without a real terminal.

use std::io;
use std::time::Duration;

use ratatui::backend::Backend;
use ratatui::crossterm::event::{poll, read, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::install_wizard::{
    changes, AiSummary, Change, Choices, EmbedChoice, Feature, Harness, KeySource, OllamaStatus,
    WizardContext, WizardPlan, DEFAULT_CANDLE_EMBED_MODEL,
};

const ACCENT: Color = Color::Green;
const INFO: Color = Color::Cyan;
const WARN: Color = Color::Yellow;
const BAD: Color = Color::Red;
const MUTED: Color = Color::DarkGray;
const VALUE: Color = Color::Magenta;
const CURSOR_BG: Color = Color::Rgb(38, 50, 56);
/// Widest the screen is drawn; wider terminals get it centred.
const MAX_WIDTH: u16 = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Harnesses,
    Features,
    Summary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    Apply(WizardPlan),
    Cancel,
}

/// The text field being typed into, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    AiModel,
    RewriteModel,
    EmbedModel,
    /// Masked: what is typed is never drawn.
    JevKey,
}

pub struct InstallWizard {
    ctx: WizardContext,
    step: Step,
    selected: [bool; 7],
    harness_cursor: usize,
    /// One switch per [`Feature::ALL`] item.
    switches: [bool; 9],
    /// Index into [`InstallWizard::offered`].
    feature_cursor: usize,
    choices: Choices,
    editing: Option<Field>,
    /// Key being typed; moved to `choices.jev_api_key` on Enter.
    key_input: String,
    notice: Option<String>,
    exit: Option<Exit>,
}

fn feature_index(f: Feature) -> usize {
    Feature::ALL.iter().position(|x| *x == f).unwrap_or(0)
}

/// Next item after `current` in `list`, wrapping; the first one when
/// `current` is not in it.
fn cycle(list: &[String], current: &str) -> Option<String> {
    if list.is_empty() {
        return None;
    }
    let next = list
        .iter()
        .position(|m| m == current)
        .map_or(0, |i| (i + 1) % list.len());
    Some(list[next].clone())
}

impl InstallWizard {
    pub fn new(ctx: WizardContext) -> Self {
        let defaults = ctx.default_harnesses();
        let selected = Harness::ALL.map(|h| defaults.contains(&h));
        let switches = Feature::ALL.map(|f| f.proposed(&ctx));
        let choices = ctx.choices();
        InstallWizard {
            ctx,
            step: Step::Harnesses,
            selected,
            harness_cursor: 0,
            switches,
            feature_cursor: 0,
            choices,
            editing: None,
            key_input: String::new(),
            notice: None,
            exit: None,
        }
    }

    #[cfg(test)]
    pub fn step(&self) -> Step {
        self.step
    }

    #[cfg(test)]
    pub fn exit(&self) -> Option<&Exit> {
        self.exit.as_ref()
    }

    pub fn chosen(&self) -> Vec<Harness> {
        Harness::ALL
            .iter()
            .zip(self.selected)
            .filter(|(_, on)| *on)
            .map(|(h, _)| *h)
            .collect()
    }

    /// Features that apply to the chosen harnesses, in display order. Line
    /// selection only shows while Jev is ticked: it does nothing otherwise.
    pub fn offered(&self) -> Vec<Feature> {
        let chosen = self.chosen();
        Feature::ALL
            .into_iter()
            .filter(|f| !f.harnesses(&chosen).is_empty())
            .filter(|f| *f != Feature::JevLineSelect || self.is_on(Feature::Jev))
            .collect()
    }

    fn is_on(&self, f: Feature) -> bool {
        self.switches[feature_index(f)]
    }

    fn set(&mut self, f: Feature, on: bool) {
        self.switches[feature_index(f)] = on;
    }

    pub fn plan(&self) -> WizardPlan {
        WizardPlan::from_choices(&self.ctx, self.chosen(), |f| self.is_on(f), &self.choices)
    }

    fn focused_feature(&self) -> Option<Feature> {
        self.offered().get(self.feature_cursor).copied()
    }

    fn has_key(&self) -> bool {
        self.ctx.jev_key != KeySource::Missing || self.choices.jev_api_key.is_some()
    }

    /// `m` on a model row: move to the next pulled model of the right kind.
    fn next_model(&mut self, f: Feature) {
        match f {
            Feature::AiSummary => {
                if let Some(m) = cycle(&self.ctx.ollama.text_models(), &self.choices.ai_model) {
                    self.choices.ai_model = m;
                    self.set(Feature::AiSummary, true);
                }
            }
            Feature::Rewrite => {
                // "Same as AI summary" first, then each text model, then back.
                let models = self.ctx.ollama.text_models();
                if models.is_empty() {
                    return;
                }
                self.choices.rewrite_model = match self.choices.rewrite_model.as_deref() {
                    None => Some(models[0].clone()),
                    Some(cur) => match models.iter().position(|m| m == cur) {
                        Some(i) if i + 1 < models.len() => Some(models[i + 1].clone()),
                        Some(_) => None,
                        None => Some(models[0].clone()),
                    },
                };
            }
            Feature::Embeddings => {
                let current = match &self.choices.embed {
                    EmbedChoice::Ollama(m) => m.clone(),
                    _ => String::new(),
                };
                if let Some(m) = cycle(&self.ctx.ollama.embedding_models(), &current) {
                    self.choices.embed = EmbedChoice::Ollama(m);
                    self.set(Feature::Embeddings, true);
                }
            }
            _ => {}
        }
    }

    /// `p` on semantic search: Candle ↔ Ollama.
    fn toggle_provider(&mut self) {
        self.choices.embed = match &self.choices.embed {
            EmbedChoice::Ollama(_) => match &self.ctx.embed {
                EmbedChoice::Candle(m) => EmbedChoice::Candle(m.clone()),
                _ => EmbedChoice::Candle(DEFAULT_CANDLE_EMBED_MODEL.to_string()),
            },
            _ => EmbedChoice::Ollama(match &self.ctx.embed {
                EmbedChoice::Ollama(m) => m.clone(),
                _ => self.ctx.ollama_embed_model(),
            }),
        };
        self.set(Feature::Embeddings, true);
    }

    /// The text buffer of a model field.
    fn field_text(&mut self, field: Field) -> Option<&mut String> {
        match field {
            Field::AiModel => Some(&mut self.choices.ai_model),
            Field::RewriteModel => Some(self.choices.rewrite_model.get_or_insert_with(String::new)),
            Field::EmbedModel => match &mut self.choices.embed {
                EmbedChoice::Candle(m) | EmbedChoice::Ollama(m) => Some(m),
                EmbedChoice::None => None,
            },
            Field::JevKey => Some(&mut self.key_input),
        }
    }

    fn start_editing(&mut self, f: Feature) {
        let field = match f {
            Feature::AiSummary => {
                self.set(Feature::AiSummary, true);
                Field::AiModel
            }
            Feature::Rewrite => Field::RewriteModel,
            Feature::Embeddings => {
                if self.choices.embed == EmbedChoice::None {
                    self.choices.embed = self.ctx.embed_start();
                }
                self.set(Feature::Embeddings, true);
                Field::EmbedModel
            }
            Feature::Jev => {
                self.key_input.clear();
                Field::JevKey
            }
            _ => return,
        };
        self.editing = Some(field);
    }

    fn finish_editing(&mut self, keep: bool) {
        match self.editing.take() {
            Some(Field::JevKey) => {
                let key = std::mem::take(&mut self.key_input);
                if keep && !key.trim().is_empty() {
                    self.choices.jev_api_key = Some(key.trim().to_string());
                    self.set(Feature::Jev, true);
                }
            }
            // An empty field means "same as AI summary".
            Some(Field::RewriteModel)
                if self
                    .choices
                    .rewrite_model
                    .as_deref()
                    .is_some_and(|m| m.trim().is_empty()) =>
            {
                self.choices.rewrite_model = None;
            }
            _ => {}
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.exit = Some(Exit::Cancel);
            return;
        }
        self.notice = None;

        if let Some(field) = self.editing {
            match key.code {
                KeyCode::Char(c) => {
                    if let Some(t) = self.field_text(field) {
                        t.push(c);
                    }
                }
                KeyCode::Backspace => {
                    if let Some(t) = self.field_text(field) {
                        t.pop();
                    }
                }
                KeyCode::Enter => self.finish_editing(true),
                KeyCode::Esc => self.finish_editing(field != Field::JevKey),
                _ => {}
            }
            return;
        }

        let up = matches!(key.code, KeyCode::Up | KeyCode::Char('k'));
        let down = matches!(key.code, KeyCode::Down | KeyCode::Char('j'));
        let toggle = matches!(key.code, KeyCode::Char(' ') | KeyCode::Char('x'));
        let next = matches!(key.code, KeyCode::Enter | KeyCode::Right | KeyCode::Tab);
        let back = matches!(key.code, KeyCode::Esc | KeyCode::Left | KeyCode::BackTab);
        let quit = matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'));

        match self.step {
            Step::Harnesses => {
                let n = Harness::ALL.len();
                if up {
                    self.harness_cursor = (self.harness_cursor + n - 1) % n;
                } else if down {
                    self.harness_cursor = (self.harness_cursor + 1) % n;
                } else if toggle {
                    self.selected[self.harness_cursor] ^= true;
                } else if key.code == KeyCode::Char('a') {
                    let all = self.selected.iter().all(|s| *s);
                    self.selected = [!all; 7];
                } else if next {
                    if self.chosen().is_empty() {
                        self.notice = Some("Select at least one harness (space).".into());
                    } else {
                        self.step = Step::Features;
                        self.feature_cursor = 0;
                    }
                } else if back || quit {
                    self.exit = Some(Exit::Cancel);
                }
            }
            Step::Features => {
                let n = self.offered().len().max(1);
                let focused = self.focused_feature();
                if up {
                    self.feature_cursor = (self.feature_cursor + n - 1) % n;
                } else if down {
                    self.feature_cursor = (self.feature_cursor + 1) % n;
                } else if toggle {
                    if let Some(f) = focused {
                        self.switches[feature_index(f)] ^= true;
                        // Hiding line selection must not leave the cursor past the end.
                        let len = self.offered().len().max(1);
                        self.feature_cursor = self.feature_cursor.min(len - 1);
                    }
                } else if key.code == KeyCode::Char('e') {
                    if let Some(f) = focused {
                        self.start_editing(f);
                    }
                } else if key.code == KeyCode::Char('m') {
                    if let Some(f) = focused {
                        self.next_model(f);
                    }
                } else if key.code == KeyCode::Char('p') && focused == Some(Feature::Embeddings) {
                    self.toggle_provider();
                } else if next {
                    self.step = Step::Summary;
                } else if back {
                    self.step = Step::Harnesses;
                } else if quit {
                    self.exit = Some(Exit::Cancel);
                }
            }
            Step::Summary => {
                if next || key.code == KeyCode::Char('y') {
                    self.exit = Some(Exit::Apply(self.plan()));
                } else if back {
                    self.step = Step::Features;
                } else if quit || key.code == KeyCode::Char('n') {
                    self.exit = Some(Exit::Cancel);
                }
            }
        }
    }

    // ── Drawing ─────────────────────────────────────────────────────────────

    pub fn render(&self, f: &mut Frame) {
        let area = f.area();
        let width = area.width.min(MAX_WIDTH);
        let area = Rect {
            x: area.x + (area.width - width) / 2,
            width,
            ..area
        };
        let [header, steps, body, detail, footer] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(6),
            Constraint::Length(1),
        ])
        .areas(area);

        self.render_header(f, header);
        self.render_steps(f, steps);
        match self.step {
            Step::Harnesses => self.render_harnesses(f, body),
            Step::Features => self.render_features(f, body),
            Step::Summary => self.render_summary(f, body),
        }
        self.render_detail(f, detail);
        self.render_footer(f, footer);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT));
        let line = Line::from(vec![
            Span::styled(" ✦ ", Style::default().fg(ACCENT)),
            Span::styled(
                "ecotokens",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  setup assistant",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("   v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(MUTED),
            ),
        ]);
        f.render_widget(Paragraph::new(line).block(block), area);
    }

    fn render_steps(&self, f: &mut Frame, area: Rect) {
        let order = [Step::Harnesses, Step::Features, Step::Summary];
        let names = ["Harnesses", "Features", "Summary"];
        let current = order.iter().position(|s| *s == self.step).unwrap_or(0);
        let mut spans = vec![Span::raw(" ")];
        for (i, name) in names.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" ─── ", Style::default().fg(MUTED)));
            }
            let span = if i == current {
                Span::styled(
                    format!(" {} {name} ", i + 1),
                    Style::default()
                        .fg(Color::Black)
                        .bg(ACCENT)
                        .add_modifier(Modifier::BOLD),
                )
            } else if i < current {
                Span::styled(format!("✔ {name}"), Style::default().fg(ACCENT))
            } else {
                Span::styled(format!("{} {name}", i + 1), Style::default().fg(MUTED))
            };
            spans.push(span);
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn panel(title: &str) -> Block<'_> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(MUTED))
            .title(Span::styled(
                format!(" {title} "),
                Style::default().fg(INFO).add_modifier(Modifier::BOLD),
            ))
    }

    fn checkbox(on: bool) -> Span<'static> {
        if on {
            Span::styled(
                "[✔]",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("[ ]", Style::default().fg(MUTED))
        }
    }

    fn row(focused: bool, mut spans: Vec<Span<'static>>) -> Line<'static> {
        let pointer = if focused {
            Span::styled(
                " ❯ ",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("   ")
        };
        spans.insert(0, pointer);
        let line = Line::from(spans);
        if focused {
            line.style(Style::default().bg(CURSOR_BG))
        } else {
            line
        }
    }

    fn badge(text: &str, bg: Color) -> Span<'static> {
        Span::styled(
            format!(" {text} "),
            Style::default()
                .fg(Color::Black)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )
    }

    /// Short Ollama state for the feature rows: text and colour.
    fn ollama_badge(&self) -> (&'static str, Color) {
        match &self.ctx.ollama {
            OllamaStatus::Running { models } if models.is_empty() => ("Ollama: no model", WARN),
            OllamaStatus::Running { .. } => ("Ollama ready", ACCENT),
            OllamaStatus::Stopped => ("Ollama stopped", WARN),
            OllamaStatus::Missing => ("Ollama not found", BAD),
        }
    }

    fn key_badge(&self) -> (&'static str, Color) {
        if self.choices.jev_api_key.is_some() {
            return ("key entered", ACCENT);
        }
        match self.ctx.jev_key {
            KeySource::Shell => ("key: shell", ACCENT),
            KeySource::EnvFile => ("key: .env", ACCENT),
            KeySource::Missing => ("no key", BAD),
        }
    }

    /// A value shown after the badges; drawn as an input while it is edited.
    fn value(&self, text: String, field: Field) -> Span<'static> {
        if self.editing == Some(field) {
            Span::styled(
                format!("  {text}▏"),
                Style::default().fg(WARN).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(format!("  {text}"), Style::default().fg(VALUE))
        }
    }

    fn render_harnesses(&self, f: &mut Frame, area: Rect) {
        let mut lines = vec![Line::raw("")];
        for (i, h) in Harness::ALL.iter().enumerate() {
            let st = self.ctx.harnesses[i];
            let focused = i == self.harness_cursor;
            let label_style = if self.selected[i] {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let mut spans = vec![
                Self::checkbox(self.selected[i]),
                Span::raw(" "),
                Span::styled(format!("{:<14}", h.label()), label_style),
            ];
            if st.installed {
                spans.push(Self::badge("installed", ACCENT));
            } else if st.detected {
                spans.push(Self::badge("detected", INFO));
            }
            lines.push(Self::row(focused, spans));
        }
        let count = self.chosen().len();
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("   {count} selected"),
            Style::default().fg(MUTED),
        )));
        f.render_widget(
            Paragraph::new(lines).block(Self::panel("Where should ecotokens be installed?")),
            area,
        );
    }

    fn feature_spans(&self, feat: Feature, on: bool) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        match feat {
            Feature::Jev => {
                let (t, c) = self.key_badge();
                spans.push(Self::badge(t, c));
                if self.editing == Some(Field::JevKey) {
                    let dots = "•".repeat(self.key_input.chars().count().min(32));
                    spans.push(self.value(format!("key: {dots}"), Field::JevKey));
                }
            }
            Feature::JevLineSelect => {}
            Feature::AiSummary => {
                let (t, c) = self.ollama_badge();
                spans.push(Self::badge(t, c));
                if on {
                    spans.push(
                        self.value(format!("model: {}", self.choices.ai_model), Field::AiModel),
                    );
                }
            }
            Feature::Rewrite => {
                let (t, c) = self.ollama_badge();
                spans.push(Self::badge(t, c));
                let text = match self.choices.rewrite_model.as_deref() {
                    Some(m) if self.editing == Some(Field::RewriteModel) || !m.is_empty() => {
                        format!("model: {m}")
                    }
                    _ => format!(
                        "model: same as AI summary ({})",
                        self.choices.rewrite_model_used()
                    ),
                };
                spans.push(self.value(text, Field::RewriteModel));
            }
            Feature::Embeddings => {
                if on {
                    if let EmbedChoice::Ollama(_) = self.choices.embed {
                        let (t, c) = self.ollama_badge();
                        spans.push(Self::badge(t, c));
                    } else {
                        spans.push(Self::badge("local", ACCENT));
                    }
                    spans.push(self.value(self.choices.embed.describe(), Field::EmbedModel));
                } else {
                    spans.push(Span::styled("  BM25 only", Style::default().fg(MUTED)));
                }
            }
            _ => {
                let names: Vec<&str> = feat
                    .harnesses(&self.chosen())
                    .iter()
                    .map(|h| h.label())
                    .collect();
                spans.push(Span::styled(names.join(", "), Style::default().fg(MUTED)));
            }
        }
        if on && feat.warning(&self.ctx, &self.choices).is_some() {
            spans.push(Span::styled("  ⚠", Style::default().fg(WARN)));
        }
        spans
    }

    fn render_features(&self, f: &mut Frame, area: Rect) {
        let mut lines = vec![Line::raw("")];
        for (i, feat) in self.offered().into_iter().enumerate() {
            let on = self.is_on(feat);
            let focused = i == self.feature_cursor;
            let label_style = if on {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let indent = if feat == Feature::JevLineSelect {
                "└ "
            } else {
                ""
            };
            let mut spans = vec![
                Self::checkbox(on),
                Span::raw(" "),
                Span::styled(
                    format!("{:<23}", format!("{indent}{}", feat.label())),
                    label_style,
                ),
            ];
            spans.extend(self.feature_spans(feat, on));
            lines.push(Self::row(focused, spans));
        }
        f.render_widget(
            Paragraph::new(lines).block(Self::panel("Which features do you want?")),
            area,
        );
    }

    fn render_summary(&self, f: &mut Frame, area: Rect) {
        let plan = self.plan();
        let key = |k: &str| Span::styled(format!("   {k:<23}"), Style::default().fg(INFO));
        let names: Vec<&str> = plan.harnesses.iter().map(|h| h.label()).collect();
        let mut lines = vec![
            Line::raw(""),
            Line::from(vec![
                key("Install for"),
                Span::styled(
                    names.join(", "),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::raw(""),
        ];
        for row in changes(&self.ctx, &plan) {
            let (text, style) = match row.change {
                Change::TurnOn => (
                    "● turn on",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Change::TurnOff => (
                    "○ turn off",
                    Style::default().fg(BAD).add_modifier(Modifier::BOLD),
                ),
                Change::Switch => (
                    "● change",
                    Style::default().fg(INFO).add_modifier(Modifier::BOLD),
                ),
                Change::StaysOn => ("● on (unchanged)", Style::default().fg(ACCENT)),
                Change::StaysOff => ("○ off (unchanged)", Style::default().fg(MUTED)),
            };
            let mut spans = vec![key(row.label), Span::styled(text, style)];
            if let Some(d) = row.detail {
                spans.push(Span::styled(format!("  {d}"), Style::default().fg(VALUE)));
            }
            lines.push(Line::from(spans));
        }
        f.render_widget(
            Paragraph::new(lines).block(Self::panel("Ready to install")),
            area,
        );
    }

    fn warn_line(text: String) -> Line<'static> {
        Line::from(Span::styled(format!("⚠ {text}"), Style::default().fg(WARN)))
    }

    fn ok_line(text: String) -> Line<'static> {
        Line::from(Span::styled(
            format!("✔ {text}"),
            Style::default().fg(ACCENT),
        ))
    }

    /// Extra status lines for the focused feature, under its description.
    fn feature_status(&self, feat: Feature) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        match feat {
            Feature::Jev => {
                let env = "~/.config/ecotokens/.env";
                match (self.choices.jev_api_key.is_some(), self.ctx.jev_key) {
                    (true, _) => lines.push(Self::ok_line(format!(
                        "The key will be saved to {env} (mode 600)."
                    ))),
                    (false, KeySource::Shell) => lines.push(Self::ok_line(
                        "Key found in your shell environment (press e to save another one).".into(),
                    )),
                    (false, KeySource::EnvFile) => lines.push(Self::ok_line(format!(
                        "Key found in {env} (press e to replace it)."
                    ))),
                    (false, KeySource::Missing) => {}
                }
            }
            Feature::AiSummary | Feature::Rewrite => {
                let pulled = self.ctx.ollama.text_models();
                if !pulled.is_empty() {
                    lines.push(Self::ok_line(format!(
                        "Ollama text models: {}",
                        pulled.join(", ")
                    )));
                }
            }
            Feature::Embeddings => {
                let pulled = self.ctx.ollama.embedding_models();
                if !pulled.is_empty() {
                    lines.push(Self::ok_line(format!(
                        "Ollama embedding models: {}",
                        pulled.join(", ")
                    )));
                }
                if self.plan().embed.is_some() {
                    lines.push(Self::warn_line(
                        "A new embedding setting needs `ecotokens index` again in your projects."
                            .into(),
                    ));
                }
            }
            _ => {}
        }
        if let Some(w) = feat.warning(&self.ctx, &self.choices) {
            lines.push(Self::warn_line(w));
        }
        lines
    }

    fn render_detail(&self, f: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();
        if let Some(n) = &self.notice {
            lines.push(Line::from(Span::styled(
                format!("✖ {n}"),
                Style::default().fg(BAD).add_modifier(Modifier::BOLD),
            )));
        }
        let title = |t: &str| {
            Span::styled(
                format!("{t}  "),
                Style::default().fg(INFO).add_modifier(Modifier::BOLD),
            )
        };
        match self.step {
            Step::Harnesses => {
                let h = Harness::ALL[self.harness_cursor];
                lines.push(Line::from(vec![
                    title(h.label()),
                    Span::raw(format!("installs {}", h.installs())),
                ]));
            }
            Step::Features => {
                if self.editing == Some(Field::JevKey) {
                    lines.push(Line::from(vec![
                        title("Jev API key"),
                        Span::raw(
                            "Paste or type the key; it is not shown. Enter saves, Esc cancels.",
                        ),
                    ]));
                } else if let Some(feat) = self.focused_feature() {
                    lines.push(Line::from(vec![
                        title(feat.label()),
                        Span::raw(feat.description()),
                    ]));
                    lines.extend(self.feature_status(feat));
                }
            }
            Step::Summary => {
                lines.push(Line::raw("Nothing is written until you press Enter."));
                let plan = self.plan();
                if plan.router == Some(true) {
                    lines.push(Self::warn_line(
                        "With the router on, each message is sent (secrets masked) to TypeSafe."
                            .into(),
                    ));
                }
                if !self.has_key() && (plan.router == Some(true) || plan.jev == Some(true)) {
                    lines.push(Self::warn_line(format!(
                        "{} is not set: go back and press e on Jev to enter it.",
                        crate::jev::API_KEY_ENV
                    )));
                }
                if matches!(plan.ai_summary, Some(AiSummary::On(_))) {
                    if let Some(w) = Feature::AiSummary.warning(&self.ctx, &self.choices) {
                        lines.push(Self::warn_line(format!("AI summary: {w}")));
                    }
                }
                if plan.embed.is_some() {
                    lines.push(Self::warn_line(
                        "Semantic search changes: run `ecotokens index` again in your projects."
                            .into(),
                    ));
                }
            }
        }
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(MUTED))
            .padding(Padding::horizontal(1));
        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }).block(block),
            area,
        );
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let mut hints: Vec<(&str, &str)> = Vec::new();
        if let Some(field) = self.editing {
            hints.push((
                "type",
                if field == Field::JevKey {
                    "the key"
                } else {
                    "model name"
                },
            ));
            hints.push(("⌫", "erase"));
            hints.push(("enter", "done"));
            if field == Field::JevKey {
                hints.push(("esc", "cancel"));
            }
        } else {
            match self.step {
                Step::Harnesses => hints.extend([
                    ("↑↓", "move"),
                    ("space", "toggle"),
                    ("a", "all"),
                    ("enter", "next"),
                    ("q", "quit"),
                ]),
                Step::Features => {
                    hints.extend([("↑↓", "move"), ("space", "toggle")]);
                    match self.focused_feature() {
                        Some(Feature::Jev) => hints.push(("e", "enter key")),
                        Some(Feature::AiSummary) | Some(Feature::Rewrite) => {
                            if !self.ctx.ollama.text_models().is_empty() {
                                hints.push(("m", "next model"));
                            }
                            hints.push(("e", "edit model"));
                        }
                        Some(Feature::Embeddings) => {
                            hints.push(("p", "provider"));
                            if !self.ctx.ollama.embedding_models().is_empty() {
                                hints.push(("m", "next model"));
                            }
                            hints.push(("e", "edit model"));
                        }
                        _ => {}
                    }
                    hints.extend([("enter", "next"), ("esc", "back"), ("q", "quit")]);
                }
                Step::Summary => {
                    hints.extend([("enter", "install"), ("esc", "back"), ("q", "quit")])
                }
            }
        }
        let mut spans = vec![Span::raw(" ")];
        for (k, what) in hints {
            spans.push(Span::styled(
                format!(" {k} "),
                Style::default().fg(Color::Black).bg(INFO),
            ));
            spans.push(Span::styled(
                format!(" {what}  "),
                Style::default().fg(MUTED),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

/// Drive the assistant until the user applies or cancels.
pub fn run<B: Backend>(terminal: &mut Terminal<B>, ctx: WizardContext) -> io::Result<Exit> {
    let mut wizard = InstallWizard::new(ctx);
    loop {
        terminal.draw(|f| wizard.render(f))?;
        if poll(Duration::from_millis(200))? {
            match read()? {
                Event::Key(key)
                    if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat =>
                {
                    wizard.on_key(key);
                }
                // A pasted key arrives as one event when bracketed paste is on.
                Event::Paste(text) => {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        wizard.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
                    }
                }
                _ => {}
            }
        }
        if let Some(exit) = wizard.exit.take() {
            return Ok(exit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn ctx() -> WizardContext {
        let mut c = WizardContext {
            jev_key: KeySource::EnvFile,
            ..Default::default()
        };
        c.harnesses[0].installed = true; // Claude Code
        c.harnesses[4].detected = true; // Hermes
        c
    }

    fn press(w: &mut InstallWizard, keys: &[KeyCode]) {
        for k in keys {
            w.on_key(KeyEvent::new(*k, KeyModifiers::NONE));
        }
    }

    fn type_text(w: &mut InstallWizard, text: &str) {
        for c in text.chars() {
            press(w, &[KeyCode::Char(c)]);
        }
    }

    /// Move the cursor to `f` on the features screen.
    fn focus(w: &mut InstallWizard, f: Feature) {
        let i = w.offered().iter().position(|x| *x == f).expect("offered");
        w.feature_cursor = i;
    }

    fn screen(w: &InstallWizard) -> String {
        let mut t = Terminal::new(TestBackend::new(110, 34)).unwrap();
        t.draw(|f| w.render(f)).unwrap();
        t.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .chunks(110)
            .map(|row| row.concat())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn applied(w: &InstallWizard) -> WizardPlan {
        match w.exit() {
            Some(Exit::Apply(plan)) => plan.clone(),
            other => panic!("expected a plan, got {other:?}"),
        }
    }

    fn with_ollama(models: &[&str]) -> WizardContext {
        let mut c = ctx();
        c.ollama = OllamaStatus::Running {
            models: models.iter().map(|m| m.to_string()).collect(),
        };
        c
    }

    use KeyCode::*;

    #[test]
    fn starts_with_detected_and_installed_harnesses_selected() {
        let w = InstallWizard::new(ctx());
        assert_eq!(w.chosen(), vec![Harness::Claude, Harness::Hermes]);
        let s = screen(&w);
        assert!(s.contains("Where should ecotokens be installed?"), "{s}");
        assert!(s.contains(" installed "), "{s}");
        assert!(s.contains(" detected "), "{s}");
        assert!(s.contains("2 selected"), "{s}");
    }

    #[test]
    fn full_flow_produces_the_plan() {
        let mut w = InstallWizard::new(ctx());
        press(&mut w, &[Enter]);
        assert_eq!(w.step(), Step::Features);
        // Claude + Hermes: auto-watch, router, handoff, hermes plugin, Jev,
        // AI summary, rewrite, semantic search (line selection hidden: Jev off).
        assert_eq!(w.offered().len(), 8);
        focus(&mut w, Feature::AutoWatch);
        press(&mut w, &[Char(' ')]);
        focus(&mut w, Feature::Handoff);
        press(&mut w, &[Char(' ')]);
        focus(&mut w, Feature::AiSummary);
        press(&mut w, &[Char('e')]);
        type_text(&mut w, "qwen3");
        press(&mut w, &[Enter, Enter]);
        assert_eq!(w.step(), Step::Summary);
        assert!(screen(&w).contains("● turn on"));
        press(&mut w, &[Enter]);

        let plan = applied(&w);
        assert_eq!(plan.harnesses, vec![Harness::Claude, Harness::Hermes]);
        assert_eq!(plan.auto_watch, Some(true));
        assert_eq!(plan.router, Some(false));
        assert_eq!(plan.handoff, Some(true));
        assert!(!plan.hermes_enable_plugin);
        assert_eq!(
            plan.ai_summary,
            Some(AiSummary::On("llama3.2:latestqwen3".into())),
            "typing appends to the proposed model"
        );
        assert_eq!(plan.embed, None, "semantic search untouched");
    }

    #[test]
    fn model_can_be_erased_and_retyped() {
        let mut w = InstallWizard::new(ctx());
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::AiSummary);
        press(&mut w, &[Char('e')]);
        for _ in 0.."llama3.2:latest".len() {
            press(&mut w, &[Backspace]);
        }
        type_text(&mut w, "mistral");
        assert!(screen(&w).contains("model: mistral▏"));
        press(&mut w, &[Enter, Enter, Enter]);
        assert_eq!(
            applied(&w).ai_summary,
            Some(AiSummary::On("mistral".into()))
        );
    }

    #[test]
    fn gemini_alone_is_offered_only_the_global_features() {
        let mut w = InstallWizard::new(WizardContext::default());
        press(&mut w, &[Char(' '), Down, Char(' '), Enter]);
        assert_eq!(w.chosen(), vec![Harness::Gemini]);
        assert_eq!(
            w.offered(),
            vec![
                Feature::Jev,
                Feature::AiSummary,
                Feature::Rewrite,
                Feature::Embeddings
            ]
        );
        let s = screen(&w);
        assert!(!s.contains("Auto-watch"), "{s}");
        assert!(s.contains(" Ollama not found "), "{s}");
        assert!(s.contains(" no key "), "{s}");
    }

    #[test]
    fn empty_selection_is_refused_with_a_notice() {
        let mut w = InstallWizard::new(ctx());
        press(&mut w, &[Char('a'), Char('a')]);
        assert!(w.chosen().is_empty());
        press(&mut w, &[Enter]);
        assert_eq!(w.step(), Step::Harnesses);
        assert!(screen(&w).contains("Select at least one harness"));
    }

    #[test]
    fn navigation_back_and_cancel() {
        let mut w = InstallWizard::new(ctx());
        press(&mut w, &[Enter, Enter, Esc]);
        assert_eq!(w.step(), Step::Features);
        press(&mut w, &[Esc]);
        assert_eq!(w.step(), Step::Harnesses);
        press(&mut w, &[Char('q')]);
        assert_eq!(w.exit(), Some(&Exit::Cancel));

        let mut w = InstallWizard::new(ctx());
        w.on_key(KeyEvent::new(Char('c'), KeyModifiers::CONTROL));
        assert_eq!(w.exit(), Some(&Exit::Cancel));
    }

    #[test]
    fn letters_typed_in_a_field_are_not_commands() {
        let mut w = InstallWizard::new(ctx());
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::AiSummary);
        press(&mut w, &[Char('e'), Char('q'), Char('j'), Char('m')]);
        assert!(w.exit().is_none());
        assert!(w.choices.ai_model.ends_with("qjm"));
    }

    // ── Jev ─────────────────────────────────────────────────────────────────

    #[test]
    fn jev_key_is_entered_masked_and_turns_jev_on() {
        let mut c = ctx();
        c.jev_key = KeySource::Missing;
        let mut w = InstallWizard::new(c);
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::Router);
        press(&mut w, &[Char(' ')]);
        assert!(screen(&w).contains("TYPESAFE_API_KEY is not set"));

        focus(&mut w, Feature::Jev);
        assert!(screen(&w).contains(" no key "));
        press(&mut w, &[Char('e')]);
        type_text(&mut w, "ts_live_SECRET42");
        let s = screen(&w);
        assert!(!s.contains("SECRET42"), "the key is never drawn: {s}");
        assert!(s.contains("key: ••••••••••••••••▏"), "{s}");
        press(&mut w, &[Enter]);
        assert!(w.is_on(Feature::Jev));
        let s = screen(&w);
        assert!(s.contains(" key entered "), "{s}");
        assert!(
            s.contains("saved to ~/.config/ecotokens/.env (mode 600)"),
            "{s}"
        );
        assert!(!s.contains("SECRET42"), "{s}");

        // Line selection appears under Jev once it is on.
        assert!(w.offered().contains(&Feature::JevLineSelect));
        press(&mut w, &[Enter]); // summary
        let s = screen(&w);
        assert!(s.contains("Jev API key"), "{s}");
        assert!(!s.contains("SECRET42"), "{s}");
        assert!(!s.contains("TYPESAFE_API_KEY is not set"), "{s}");
        press(&mut w, &[Enter]);
        let plan = applied(&w);
        assert_eq!(plan.jev_api_key.as_deref(), Some("ts_live_SECRET42"));
        assert_eq!(plan.jev, Some(true));
        assert_eq!(plan.router, Some(true));
    }

    #[test]
    fn esc_while_typing_the_key_discards_it() {
        let mut w = InstallWizard::new(ctx());
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::Jev);
        press(&mut w, &[Char('e')]);
        type_text(&mut w, "oops");
        press(&mut w, &[Esc]);
        assert_eq!(w.choices.jev_api_key, None);
        assert!(w.key_input.is_empty());
        assert_eq!(w.step(), Step::Features, "Esc left the field, not the step");
    }

    #[test]
    fn line_selection_hides_with_jev_and_keeps_the_cursor_in_range() {
        let mut c = ctx();
        c.jev_enabled = true;
        let mut w = InstallWizard::new(c);
        press(&mut w, &[Enter]);
        assert!(w.offered().contains(&Feature::JevLineSelect));
        focus(&mut w, Feature::Jev);
        press(&mut w, &[Char(' ')]);
        assert!(!w.offered().contains(&Feature::JevLineSelect));
        assert!(w.feature_cursor < w.offered().len());
    }

    #[test]
    fn existing_key_source_is_shown() {
        let mut c = ctx();
        c.jev_key = KeySource::Shell;
        let mut w = InstallWizard::new(c);
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::Jev);
        let s = screen(&w);
        assert!(s.contains(" key: shell "), "{s}");
        assert!(s.contains("Key found in your shell environment"), "{s}");
    }

    // ── Ollama models ───────────────────────────────────────────────────────

    #[test]
    fn running_ollama_pre_ticks_ai_summary_with_a_text_model() {
        let mut w = InstallWizard::new(with_ollama(&["nomic-embed-text:latest", "qwen3:4b"]));
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::AiSummary);
        let s = screen(&w);
        assert!(s.contains("[✔] AI summary"), "{s}");
        assert!(s.contains(" Ollama ready "), "{s}");
        assert!(s.contains("model: qwen3:4b"), "{s}");
        assert!(s.contains("Ollama text models: qwen3:4b"), "{s}");
        assert!(s.contains("next model"), "{s}");
        press(&mut w, &[Enter, Enter]);
        assert_eq!(
            applied(&w).ai_summary,
            Some(AiSummary::On("qwen3:4b".into()))
        );
    }

    #[test]
    fn m_cycles_text_models_for_ai_summary() {
        let mut w = InstallWizard::new(with_ollama(&["a:1", "b:2", "nomic-embed-text", "c:3"]));
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::AiSummary);
        assert_eq!(w.choices.ai_model, "a:1");
        press(&mut w, &[Char('m')]);
        assert_eq!(w.choices.ai_model, "b:2");
        press(&mut w, &[Char('m'), Char('m')]);
        assert_eq!(
            w.choices.ai_model, "a:1",
            "wraps, skipping the embedding model"
        );
    }

    #[test]
    fn stopped_ollama_leaves_ai_summary_off_and_explains() {
        let mut c = ctx();
        c.ollama = OllamaStatus::Stopped;
        let mut w = InstallWizard::new(c);
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::AiSummary);
        let s = screen(&w);
        assert!(s.contains("[ ] AI summary"), "{s}");
        assert!(s.contains(" Ollama stopped "), "{s}");
        assert!(s.contains("ollama serve"), "{s}");
        assert!(!s.contains("next model"), "{s}");
        press(&mut w, &[Char('m')]);
        assert!(!w.is_on(Feature::AiSummary));
    }

    #[test]
    fn typed_model_not_pulled_is_flagged_in_the_summary() {
        let mut w = InstallWizard::new(with_ollama(&["qwen3:4b"]));
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::AiSummary);
        press(&mut w, &[Char('e')]);
        for _ in 0.."qwen3:4b".len() {
            press(&mut w, &[Backspace]);
        }
        type_text(&mut w, "phi4");
        press(&mut w, &[Enter, Enter]);
        let s = screen(&w);
        assert!(
            s.contains("phi4 is not pulled yet: run `ollama pull phi4`"),
            "{s}"
        );
    }

    // ── Rewrite ─────────────────────────────────────────────────────────────

    #[test]
    fn rewrite_follows_ai_summary_then_cycles_and_clears() {
        let mut w = InstallWizard::new(with_ollama(&["qwen3:4b", "mistral:latest"]));
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::Rewrite);
        assert!(screen(&w).contains("model: same as AI summary (qwen3:4b)"));
        press(&mut w, &[Char('m')]);
        assert_eq!(w.choices.rewrite_model.as_deref(), Some("qwen3:4b"));
        press(&mut w, &[Char('m')]);
        assert_eq!(w.choices.rewrite_model.as_deref(), Some("mistral:latest"));
        press(&mut w, &[Char('m')]);
        assert_eq!(w.choices.rewrite_model, None, "back to same as AI summary");

        press(&mut w, &[Char('e')]);
        type_text(&mut w, "phi4");
        press(&mut w, &[Enter]);
        assert_eq!(w.choices.rewrite_model.as_deref(), Some("phi4"));
        assert!(screen(&w).contains("ollama pull phi4"));
        press(&mut w, &[Char('e')]);
        for _ in 0..4 {
            press(&mut w, &[Backspace]);
        }
        press(&mut w, &[Enter]);
        assert_eq!(
            w.choices.rewrite_model, None,
            "an emptied field follows AI summary"
        );
    }

    #[test]
    fn rewrite_auto_and_model_reach_the_plan() {
        let mut w = InstallWizard::new(with_ollama(&["qwen3:4b", "mistral:latest"]));
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::Rewrite);
        press(&mut w, &[Char(' '), Char('m'), Char('m'), Enter]);
        let s = screen(&w);
        assert!(s.contains("auto · model: mistral:latest"), "{s}");
        press(&mut w, &[Enter]);
        let plan = applied(&w);
        assert_eq!(plan.rewrite_auto, Some(true));
        assert_eq!(plan.rewrite_model, Some(Some("mistral:latest".into())));
    }

    // ── Semantic search ─────────────────────────────────────────────────────

    #[test]
    fn embeddings_switch_to_ollama_with_a_pulled_embedding_model() {
        let mut w = InstallWizard::new(with_ollama(&[
            "qwen3:4b",
            "nomic-embed-text:latest",
            "bge-m3:latest",
        ]));
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::Embeddings);
        let s = screen(&w);
        assert!(s.contains("candle · all-MiniLM-L6-v2"), "{s}");
        assert!(s.contains(" local "), "{s}");
        assert!(
            s.contains("Ollama embedding models: nomic-embed-text:latest, bge-m3:latest"),
            "{s}"
        );
        press(&mut w, &[Char('p')]);
        assert_eq!(
            w.choices.embed,
            EmbedChoice::Ollama("nomic-embed-text:latest".into())
        );
        press(&mut w, &[Char('m')]);
        assert_eq!(w.choices.embed, EmbedChoice::Ollama("bge-m3:latest".into()));
        let s = screen(&w);
        assert!(s.contains("ollama · bge-m3:latest"), "{s}");
        assert!(s.contains("`ecotokens index` again"), "{s}");
        press(&mut w, &[Enter, Enter]);
        assert_eq!(
            applied(&w).embed,
            Some(EmbedChoice::Ollama("bge-m3:latest".into()))
        );
    }

    #[test]
    fn embeddings_off_means_bm25_only_and_p_goes_back_to_candle() {
        let mut w = InstallWizard::new(ctx());
        press(&mut w, &[Enter]);
        focus(&mut w, Feature::Embeddings);
        press(&mut w, &[Char(' ')]);
        assert!(screen(&w).contains("BM25 only"));
        press(&mut w, &[Char('p')]); // turns it back on, on Ollama
        assert!(w.is_on(Feature::Embeddings));
        let s = screen(&w);
        assert!(s.contains("ollama · nomic-embed-text"), "{s}");
        assert!(s.contains("Ollama not found"), "{s}");
        press(&mut w, &[Char('p')]);
        assert_eq!(
            w.choices.embed,
            EmbedChoice::Candle(DEFAULT_CANDLE_EMBED_MODEL.into())
        );
        press(&mut w, &[Enter, Enter]);
        assert_eq!(applied(&w).embed, None, "back to the current setting");
    }

    #[test]
    fn small_and_wide_terminals_render_without_panicking() {
        let mut w = InstallWizard::new(with_ollama(&["qwen3:4b"]));
        for (cols, rows) in [(40, 12), (80, 24), (200, 60)] {
            for _ in 0..3 {
                let mut t = Terminal::new(TestBackend::new(cols, rows)).unwrap();
                t.draw(|f| w.render(f)).unwrap();
                press(&mut w, &[Enter]);
            }
            w = InstallWizard::new(with_ollama(&["qwen3:4b"]));
        }
    }
}
