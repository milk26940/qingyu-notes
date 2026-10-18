use std::path::{Component, Path, PathBuf};

use crate::error::AppError;

// 所有已存在的路径组件都必须为普通目录或文件，拒绝符号链接和 Windows 目录联接。
pub fn check_inside(root: &Path, path: &Path) -> Result<(), AppError> {
    let relative = path.strip_prefix(root).map_err(|_| unsafe_path())?;
    if relative.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err(unsafe_path());
    }
    let mut current = root.to_path_buf();
    check_component(&current)?;
    for part in relative.components() {
        current.push(part.as_os_str());
        check_component(&current)?;
    }
    Ok(())
}

pub fn existing_inside(root: &Path, path: &Path) -> Result<PathBuf, AppError> {
    check_inside(root, path)?;
    let canonical_root = root.canonicalize().map_err(|_| unsafe_path())?;
    let canonical = path.canonicalize().map_err(|_| AppError::new("attachment_not_found", "附件不存在"))?;
    if !canonical.starts_with(canonical_root) || !canonical.is_file() { return Err(unsafe_path()); }
    Ok(canonical)
}

fn check_component(path: &Path) -> Result<(), AppError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() { return Err(unsafe_path()); }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 { return Err(unsafe_path()); }
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(unsafe_path()),
    }
}

fn unsafe_path() -> AppError { AppError::new("attachment_invalid_path", "附件路径越界或包含链接，已拒绝访问") }

