use std::path::{Path, PathBuf};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::error::AppError;

pub struct CodexConfigWatcher {
    _watcher: RecommendedWatcher,
}

impl CodexConfigWatcher {
    pub fn start<F>(on_change: F) -> Result<Self, AppError>
    where
        F: Fn() + Send + 'static,
    {
        Ok(Self {
            _watcher: watch_config(&codex_config_path()?, on_change)?,
        })
    }
}

pub fn codex_config_path() -> Result<PathBuf, AppError> {
    if let Some(home) = std::env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(home).join("config.toml"));
    }
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|home| home.join(".codex").join("config.toml"))
        .ok_or_else(|| AppError::new("codex_config_watch_failed", "无法定位 Codex 配置目录"))
}

fn watch_config<F>(config: &Path, on_change: F) -> Result<RecommendedWatcher, AppError>
where
    F: Fn() + Send + 'static,
{
    let parent = config
        .parent()
        .ok_or_else(|| AppError::new("codex_config_watch_failed", "Codex 配置路径无效"))?
        .canonicalize()
        .map_err(|error| AppError::new("codex_config_watch_failed", error.to_string()))?;
    let target = parent.join(
        config
            .file_name()
            .ok_or_else(|| AppError::new("codex_config_watch_failed", "Codex 配置路径无效"))?,
    );
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let Ok(event) = result else {
            return;
        };
        if !matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) {
            return;
        }
        if event.paths.iter().any(|path| path == &target) {
            on_change();
        }
    })
    .map_err(|error| AppError::new("codex_config_watch_failed", error.to_string()))?;
    watcher
        .watch(&parent, RecursiveMode::NonRecursive)
        .map_err(|error| AppError::new("codex_config_watch_failed", error.to_string()))?;
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc;
    use std::time::Duration;
    use uuid::Uuid;

    #[test]
    fn config_metadata_change_is_reported_without_reading_content() {
        let root = std::env::temp_dir().join(format!("qingyu-config-watch-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        fs::write(&config, "model = 'first'").unwrap();
        let (sender, receiver) = mpsc::channel();
        let _watcher = watch_config(&config, move || {
            let _ = sender.send(());
        })
        .unwrap();

        fs::write(&config, "model = 'second'").unwrap();

        receiver.recv_timeout(Duration::from_secs(3)).unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
