#![cfg_attr(debug_assertions, allow(dead_code))]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PortablePaths {
    pub data_root: PathBuf,
    pub database_path: PathBuf,
    pub vault_root: PathBuf,
    pub backups_root: PathBuf,
    pub indexes_root: PathBuf,
    pub webview_root: PathBuf,
    pub exports_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct LegacyPaths {
    pub vault_root: PathBuf,
    pub backups_root: Option<PathBuf>,
    pub webview_local_storage: Option<PathBuf>,
}

impl PortablePaths {
    pub fn from_install_dir(install_dir: PathBuf) -> Self {
        let data_root = install_dir.join("data");
        Self {
            database_path: data_root.join("qingyu.sqlite3"),
            vault_root: data_root.join("vault"),
            backups_root: data_root.join("backups"),
            indexes_root: data_root.join("indexes"),
            webview_root: data_root.join("webview"),
            exports_root: data_root.join("exports"),
            data_root,
        }
    }

    pub fn current() -> io::Result<Self> {
        let executable = std::env::current_exe()?;
        let install_dir = executable
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法定位轻羽安装目录"))?;
        let mut paths = Self::from_install_dir(install_dir.to_path_buf());
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            paths.backups_root = PathBuf::from(local_app_data).join("QingYu").join("Backups");
        }
        Ok(paths)
    }

    fn from_data_root(data_root: PathBuf) -> Self {
        Self {
            database_path: data_root.join("qingyu.sqlite3"),
            vault_root: data_root.join("vault"),
            backups_root: data_root.join("backups"),
            indexes_root: data_root.join("indexes"),
            webview_root: data_root.join("webview"),
            exports_root: data_root.join("exports"),
            data_root,
        }
    }

    pub fn ensure(&self) -> io::Result<()> {
        for directory in [
            &self.data_root,
            &self.vault_root,
            &self.backups_root,
            &self.indexes_root,
            &self.webview_root,
            &self.exports_root,
        ] {
            fs::create_dir_all(directory)?;
        }
        Ok(())
    }
}

pub fn prepare() -> io::Result<PortablePaths> {
    let paths = PortablePaths::current()?;
    let install_backup = paths.data_root.join("backups");
    if install_backup != paths.backups_root && install_backup.exists() && !paths.backups_root.exists() {
        copy_tree_verified(&install_backup, &paths.backups_root)?;
    }
    if paths.data_root.exists() {
        paths.ensure()?;
        return Ok(paths);
    }

    #[cfg(debug_assertions)]
    {
        paths.ensure()?;
        return Ok(paths);
    }

    #[cfg(not(debug_assertions))]
    {
        let legacy = discover_legacy(&paths)?;
        migrate_legacy(&paths, &legacy)?;
        Ok(paths)
    }
}

#[cfg(not(debug_assertions))]
fn discover_legacy(paths: &PortablePaths) -> io::Result<LegacyPaths> {
    let profile = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let vault_root = profile.join("Documents").join("轻羽笔记库");
    let local_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let backups_root = local_data.as_ref().and_then(|root| {
        let id = vault_id(&vault_root);
        let path = root.join("QingYu").join("backups").join(id);
        path.exists().then_some(path)
    });
    let installed_webview = paths
        .data_root
        .parent()
        .map(|root| root.join("tauri-app.exe.WebView2"));
    let app_webview = local_data
        .as_ref()
        .map(|root| root.join("com.qingyu.notes"));
    let webview_local_storage = app_webview
        .map(|path| path.join("EBWebView").join("Default").join("Local Storage"))
        .filter(|path| path.exists())
        .or_else(|| {
            installed_webview
                .map(|path| path.join("EBWebView").join("Default").join("Local Storage"))
                .filter(|path| path.exists())
        });
    Ok(LegacyPaths {
        vault_root,
        backups_root,
        webview_local_storage,
    })
}

