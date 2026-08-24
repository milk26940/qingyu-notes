use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::model::{BackupEntry, BackupManifest, BackupScope};
use crate::error::AppError;
use crate::pages::model::PageKind;
use crate::pages::repository::PageRepository;

#[derive(Debug, Clone)]
pub struct BackupRepository {
    vault_root: PathBuf,
    backup_root: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl BackupRepository {
    pub fn open(vault_root: &Path) -> Result<Self, AppError> {
        let local_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| AppError::new("backup_init_failed", "无法定位本地应用数据目录"))?;
        let vault_id = hash_bytes(
            vault_root
                .canonicalize()
                .unwrap_or_else(|_| vault_root.to_path_buf())
                .to_string_lossy()
                .to_lowercase()
                .as_bytes(),
        );
        Self::open_at(
            vault_root,
            &local_data
                .join("QingYu")
                .join("backups")
                .join(&vault_id[..16]),
        )
    }

    fn open_at(vault_root: &Path, backup_root: &Path) -> Result<Self, AppError> {
        for directory in ["objects", "snapshots", "staging"] {
            fs::create_dir_all(backup_root.join(directory))
                .map_err(|error| AppError::new("backup_init_failed", error.to_string()))?;
        }
        Ok(Self {
            vault_root: vault_root.to_path_buf(),
            backup_root: backup_root.to_path_buf(),
            lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn checkpoint(
        &self,
        scope: BackupScope,
        source: impl Into<String>,
    ) -> Result<BackupManifest, AppError> {
        let _guard = self.lock()?;
        self.checkpoint_unlocked(scope, source.into())
    }

    pub fn list(&self) -> Result<Vec<BackupManifest>, AppError> {
        let _guard = self.lock()?;
        let mut manifests = Vec::new();
        for entry in fs::read_dir(self.backup_root.join("snapshots"))
            .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?
        {
            let path = entry
                .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?
                .path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let text = fs::read_to_string(path)
                .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?;
            manifests
                .push(serde_json::from_str(&text).map_err(|error| {
                    AppError::new("backup_invalid_manifest", error.to_string())
                })?);
        }
        manifests.sort_by(|left: &BackupManifest, right| right.created_at.cmp(&left.created_at));
        Ok(manifests)
    }

    pub fn restore(&self, snapshot_id: &str) -> Result<BackupManifest, AppError> {
        let _guard = self.lock()?;
        let manifest = self.read_manifest(snapshot_id)?;
        self.checkpoint_unlocked(manifest.scope.clone(), "beforeRestore".into())?;
        let staging = self.rebuild_staging(&manifest)?;
        let result = self.apply_staging(&manifest, &staging);
        let _ = fs::remove_dir_all(&staging);
        result?;
        Ok(manifest)
    }

    pub fn export_zip(&self, destination: &Path) -> Result<PathBuf, AppError> {
        let _guard = self.lock()?;
        if is_within(destination, &self.vault_root) || is_within(destination, &self.backup_root) {
            return Err(AppError::new(
                "backup_export_unsafe_path",
                "导出目标不能位于笔记库或备份目录内",
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| AppError::new("backup_export_failed", error.to_string()))?;
        }
        let temporary = destination.with_extension(format!("{}.tmp", Uuid::new_v4()));
        let file = fs::File::create(&temporary)
            .map_err(|error| AppError::new("backup_export_failed", error.to_string()))?;
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);
        for (relative, path) in self.collect_files(&BackupScope::Vault)? {
            archive
                .start_file(&relative, options)
                .map_err(|error| AppError::new("backup_export_failed", error.to_string()))?;
            archive
                .write_all(
                    &fs::read(path).map_err(|error| {
                        AppError::new("backup_export_failed", error.to_string())
                    })?,
                )
                .map_err(|error| AppError::new("backup_export_failed", error.to_string()))?;
        }
        archive
            .finish()
            .map_err(|error| AppError::new("backup_export_failed", error.to_string()))?;
        fs::rename(&temporary, destination).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            AppError::new("backup_export_failed", error.to_string())
        })?;
        Ok(destination.to_path_buf())
    }

    pub fn import_zip(&self, source: &Path) -> Result<BackupManifest, AppError> {
        let _guard = self.lock()?;
        let file = fs::File::open(source)
            .map_err(|error| AppError::new("backup_import_failed", error.to_string()))?;
        let mut archive = ZipArchive::new(file)
            .map_err(|error| AppError::new("backup_import_failed", error.to_string()))?;
        let staging = self
            .backup_root
            .join("staging")
            .join(format!("import-{}", Uuid::new_v4()));
        fs::create_dir_all(&staging)
            .map_err(|error| AppError::new("backup_import_failed", error.to_string()))?;
        let result = (|| {
            let mut total = 0_u64;
            for index in 0..archive.len() {
                let mut entry = archive
                    .by_index(index)
                    .map_err(|error| AppError::new("backup_import_failed", error.to_string()))?;
                let enclosed = entry
                    .enclosed_name()
                    .ok_or_else(|| AppError::new("backup_unsafe_archive", "ZIP 包含不安全路径"))?;
                if entry.is_dir() {
                    continue;
                }
                let remaining = (1024_u64 * 1024 * 1024).checked_sub(total).ok_or_else(|| {
                    AppError::new("backup_archive_too_large", "ZIP 解压后超过 1 GiB")
                })?;
                let destination = staging.join(enclosed);
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        AppError::new("backup_import_failed", error.to_string())
                    })?;
                }
                let mut output = fs::File::create(destination)
                    .map_err(|error| AppError::new("backup_import_failed", error.to_string()))?;
                let copied = copy_limited(&mut entry, &mut output, remaining)?;
                total += copied;
            }
            self.checkpoint_unlocked(BackupScope::Vault, "beforeImport".into())?;
            self.apply_import(&staging)?;
            self.checkpoint_unlocked(BackupScope::Vault, "zipImport".into())
        })();
        let _ = fs::remove_dir_all(&staging);
        result
    }

    fn checkpoint_unlocked(
        &self,
        scope: BackupScope,
        source: String,
    ) -> Result<BackupManifest, AppError> {
        let mut entries = Vec::new();
        for (relative, path) in self.collect_files(&scope)? {
            let bytes = fs::read(&path)
                .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?;
            let hash = hash_bytes(&bytes);
            let object = self.object_path(&hash);
            if !object.exists() {
                if let Some(parent) = object.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| AppError::new("backup_write_failed", error.to_string()))?;
                }
                let temporary = object.with_extension(format!("{}.tmp", Uuid::new_v4()));
                fs::write(&temporary, &bytes)
                    .map_err(|error| AppError::new("backup_write_failed", error.to_string()))?;
                fs::rename(temporary, &object)
                    .map_err(|error| AppError::new("backup_write_failed", error.to_string()))?;
            }
            entries.push(BackupEntry {
                path: relative,
                hash,
                size: bytes.len() as u64,
            });
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        let manifest = BackupManifest {
            id: Uuid::new_v4().to_string(),
            created_at: now(),
            source,
            scope,
            entries,
        };
        let destination = self
            .backup_root
            .join("snapshots")
            .join(format!("{}.json", manifest.id));
        let temporary = destination.with_extension(format!("{}.tmp", Uuid::new_v4()));
        fs::write(
            &temporary,
            format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap()),
        )
        .map_err(|error| AppError::new("backup_write_failed", error.to_string()))?;
        fs::rename(temporary, destination)
            .map_err(|error| AppError::new("backup_write_failed", error.to_string()))?;
        Ok(manifest)
    }

    fn collect_files(&self, scope: &BackupScope) -> Result<Vec<(String, PathBuf)>, AppError> {
        match scope {
            BackupScope::Vault => {
                let mut files = Vec::new();
                walk_files(&self.vault_root, &self.vault_root, &mut files)?;
                Ok(files)
            }
            BackupScope::Page { page_id } => {
                let pages = PageRepository::open(self.vault_root.clone())?;
                let summary = pages
                    .list(true)?
                    .into_iter()
                    .find(|page| &page.id == page_id)
                    .ok_or_else(|| AppError::new("page_not_found", "页面不存在"))?;
                let mut files = vec![(summary.path.clone(), self.vault_root.join(&summary.path))];
                if summary.kind == PageKind::Database {
                    let relative = format!(".noteapp/databases/{page_id}.json");
                    let definition = self.vault_root.join(&relative);
                    if definition.exists() {
                        files.push((relative, definition));
                    }
                }
                Ok(files)
            }
        }
    }

    fn read_manifest(&self, id: &str) -> Result<BackupManifest, AppError> {
        Uuid::parse_str(id).map_err(|_| AppError::new("backup_not_found", "备份快照不存在"))?;
        let text = fs::read_to_string(
            self.backup_root
                .join("snapshots")
                .join(format!("{id}.json")),
        )
        .map_err(|error| AppError::new("backup_not_found", error.to_string()))?;
        serde_json::from_str(&text)
            .map_err(|error| AppError::new("backup_invalid_manifest", error.to_string()))
    }

    fn rebuild_staging(&self, manifest: &BackupManifest) -> Result<PathBuf, AppError> {
        let staging = self
            .backup_root
            .join("staging")
            .join(Uuid::new_v4().to_string());
        fs::create_dir_all(&staging)
            .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
        for entry in &manifest.entries {
            let relative = safe_relative(&entry.path)?;
            let hash = validate_hash(&entry.hash)?;
            let bytes = fs::read(self.object_path(&hash))
                .map_err(|error| AppError::new("backup_object_missing", error.to_string()))?;
            if hash_bytes(&bytes) != entry.hash || bytes.len() as u64 != entry.size {
                let _ = fs::remove_dir_all(&staging);
                return Err(AppError::new("backup_object_corrupt", "备份对象校验失败"));
            }
            let destination = staging.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
            }
            fs::write(destination, bytes)
                .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
        }
        Ok(staging)
    }

    fn apply_staging(&self, manifest: &BackupManifest, staging: &Path) -> Result<(), AppError> {
        let current_page_path = if let BackupScope::Page { page_id } = &manifest.scope {
            PageRepository::open(self.vault_root.clone())?
                .list(true)?
                .into_iter()
                .find(|page| &page.id == page_id)
                .map(|page| page.path)
        } else {
            None
        };
        let rollback = staging.join(".rollback");
        fs::create_dir_all(&rollback)
            .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
        let mut changed = Vec::new();
        for entry in &manifest.entries {
            let relative = safe_relative(&entry.path)?;
            let destination = self.vault_root.join(&relative);
            let previous = rollback.join(&relative);
            let existed = destination.exists();
            if existed {
                if let Some(parent) = previous.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        AppError::new("backup_restore_failed", error.to_string())
                    })?;
                }
                fs::copy(&destination, &previous)
                    .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
            }
            if let Err(error) = replace_file(&staging.join(&relative), &destination) {
                rollback_changes(&self.vault_root, &rollback, &changed);
                return Err(error);
            }
            changed.push((relative, existed));
        }
        if matches!(manifest.scope, BackupScope::Vault) {
            let expected = manifest
                .entries
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<HashSet<_>>();
            for (relative, path) in self.collect_files(&BackupScope::Vault)? {
                if !expected.contains(relative.as_str()) {
                    fs::remove_file(path).map_err(|error| {
                        AppError::new("backup_restore_failed", error.to_string())
                    })?;
                }
            }
        } else if let Some(current) = current_page_path {
            let expected = manifest
                .entries
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<HashSet<_>>();
            if !expected.contains(current.as_str()) {
                let path = self.vault_root.join(current);
                if path.exists() {
                    fs::remove_file(path).map_err(|error| {
                        AppError::new("backup_restore_failed", error.to_string())
                    })?;
                }
            }
        }
        Ok(())
    }

    fn apply_import(&self, staging: &Path) -> Result<(), AppError> {
        let mut files = Vec::new();
        walk_files(staging, staging, &mut files)?;
        for (relative, source) in files {
            replace_file(&source, &self.vault_root.join(safe_relative(&relative)?))?;
        }
        Ok(())
    }

    fn object_path(&self, hash: &str) -> PathBuf {
        self.backup_root.join("objects").join(&hash[..2]).join(hash)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, AppError> {
        self.lock
            .lock()
            .map_err(|_| AppError::new("backup_lock_failed", "备份存储锁已损坏"))
    }
}

