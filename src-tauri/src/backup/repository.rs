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
use crate::pages::repository::shared_vault_lock;
use crate::pages::repository::PageRepository;
use crate::portable::PortablePaths;

#[derive(Debug, Clone)]
pub struct BackupRepository {
    vault_root: PathBuf,
    backup_root: PathBuf,
    lock: Arc<Mutex<()>>,
    vault_lock: Arc<Mutex<()>>,
}

impl BackupRepository {
    pub fn open(vault_root: &Path) -> Result<Self, AppError> {
        let paths = PortablePaths::current()
            .map_err(|error| AppError::new("backup_init_failed", error.to_string()))?;
        Self::open_at(vault_root, &paths.backups_root)
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
            vault_lock: shared_vault_lock(vault_root),
        })
    }

    pub fn checkpoint(
        &self,
        scope: BackupScope,
        source: impl Into<String>,
    ) -> Result<BackupManifest, AppError> {
        let _guard = self.lock()?;
        let _vault_guard = self.lock_vault()?;
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
        let _vault_guard = self.lock_vault()?;
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
        let _vault_guard = self.lock_vault()?;
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
        let (files, temporary_files) = self.collect_files(&BackupScope::Vault)?;
        for (relative, path) in files {
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
        for path in temporary_files {
            let _ = fs::remove_file(path);
        }
        fs::rename(&temporary, destination).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            AppError::new("backup_export_failed", error.to_string())
        })?;
        Ok(destination.to_path_buf())
    }

    pub fn import_zip(&self, source: &Path) -> Result<BackupManifest, AppError> {
        let _guard = self.lock()?;
        let _vault_guard = self.lock_vault()?;
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
        let (files, temporary_files) = self.collect_files(&scope)?;
        for (relative, path) in files {
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
                commit_temporary_file(&temporary, &object, "backup_write_failed")?;
            }
            entries.push(BackupEntry {
                path: relative,
                hash,
                size: bytes.len() as u64,
            });
        }
        for path in temporary_files {
            let _ = fs::remove_file(path);
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
        commit_temporary_file(&temporary, &destination, "backup_write_failed")?;
        Ok(manifest)
    }

    fn collect_files(
        &self,
        scope: &BackupScope,
    ) -> Result<(Vec<(String, PathBuf)>, Vec<PathBuf>), AppError> {
        let data_root = self.vault_root.parent().unwrap_or(&self.vault_root);
        match scope {
            BackupScope::Vault => {
                let mut files = Vec::new();
                let mut temporary_files = Vec::new();
                if let Ok(pages) = PageRepository::open(self.vault_root.clone()) {
                    let database = self
                        .backup_root
                        .join("staging")
                        .join(format!("database-{}.sqlite3", Uuid::new_v4()));
                    pages.snapshot_database(&database)?;
                    files.push(("qingyu.sqlite3".into(), database.clone()));
                    temporary_files.push(database);
                }
                let attachments = data_root.join("attachments");
                if attachments.exists() {
                    walk_files(data_root, &attachments, &mut files, true)?;
                }
                // 旧 Markdown 仅作为回退材料随整库备份保留。
                walk_files(&self.vault_root, &self.vault_root, &mut files, true)?;
                Ok((files, temporary_files))
            }
            BackupScope::Page { page_id } => {
                let pages = PageRepository::open(self.vault_root.clone())?;
                let bundle = pages.open_bundle(page_id)?;
                let document_path = self
                    .backup_root
                    .join("staging")
                    .join(format!("page-{page_id}-{}.json", Uuid::new_v4()));
                fs::write(
                    &document_path,
                    serde_json::to_vec_pretty(&bundle.document).unwrap(),
                )
                .map_err(|error| AppError::new("backup_write_failed", error.to_string()))?;
                let mut files = vec![(format!("pages/{page_id}.json"), document_path.clone())];
                let assets = data_root.join("attachments").join(page_id);
                if assets.exists() {
                    walk_files(data_root, &assets, &mut files, true)?;
                }
                Ok((files, vec![document_path]))
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
        if staging.join("qingyu.sqlite3").exists() {
            return self.apply_database_staging(staging);
        }
        if let BackupScope::Page { page_id } = &manifest.scope {
            let document = staging.join("pages").join(format!("{page_id}.json"));
            if document.exists() {
                return self.apply_page_staging(page_id, &document, staging);
            }
        }
        let current_page_path = if let BackupScope::Page { page_id } = &manifest.scope {
            PageRepository::open(self.vault_root.clone())?
                .list(true)?
                .into_iter()
                .find(|page| &page.id == page_id)
                .map(|page| page.path)
        } else {
            None
        };
        let expected = manifest
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<HashSet<_>>();
        let mut removals = Vec::new();
        if matches!(manifest.scope, BackupScope::Vault) {
            let mut current_files = Vec::new();
            walk_files(
                &self.vault_root,
                &self.vault_root,
                &mut current_files,
                false,
            )?;
            for (relative, path) in current_files {
                if !expected.contains(relative.as_str()) {
                    removals.push((safe_relative(&relative)?, path));
                }
            }
        } else if let Some(current) = current_page_path {
            if !expected.contains(current.as_str()) {
                let relative = safe_relative(&current)?;
                let path = self.vault_root.join(&relative);
                if path.exists() {
                    removals.push((relative, path));
                }
            }
        }

        let rollback = self
            .vault_root
            .join(".noteapp")
            .join("restore-rollback")
            .join(Uuid::new_v4().to_string());
        let originals = rollback.join("originals");
        let removed_root = rollback.join("removed");
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        let result = (|| {
            fs::create_dir_all(&originals)
                .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
            fs::create_dir_all(&removed_root)
                .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;

            for (relative, source) in removals {
                let previous = removed_root.join(&relative);
                if let Some(parent) = previous.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        AppError::new("backup_restore_failed", error.to_string())
                    })?;
                }
                fs::rename(&source, &previous)
                    .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
                removed.push(relative);
            }

            for entry in &manifest.entries {
                let relative = safe_relative(&entry.path)?;
                let destination = self.vault_root.join(&relative);
                let previous = originals.join(&relative);
                let existed = destination.exists();
                if existed {
                    if let Some(parent) = previous.parent() {
                        fs::create_dir_all(parent).map_err(|error| {
                            AppError::new("backup_restore_failed", error.to_string())
                        })?;
                    }
                    fs::copy(&destination, &previous).map_err(|error| {
                        AppError::new("backup_restore_failed", error.to_string())
                    })?;
                }
                replace_file(&staging.join(&relative), &destination)?;
                changed.push((relative, existed));
            }
            Ok(())
        })();

        if let Err(error) = result {
            rollback_changes(&self.vault_root, &originals, &changed);
            rollback_removed(&self.vault_root, &removed_root, &removed);
            let _ = fs::remove_dir_all(&rollback);
            return Err(error);
        }
        fs::remove_dir_all(&rollback)
            .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
        Ok(())
    }

    fn apply_database_staging(&self, staging: &Path) -> Result<(), AppError> {
        let pages = PageRepository::open(self.vault_root.clone())?;
        let data_root = self.vault_root.parent().unwrap_or(&self.vault_root);
        let attachments = data_root.join("attachments");
        let previous = data_root.join(format!(".attachments.restore-{}", Uuid::new_v4()));
        if attachments.exists() {
            fs::rename(&attachments, &previous)
                .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
        }
        let result = (|| {
            let source = staging.join("attachments");
            if source.exists() {
                crate::portable::copy_tree_verified(&source, &attachments)
                    .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
            } else {
                fs::create_dir_all(&attachments)
                    .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
            }
            pages.restore_database(&staging.join("qingyu.sqlite3"))
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&attachments);
            if previous.exists() {
                let _ = fs::rename(&previous, &attachments);
            }
        } else if previous.exists() {
            fs::remove_dir_all(&previous)
                .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
        }
        result
    }

    fn apply_page_staging(
        &self,
        page_id: &str,
        document_path: &Path,
        staging: &Path,
    ) -> Result<(), AppError> {
        let document = serde_json::from_slice(&fs::read(document_path)
            .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?)
            .map_err(|error| AppError::new("backup_invalid_page", error.to_string()))?;
        let pages = PageRepository::open(self.vault_root.clone())?;
        let data_root = self.vault_root.parent().unwrap_or(&self.vault_root);
        let attachments = data_root.join("attachments").join(page_id);
        let previous = data_root.join(format!(".page-assets.restore-{}", Uuid::new_v4()));
        if attachments.exists() {
            fs::rename(&attachments, &previous)
                .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
        }
        let result = (|| {
            let source = staging.join("attachments").join(page_id);
            if source.exists() {
                crate::portable::copy_tree_verified(&source, &attachments)
                    .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
            }
            pages.restore_document(document).map(|_| ())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&attachments);
            if previous.exists() {
                let _ = fs::rename(&previous, &attachments);
            }
        } else if previous.exists() {
            fs::remove_dir_all(&previous)
                .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
        }
        result
    }

    fn apply_import(&self, staging: &Path) -> Result<(), AppError> {
        let mut files = Vec::new();
        walk_files(staging, staging, &mut files, true)?;
        let rollback = self
            .vault_root
            .join(".noteapp")
            .join("restore-rollback")
            .join(format!("import-{}", Uuid::new_v4()));
        let originals = rollback.join("originals");
        let mut changed = Vec::new();
        let result = (|| {
            fs::create_dir_all(&originals)
                .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
            for (relative, source) in files {
                let relative = safe_relative(&relative)?;
                let destination = self.vault_root.join(&relative);
                let existed = destination.exists();
                if existed {
                    let previous = originals.join(&relative);
                    if let Some(parent) = previous.parent() {
                        fs::create_dir_all(parent).map_err(|error| {
                            AppError::new("backup_restore_failed", error.to_string())
                        })?;
                    }
                    fs::copy(&destination, previous).map_err(|error| {
                        AppError::new("backup_restore_failed", error.to_string())
                    })?;
                }
                replace_file(&source, &destination)?;
                changed.push((relative, existed));
            }
            Ok(())
        })();
        if let Err(error) = result {
            rollback_changes(&self.vault_root, &originals, &changed);
            let _ = fs::remove_dir_all(&rollback);
            return Err(error);
        }
        fs::remove_dir_all(&rollback)
            .map_err(|error| AppError::new("backup_restore_failed", error.to_string()))?;
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

    fn lock_vault(&self) -> Result<std::sync::MutexGuard<'_, ()>, AppError> {
        self.vault_lock
            .lock()
            .map_err(|_| AppError::new("vault_write_lock_failed", "笔记库写入锁已损坏"))
    }
}

