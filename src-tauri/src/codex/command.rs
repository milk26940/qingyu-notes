use std::path::PathBuf;
use std::process::Command;

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexLaunch {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
}

impl CodexLaunch {
    pub fn from_path(path: PathBuf) -> Self {
        if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("cmd")) {
            return Self {
                program: PathBuf::from("cmd.exe"),
                prefix_args: vec!["/D".into(), "/S".into(), "/C".into(), path.to_string_lossy().into()],
            };
        }
        Self { program: path, prefix_args: Vec::new() }
    }
}

pub fn resolve_codex_launch() -> Result<CodexLaunch, AppError> {
    for candidate in ["codex.exe", "codex.cmd"] {
        let output = Command::new("where.exe")
            .arg(candidate)
            .output()
            .map_err(|error| AppError::new("codex_discovery_failed", error.to_string()))?;
        if output.status.success() {
            if let Some(path) = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
            {
                let path = PathBuf::from(path);
                if path.is_file() {
                    return Ok(CodexLaunch::from_path(path));
                }
            }
        }
    }
    Err(AppError::new("codex_not_found", "未找到本机 Codex 可执行文件"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_candidate_is_launched_directly() {
        let launch = CodexLaunch::from_path(PathBuf::from(r"C:\tools\codex.exe"));
        assert_eq!(launch.program, PathBuf::from(r"C:\tools\codex.exe"));
        assert!(launch.prefix_args.is_empty());
    }

    #[test]
    fn cmd_candidate_uses_cmd_exe() {
        let launch = CodexLaunch::from_path(PathBuf::from(r"C:\tools\codex.cmd"));
        assert_eq!(launch.program, PathBuf::from("cmd.exe"));
        assert_eq!(&launch.prefix_args[0..3], ["/D", "/S", "/C"]);
    }
}