pub fn migrate_legacy(paths: &PortablePaths, legacy: &LegacyPaths) -> io::Result<()> {
    if paths.data_root.exists() {
        return Ok(());
    }
    let install_dir = paths.data_root.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "便携数据目录没有父目录")
    })?;
    fs::create_dir_all(install_dir)?;
    let staging_root = install_dir.join(format!(".data.migrating-{}", Uuid::new_v4()));
    let staging = PortablePaths::from_data_root(staging_root.clone());
    let result = (|| {
        fs::create_dir_all(&staging.data_root)?;
        if legacy.vault_root.exists() {
            copy_tree_verified(&legacy.vault_root, &staging.vault_root)?;
        } else {
            fs::create_dir_all(&staging.vault_root)?;
        }
        if let Some(source) = legacy.backups_root.as_ref().filter(|path| path.exists()) {
            copy_tree_verified(source, &staging.backups_root)?;
        } else {
            fs::create_dir_all(&staging.backups_root)?;
        }
        fs::create_dir_all(&staging.indexes_root)?;
        fs::create_dir_all(&staging.webview_root)?;
        fs::create_dir_all(&staging.exports_root)?;
        if let Some(source) = legacy
            .webview_local_storage
            .as_ref()
            .filter(|path| path.exists())
        {
            copy_tree_verified(
                source,
                &staging
                    .webview_root
                    .join("EBWebView")
                    .join("Default")
                    .join("Local Storage"),
            )?;
        }
        fs::write(
            staging.data_root.join("layout.json"),
            b"{\"version\":1,\"portable\":true}\n",
        )?;
        fs::rename(&staging.data_root, &paths.data_root)
    })();
    if result.is_err() && staging_root.exists() {
        let _ = fs::remove_dir_all(staging_root);
    }
    result
}

#[cfg(not(debug_assertions))]
fn vault_id(vault_root: &Path) -> String {
    let normalized = vault_root
        .canonicalize()
        .unwrap_or_else(|_| vault_root.to_path_buf())
        .to_string_lossy()
        .to_lowercase();
    format!("{:x}", Sha256::digest(normalized.as_bytes()))[..16].to_owned()
}

pub fn copy_tree_verified(source: &Path, destination: &Path) -> io::Result<()> {
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("目标已存在：{}", destination.display()),
        ));
    }
    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "迁移目标没有父目录")
    })?;
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(
        ".{}.migrating-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("data"),
        Uuid::new_v4()
    ));
    let result = (|| {
        copy_directory(source, &staging)?;
        verify_directory(source, &staging)?;
        fs::rename(&staging, destination)
    })();
    if result.is_err() && staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else {
            copy_file_contents(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn copy_file_contents(source: &Path, destination: &Path) -> io::Result<()> {
    let mut source_file = fs::File::open(source)?;
    let mut destination_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    io::copy(&mut source_file, &mut destination_file)?;
    destination_file.sync_all()
}

fn verify_directory(source: &Path, destination: &Path) -> io::Result<()> {
    let source_files = directory_manifest(source)?;
    let destination_files = directory_manifest(destination)?;
    if source_files != destination_files {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "迁移数据校验失败",
        ));
    }
    Ok(())
}

