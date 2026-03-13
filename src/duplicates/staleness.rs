use std::path::Path;
use std::time::SystemTime;

#[allow(dead_code)]
pub struct StaleWarning {
    pub index_time: SystemTime,
    pub newest_source_time: SystemTime,
}

pub fn check_staleness(index_dir: &Path, source_path: &Path) -> Option<StaleWarning> {
    let meta_json = index_dir.join("meta.json");
    let index_mtime = std::fs::metadata(&meta_json).ok()?.modified().ok()?;
    let newest = walk_newest_mtime(source_path)?;
    if newest > index_mtime {
        Some(StaleWarning {
            index_time: index_mtime,
            newest_source_time: newest,
        })
    } else {
        None
    }
}

fn walk_newest_mtime(dir: &Path) -> Option<SystemTime> {
    let walker = ignore::WalkBuilder::new(dir)
        .hidden(false)
        .git_ignore(true)
        .build();

    let mut newest: Option<SystemTime> = None;
    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Ok(meta) = std::fs::metadata(path) {
            if let Ok(t) = meta.modified() {
                newest = Some(match newest {
                    Some(prev) => prev.max(t),
                    None => t,
                });
            }
        }
    }
    newest
}