fn walk_files(
    root: &Path,
    directory: &Path,
    output: &mut Vec<(String, PathBuf)>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(directory)
        .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?
    {
        let path = entry
            .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?
            .path();
        if path.is_dir() {
            walk_files(root, &path, output)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| AppError::new("backup_unsafe_path", error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if should_skip(&relative) {
            continue;
        }
        output.push((relative, path));
    }
    Ok(())
}

fn should_skip(relative: &str) -> bool {
    relative.ends_with(".tmp")
        || relative.starts_with(".noteapp/restore-staging/")
        || (relative.starts_with(".noteapp/")
            && [".db", ".sqlite", ".sqlite3"]
                .iter()
                .any(|extension| relative.ends_with(extension)))
}

fn safe_relative(value: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError::new("backup_unsafe_path", "备份包含不安全路径"));
    }
    Ok(path)
}

fn replace_file(source: &Path, destination: &Path) -> Result<(), AppError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
    }
    let temporary = destination.with_extension(format!("{}.restore.tmp", Uuid::new_v4()));
    fs::copy(source, &temporary)
        .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
    }
    fs::rename(temporary, destination)
        .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))
}

fn rollback_changes(root: &Path, rollback: &Path, changed: &[(PathBuf, bool)]) {
    for (relative, existed) in changed.iter().rev() {
        let destination = root.join(relative);
        if *existed {
            let _ = replace_file(&rollback.join(relative), &destination);
        } else {
            let _ = fs::remove_file(destination);
        }
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_hash(value: &str) -> Result<String, AppError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::new(
            "backup_invalid_manifest",
            "备份清单包含无效哈希",
        ));
    }
    Ok(value.to_ascii_lowercase())
}