fn walk_files(
    root: &Path,
    directory: &Path,
    output: &mut Vec<(String, PathBuf)>,
    reject_hard_links: bool,
) -> Result<(), AppError> {
    let canonical_root = root
        .canonicalize()
        .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?;
    for entry in fs::read_dir(directory)
        .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?
    {
        let entry =
            entry.map_err(|error| AppError::new("backup_read_failed", error.to_string()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?;
        if file_type.is_symlink() {
            continue;
        }
        let canonical_path = path
            .canonicalize()
            .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?;
        if !canonical_path.starts_with(&canonical_root) {
            continue;
        }
        if file_type.is_dir() {
            walk_files(root, &path, output, reject_hard_links)?;
            continue;
        }
        if reject_hard_links && has_multiple_links(&path)? {
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

#[cfg(windows)]
fn has_multiple_links(path: &Path) -> Result<bool, AppError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let file = fs::File::open(path)
        .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?;
    // Windows 标准库尚未稳定暴露链接计数，因此通过只读文件句柄查询元数据。
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let success =
        unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut information) };
    if success == 0 {
        return Err(AppError::new(
            "backup_read_failed",
            std::io::Error::last_os_error().to_string(),
        ));
    }
    Ok(information.nNumberOfLinks > 1)
}

#[cfg(not(windows))]
fn has_multiple_links(path: &Path) -> Result<bool, AppError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(path)
        .map_err(|error| AppError::new("backup_read_failed", error.to_string()))?;
    Ok(metadata.nlink() > 1)
}

fn should_skip(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/").to_ascii_lowercase();
    let components = normalized.split('/').collect::<Vec<_>>();
    let file_name = components.last().copied().unwrap_or_default();
    let sensitive_tree = components.iter().any(|component| {
        matches!(
            *component,
            ".codex" | "ccswitch" | "cc-switch" | "cc_switch"
        )
    });
    sensitive_tree
        || file_name == "auth.json"
        || [".db", ".sqlite", ".sqlite3"]
            .iter()
            .any(|extension| file_name.ends_with(extension))
        || normalized.ends_with(".tmp")
        || normalized.starts_with(".noteapp/restore-staging/")
        || normalized.starts_with(".noteapp/restore-rollback/")
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

fn commit_temporary_file(
    temporary: &Path,
    destination: &Path,
    error_code: &str,
) -> Result<(), AppError> {
    fs::copy(temporary, destination)
        .map_err(|error| AppError::new(error_code, error.to_string()))?;
    fs::remove_file(temporary)
        .map_err(|error| AppError::new(error_code, error.to_string()))
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

fn rollback_removed(root: &Path, rollback: &Path, removed: &[PathBuf]) {
    for relative in removed.iter().rev() {
        let source = rollback.join(relative);
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::rename(source, destination);
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
    fn vault_snapshot_excludes_credentials_databases_and_sensitive_trees() {
        let (vault, backups) = roots();
        fs::create_dir_all(vault.join(".codex")).unwrap();
        fs::create_dir_all(vault.join("CCSwitch")).unwrap();
        fs::write(vault.join("普通.md"), "正文").unwrap();
        fs::write(vault.join("auth.json"), "credential").unwrap();
        fs::write(vault.join("index.sqlite"), "database").unwrap();
        fs::write(vault.join(".codex").join("secret.txt"), "secret").unwrap();
        fs::write(vault.join("CCSwitch").join("cc-switch.db"), "database").unwrap();
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();

        let snapshot = repo.checkpoint(BackupScope::Vault, "manual").unwrap();
        let paths = snapshot
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec!["普通.md"]);
        let _ = fs::remove_dir_all(vault.parent().unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn vault_snapshot_excludes_hard_linked_files() {
        let (vault, backups) = roots();
        fs::create_dir_all(&vault).unwrap();
        let external = vault.parent().unwrap().join("external-secret.txt");
        fs::write(&external, "secret").unwrap();
        fs::hard_link(&external, vault.join("普通附件.txt")).unwrap();
        fs::write(vault.join("普通.md"), "正文").unwrap();
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();

        let snapshot = repo.checkpoint(BackupScope::Vault, "manual").unwrap();

        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].path, "普通.md");
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
        assert!(repo
            .list()
            .unwrap()
            .iter()
            .any(|snapshot| snapshot.source == "beforeRestore"));
        let _ = fs::remove_dir_all(vault.parent().unwrap());
    }

    #[test]
    fn vault_snapshot_captures_committed_wal_data_with_sqlite_online_backup() {
        let (vault, backups) = roots();
        let pages = PageRepository::open(vault.clone()).unwrap();
        let page = pages.create("WAL 页面".into(), None, "第一版".into()).unwrap();
        pages
            .update(&page.id, page.title.clone(), "尚未检查点的最新版本".into())
            .unwrap();
        assert!(pages.database_path().with_extension("sqlite3-wal").exists());
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();

        let snapshot = repo.checkpoint(BackupScope::Vault, "manual").unwrap();
        pages
            .update(&page.id, page.title, "快照之后".into())
            .unwrap();
        repo.restore(&snapshot.id).unwrap();

        assert_eq!(
            pages.get(&page.id, false).unwrap().content,
            "尚未检查点的最新版本"
        );
        let _ = fs::remove_dir_all(vault.parent().unwrap());
    }

    #[test]
    fn page_snapshots_include_and_restore_page_assets() {
        let (vault, backups) = roots();
        let pages = PageRepository::open(vault.clone()).unwrap();
        let page = pages.create("图片页".into(), None, String::new()).unwrap();
        let relative = pages
            .store_asset(&page.id, "image.png", &[137, 80, 78, 71, 13, 10, 26, 10, 1])
            .unwrap();
        let file_name = Path::new(&relative).file_name().unwrap();
        let manifest_path = format!("attachments/{}/{}", page.id, file_name.to_string_lossy());
        let asset = vault
            .parent()
            .unwrap()
            .join("attachments")
            .join(&page.id)
            .join(file_name);
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();

        let vault_snapshot = repo.checkpoint(BackupScope::Vault, "manual").unwrap();
        assert!(vault_snapshot
            .entries
            .iter()
            .any(|entry| entry.path == manifest_path));
        let zip_path = vault.parent().unwrap().join("assets.zip");
        repo.export_zip(&zip_path).unwrap();
        let mut archive = ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
        assert!(archive.by_name(&manifest_path).is_ok());
        drop(archive);

        let snapshot = repo
            .checkpoint(
                BackupScope::Page {
                    page_id: page.id.clone(),
                },
                "manual",
            )
            .unwrap();
        assert!(snapshot
            .entries
            .iter()
            .any(|entry| entry.path == manifest_path));

        fs::write(&asset, b"changed").unwrap();
        repo.restore(&snapshot.id).unwrap();
        assert_eq!(
            fs::read(asset).unwrap(),
            [137, 80, 78, 71, 13, 10, 26, 10, 1]
        );
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
    fn failed_zip_import_rolls_back_files_already_replaced() {
        let (vault, backups) = roots();
        fs::create_dir_all(vault.join("blocked.md")).unwrap();
        fs::write(vault.join("a.md"), "导入前").unwrap();
        let archive_path = vault.parent().unwrap().join("partial.zip");
        let file = fs::File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(file);
        archive
            .start_file("a.md", SimpleFileOptions::default())
            .unwrap();
        archive.write_all("导入后".as_bytes()).unwrap();
        archive
            .start_file("blocked.md", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"blocked").unwrap();
        archive.finish().unwrap();
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();

        let error = repo.import_zip(&archive_path).unwrap_err();

        assert_eq!(error.code, "backup_restore_failed");
        assert_eq!(fs::read_to_string(vault.join("a.md")).unwrap(), "导入前");
        assert!(vault.join("blocked.md").is_dir());
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

    #[cfg(windows)]
    #[test]
    fn vault_restore_rolls_back_replacements_when_extra_file_cannot_be_removed() {
        use std::os::windows::fs::OpenOptionsExt;

        let (vault, backups) = roots();
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("笔记.md"), "快照内容").unwrap();
        let repo = BackupRepository::open_at(&vault, &backups).unwrap();
        let snapshot = repo.checkpoint(BackupScope::Vault, "manual").unwrap();
        fs::write(vault.join("笔记.md"), "恢复前内容").unwrap();
        let locked_path = vault.join("锁定.md");
        fs::write(&locked_path, "不能删除").unwrap();
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked_path)
            .unwrap();

        let staging = repo.rebuild_staging(&snapshot).unwrap();
        let error = repo.apply_staging(&snapshot, &staging).unwrap_err();
        drop(locked);

        assert_eq!(error.code, "backup_restore_failed");
        assert_eq!(
            fs::read_to_string(vault.join("笔记.md")).unwrap(),
            "恢复前内容"
        );
        assert_eq!(fs::read_to_string(&locked_path).unwrap(), "不能删除");
        let _ = fs::remove_dir_all(staging);
        let _ = fs::remove_dir_all(vault.parent().unwrap());
    }

    #[test]
    fn restore_lock_blocks_page_writes_until_the_transaction_finishes() {
        let (vault, backups) = roots();
        let pages = PageRepository::open(vault.clone()).unwrap();
        let page = pages.create("页面".into(), None, "旧内容".into()).unwrap();
        let backups = BackupRepository::open_at(&vault, &backups).unwrap();
        let guard = backups.lock_vault().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = pages.update_checked(
                &page.id,
                page.title.clone(),
                "新内容".into(),
                &page.title,
                &page.content,
            );
            let _ = sender.send(result);
        });

        assert!(receiver
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());
        drop(guard);
        assert_eq!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap()
                .unwrap()
                .content,
            "新内容"
        );
        worker.join().unwrap();
        let _ = fs::remove_dir_all(vault.parent().unwrap());
    }
}
