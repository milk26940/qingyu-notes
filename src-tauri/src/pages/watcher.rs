use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::error::AppError;

pub fn watch_vault<F>(root: &Path, on_change: F) -> Result<RecommendedWatcher, AppError>
where
    F: Fn(Vec<PathBuf>) + Send + 'static,
{
    let root = root
        .canonicalize()
        .map_err(|error| AppError::new("vault_watch_failed", error.to_string()))?;
    let (sender, receiver) = mpsc::channel::<Event>();
    let mut watcher = notify::recommended_watcher(move |result| {
        if let Ok(event) = result {
            let _ = sender.send(event);
        }
    })
    .map_err(|error| AppError::new("vault_watch_failed", error.to_string()))?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|error| AppError::new("vault_watch_failed", error.to_string()))?;

    std::thread::spawn(move || {
        while let Ok(first) = receiver.recv() {
            let mut events = vec![first];
            while let Ok(event) = receiver.recv_timeout(Duration::from_millis(120)) {
                events.push(event);
            }
            let mut changed = HashSet::new();
            for path in events.into_iter().flat_map(|event| event.paths) {
                let Ok(relative) = path.strip_prefix(&root) else {
                    continue;
                };
                if should_report(relative) {
                    changed.insert(relative.to_path_buf());
                }
            }
            if !changed.is_empty() {
                let mut changed = changed.into_iter().collect::<Vec<_>>();
                changed.sort();
                on_change(changed);
            }
        }
    });
    Ok(watcher)
}

fn should_report(relative: &Path) -> bool {
    let normalized = relative.to_string_lossy().replace('\\', "/");
    if normalized.is_empty()
        || normalized.ends_with(".tmp")
        || normalized.starts_with(".noteapp/history/")
        || normalized.starts_with(".noteapp/restore-staging/")
        || normalized.starts_with(".noteapp/restore-rollback/")
    {
        return false;
    }
    !matches!(
        relative
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("sqlite" | "sqlite3" | "db")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc;
    use std::time::Duration;
    use uuid::Uuid;

    #[test]
    fn external_markdown_change_is_reported() {
        let root = std::env::temp_dir().join(format!("qingyu-watch-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let (sender, receiver) = mpsc::channel();
        let _watcher = watch_vault(&root, move |paths| {
            let _ = sender.send(paths);
        })
        .unwrap();

        let note = root.join("外部修改.md");
        fs::write(&note, "外部内容").unwrap();

        let changed = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(changed.iter().any(|path| path == Path::new("外部修改.md")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn page_directory_changes_are_reported() {
        assert!(should_report(Path::new("父页面")));
        assert!(should_report(Path::new("父页面/子页面")));
        assert!(!should_report(Path::new(".noteapp/restore-rollback/job")));
    }
}