fn is_within(path: &Path, root: &Path) -> bool {
    let path = path
        .canonicalize()
        .or_else(|_| {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .canonicalize()
                .map(|parent| {
                    parent.join(path.file_name().unwrap_or_else(|| std::ffi::OsStr::new("")))
                })
        })
        .unwrap_or_else(|_| path.to_path_buf());
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    path == root || path.starts_with(root)
}

fn copy_limited<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    limit: u64,
) -> Result<u64, AppError> {
    let mut limited = reader.take(limit.saturating_add(1));
    let copied = std::io::copy(&mut limited, writer)
        .map_err(|error| AppError::new("backup_import_failed", error.to_string()))?;
    if copied > limit {
        return Err(AppError::new(
            "backup_archive_too_large",
            "ZIP 解压后超过 1 GiB",
        ));
    }
    Ok(copied)
}

fn now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("qingyu-backup-{}", Uuid::new_v4()));
        (base.join("vault"), base.join("backups"))
    }

    #[test]
    fn identical_content_is_stored_once_across_snapshots() {
        let (vault, backups) = roots();
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("一.md"), "相同内容").unwrap();
        fs::write(vault.join("二.md"), "相同内容").unwrap();
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();
        let first = repo.checkpoint(BackupScope::Vault, "manual").unwrap();
        let second = repo.checkpoint(BackupScope::Vault, "manual").unwrap();
        assert_eq!(first.entries.len(), 2);
        assert_eq!(second.entries.len(), 2);
        let object_count = fs::read_dir(repo.backup_root.join("objects"))
            .unwrap()
            .flat_map(|entry| fs::read_dir(entry.unwrap().path()).unwrap())
            .count();
        assert_eq!(object_count, 1);
        let _ = fs::remove_dir_all(vault.parent().unwrap());
    }

    #[test]
    fn vault_and_page_restore_recover_snapshot_content() {
        let (vault, backups) = roots();
        let pages = PageRepository::open(vault.clone()).unwrap();
        let first = pages
            .create("第一篇".into(), None, "版本一".into())
            .unwrap();
        let second = pages.create("第二篇".into(), None, "保留".into()).unwrap();
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();
        let page_snapshot = repo
            .checkpoint(
                BackupScope::Page {
                    page_id: first.id.clone(),
                },
                "manual",
            )
            .unwrap();
        pages
            .update(&first.id, first.title.clone(), "版本二".into())
            .unwrap();
        pages
            .update(&second.id, second.title.clone(), "仍然保留".into())
            .unwrap();
        repo.restore(&page_snapshot.id).unwrap();
        assert_eq!(pages.get(&first.id, false).unwrap().content, "版本一");
        assert_eq!(pages.get(&second.id, false).unwrap().content, "仍然保留");

        let vault_snapshot = repo.checkpoint(BackupScope::Vault, "manual").unwrap();
        pages
            .update(&first.id, first.title, "版本三".into())
            .unwrap();
        repo.restore(&vault_snapshot.id).unwrap();
        assert_eq!(pages.get(&first.id, false).unwrap().content, "版本一");
        let _ = fs::remove_dir_all(vault.parent().unwrap());
    }

    #[test]
    fn corrupt_object_is_rejected_before_restore() {
        let (vault, backups) = roots();
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("笔记.md"), "原始").unwrap();
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();
        let snapshot = repo.checkpoint(BackupScope::Vault, "manual").unwrap();
        fs::write(repo.object_path(&snapshot.entries[0].hash), "损坏").unwrap();
        fs::write(vault.join("笔记.md"), "当前").unwrap();
        let error = repo.restore(&snapshot.id).unwrap_err();
        assert_eq!(error.code, "backup_object_corrupt");
        assert_eq!(fs::read_to_string(vault.join("笔记.md")).unwrap(), "当前");
        let _ = fs::remove_dir_all(vault.parent().unwrap());
    }

    #[test]
    fn zip_import_rejects_parent_directory_paths() {
        let (vault, backups) = roots();
        fs::create_dir_all(&vault).unwrap();
        let archive_path = vault.parent().unwrap().join("unsafe.zip");
        let file = fs::File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(file);
        archive
            .start_file("../outside.md", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"unsafe").unwrap();
        archive.finish().unwrap();
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();
        let error = repo.import_zip(&archive_path).unwrap_err();
        assert_eq!(error.code, "backup_unsafe_archive");
        assert!(!vault.parent().unwrap().join("outside.md").exists());
        let _ = fs::remove_dir_all(vault.parent().unwrap());
    }

    #[test]
    fn malformed_manifest_hash_is_rejected_without_panic() {
        assert!(validate_hash("x").is_err());
        assert!(validate_hash("../outside").is_err());
    }

    #[test]
    fn actual_zip_output_limit_is_enforced() {
        let mut input = std::io::Cursor::new(vec![1_u8, 2, 3, 4]);
        let mut output = Vec::new();
        let error = copy_limited(&mut input, &mut output, 3).unwrap_err();
        assert_eq!(error.code, "backup_archive_too_large");
    }

    #[test]
    fn export_rejects_destination_inside_vault() {
        let (vault, backups) = roots();
        fs::create_dir_all(&vault).unwrap();
        let target = vault.join("export.zip");
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();
        let error = repo.export_zip(&target).unwrap_err();
        assert_eq!(error.code, "backup_export_unsafe_path");
        assert!(!target.exists());
        let _ = fs::remove_dir_all(vault.parent().unwrap());
    }
}