fn directory_manifest(root: &Path) -> io::Result<Vec<(PathBuf, u64, String)>> {
    fn visit(
        root: &Path,
        directory: &Path,
        output: &mut Vec<(PathBuf, u64, String)>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                visit(root, &path, output)?;
            } else {
                let bytes = fs::read(&path)?;
                output.push((
                    path.strip_prefix(root).unwrap_or(&path).to_path_buf(),
                    bytes.len() as u64,
                    format!("{:x}", Sha256::digest(&bytes)),
                ));
            }
        }
        Ok(())
    }

    let mut output = Vec::new();
    visit(root, root, &mut output)?;
    output.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use uuid::Uuid;

    use super::{copy_tree_verified, migrate_legacy, LegacyPaths, PortablePaths};

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("qingyu-portable-test-{}", Uuid::new_v4()))
    }

    #[test]
    fn portable_paths_stay_below_the_install_directory() {
        let install = temp_root().join("轻羽");
        let paths = PortablePaths::from_install_dir(install.clone());

        assert_eq!(paths.data_root, install.join("data"));
        assert_eq!(paths.database_path, install.join("data").join("qingyu.sqlite3"));
        assert_eq!(paths.vault_root, install.join("data").join("vault"));
        assert_eq!(paths.backups_root, install.join("data").join("backups"));
        assert_eq!(paths.indexes_root, install.join("data").join("indexes"));
        assert_eq!(paths.webview_root, install.join("data").join("webview"));
        assert_eq!(paths.exports_root, install.join("data").join("exports"));
    }

    #[test]
    fn verified_copy_preserves_nested_data_and_the_source() {
        let root = temp_root();
        let source = root.join("legacy");
        let destination = root.join("data").join("vault");
        fs::create_dir_all(source.join(".noteapp").join("databases")).unwrap();
        fs::write(source.join("页面.md"), "# 页面\n").unwrap();
        fs::write(
            source.join(".noteapp").join("databases").join("db.json"),
            "{\"id\":\"db\"}\n",
        )
        .unwrap();

        copy_tree_verified(&source, &destination).unwrap();

        assert_eq!(fs::read_to_string(destination.join("页面.md")).unwrap(), "# 页面\n");
        assert!(destination.join(".noteapp").join("databases").join("db.json").exists());
        assert!(source.join("页面.md").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verified_copy_never_overwrites_existing_data() {
        let root = temp_root();
        let source = root.join("legacy");
        let destination = root.join("data").join("vault");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("页面.md"), "旧内容").unwrap();
        fs::write(destination.join("页面.md"), "新内容").unwrap();

        assert!(copy_tree_verified(&source, &destination).is_err());
        assert_eq!(fs::read_to_string(destination.join("页面.md")).unwrap(), "新内容");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn verified_copy_does_not_preserve_source_file_attributes() {
        let root = temp_root();
        let source = root.join("legacy");
        let destination = root.join("data").join("vault");
        fs::create_dir_all(&source).unwrap();
        let source_file = source.join("页面.md");
        fs::write(&source_file, "只迁移文件内容").unwrap();
        let mut permissions = fs::metadata(&source_file).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&source_file, permissions).unwrap();

        copy_tree_verified(&source, &destination).unwrap();

        assert_eq!(
            fs::read_to_string(destination.join("页面.md")).unwrap(),
            "只迁移文件内容"
        );
        assert!(!fs::metadata(destination.join("页面.md"))
            .unwrap()
            .permissions()
            .readonly());
        let mut permissions = fs::metadata(&source_file).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&source_file, permissions).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_data_is_migrated_into_one_portable_root() {
        let root = temp_root();
        let install = root.join("轻羽");
        let legacy_vault = root.join("legacy-vault");
        let legacy_backups = root.join("legacy-backups");
        let legacy_webview = root.join("legacy-webview");
        fs::create_dir_all(&legacy_vault).unwrap();
        fs::create_dir_all(&legacy_backups).unwrap();
        fs::create_dir_all(&legacy_webview).unwrap();
        fs::write(legacy_vault.join("页面.md"), "笔记").unwrap();
        fs::write(legacy_backups.join("snapshot.json"), "备份").unwrap();
        fs::write(legacy_webview.join("settings.db"), "设置").unwrap();
        let paths = PortablePaths::from_install_dir(install);
        let legacy = LegacyPaths {
            vault_root: legacy_vault.clone(),
            backups_root: Some(legacy_backups.clone()),
            webview_local_storage: Some(legacy_webview.clone()),
        };

        migrate_legacy(&paths, &legacy).unwrap();

        assert!(paths.vault_root.join("页面.md").exists());
        assert!(paths.backups_root.join("snapshot.json").exists());
        assert!(paths
            .webview_root
            .join("EBWebView")
            .join("Default")
            .join("Local Storage")
            .join("settings.db")
            .exists());
        assert!(paths.data_root.join("layout.json").exists());
        assert!(paths.exports_root.exists());
        assert!(legacy_vault.join("页面.md").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
