use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use percent_encoding::percent_decode_str;
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag};
use pulldown_cmark_to_cmark::cmark;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::database::model::DatabaseFieldType;
use crate::database::repository::DatabaseRepository;
use crate::error::AppError;
use crate::pages::model::{BlockNode, Page, PageKind, PageSummary};
use crate::pages::repository::PageRepository;

const MARKDOWN_ASSETS_DIRECTORY: &str = "_qingyu-assets";
const DATABASE_MARKER: &str = "<!-- qingyu-database -->";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferManifest {
    version: u32,
    format: String,
    files: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferCommitJournal {
    prepared_name: String,
    rollback_name: String,
    attachments_existed: bool,
}

pub(crate) fn recover_interrupted_import(vault_root: &Path) -> Result<(), AppError> {
    let data_root = vault_root.parent().unwrap_or(vault_root);
    let commit_root = data_root.join(".transfer-commit");
    let journal_path = commit_root.join("journal.json");
    if !journal_path.exists() {
        return Ok(());
    }
    let journal: TransferCommitJournal = serde_json::from_slice(
        &fs::read(&journal_path).map_err(io_import)?,
    )
    .map_err(|error| AppError::new("transfer_recovery_failed", error.to_string()))?;
    if !safe_commit_name(&journal.prepared_name, ".attachments-import-")
        || !safe_commit_name(&journal.rollback_name, ".attachments-rollback-")
    {
        return Err(AppError::new("transfer_recovery_failed", "导入恢复日志路径不安全"));
    }

    let original_database = commit_root.join("original.sqlite3");
    if !original_database.is_file() {
        return Err(AppError::new("transfer_recovery_failed", "导入恢复数据库不存在"));
    }
    fs::copy(&original_database, data_root.join("qingyu.sqlite3")).map_err(io_import)?;
    let _ = fs::remove_file(data_root.join("qingyu.sqlite3-wal"));
    let _ = fs::remove_file(data_root.join("qingyu.sqlite3-shm"));

    let attachments = data_root.join("attachments");
    if attachments.exists() {
        fs::remove_dir_all(&attachments).map_err(io_import)?;
    }
    if journal.attachments_existed {
        copy_tree(&commit_root.join("original-attachments"), &attachments)?;
    } else {
        fs::create_dir_all(&attachments).map_err(io_import)?;
    }
    let _ = fs::remove_dir_all(data_root.join(&journal.prepared_name));
    let _ = fs::remove_dir_all(data_root.join(&journal.rollback_name));
    fs::remove_dir_all(commit_root).map_err(io_import)
}

fn safe_commit_name(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
        && !value[prefix.len()..].is_empty()
        && Path::new(value).components().count() == 1
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub pages: usize,
    pub files: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub pages: usize,
    pub databases: usize,
    pub files: usize,
    pub warnings: Vec<String>,
    pub source_kind: String,
}

pub fn export_lossless(repo: &PageRepository, destination: &Path) -> Result<PathBuf, AppError> {
    repo.with_write_lock(|| export_lossless_locked(repo, destination))
}

fn export_lossless_locked(repo: &PageRepository, destination: &Path) -> Result<PathBuf, AppError> {
    let staging = temporary_root("lossless-export")?;
    let result = (|| {
        let database = staging.join("qingyu.sqlite3");
        repo.snapshot_database(&database)?;
        let attachments = repo
            .root()
            .parent()
            .unwrap_or(repo.root())
            .join("attachments");
        if attachments.exists() {
            copy_tree(&attachments, &staging.join("attachments"))?;
        }
        let mut files = HashMap::new();
        collect_hashes(&staging, &staging, &mut files)?;
        fs::write(
            staging.join("manifest.json"),
            serde_json::to_vec_pretty(&TransferManifest {
                version: 1,
                format: "qingyu-lossless".into(),
                files,
            })
            .unwrap(),
        )
        .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
        zip_directory(&staging, destination)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

pub fn export_markdown(repo: &PageRepository, destination: &Path) -> Result<PathBuf, AppError> {
    repo.with_write_lock(|| export_markdown_locked(repo, destination))
}

fn export_markdown_locked(repo: &PageRepository, destination: &Path) -> Result<PathBuf, AppError> {
    let staging = temporary_root("markdown-export")?;
    let result = (|| {
        let pages = repo.list(false)?;
        let by_id = pages
            .iter()
            .map(|page| (page.id.as_str(), page))
            .collect::<HashMap<_, _>>();
        let paths = export_paths(&pages, &by_id);
        for page in &pages {
            let relative = paths
                .get(&page.id)
                .ok_or_else(|| AppError::new("transfer_export_failed", "无法生成页面导出路径"))?;
            let directory = staging.join(relative.parent().unwrap_or(Path::new("")));
            fs::create_dir_all(&directory)
                .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
            let bundle = repo.open_bundle(&page.id)?;
            let markdown = document_markdown(&bundle.document.blocks, relative, &paths);
            let marker = if page.kind == PageKind::Database {
                format!("{DATABASE_MARKER}\n\n")
            } else {
                String::new()
            };
            fs::write(
                staging.join(relative),
                format!("{marker}# {}\n\n{}", page.title, markdown.trim_start()),
            )
            .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
            if page.kind == PageKind::Database {
                export_database_csv(repo, page, &staging.join(relative).with_extension("csv"))?;
            }
        }
        let attachments = repo
            .root()
            .parent()
            .unwrap_or(repo.root())
            .join("attachments");
        if attachments.exists() {
            copy_tree(&attachments, &staging.join(MARKDOWN_ASSETS_DIRECTORY))?;
        }
        zip_directory(&staging, destination)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

pub fn import_content(repo: &PageRepository, source: &Path) -> Result<ImportReport, AppError> {
    repo.with_write_lock(|| import_content_locked(repo, source))
}

fn import_content_locked(repo: &PageRepository, source: &Path) -> Result<ImportReport, AppError> {
    let checkpoint = temporary_root("import-checkpoint")?;
    let original_database = checkpoint.join("original.sqlite3");
    repo.snapshot_database(&original_database)?;
    let attachment_root = repo
        .root()
        .parent()
        .unwrap_or(repo.root())
        .join("attachments");
    let original_attachments = checkpoint.join("original-attachments");
    if attachment_root.exists() {
        copy_tree(&attachment_root, &original_attachments)?;
    }

    let working_root = checkpoint.join("working");
    let working_vault = working_root.join("vault");
    fs::create_dir_all(&working_vault).map_err(io_import)?;
    fs::copy(&original_database, working_root.join("qingyu.sqlite3")).map_err(io_import)?;
    let working_attachments = working_root.join("attachments");
    if original_attachments.exists() {
        copy_tree(&original_attachments, &working_attachments)?;
    } else {
        fs::create_dir_all(&working_attachments).map_err(io_import)?;
    }
    let working_repo = PageRepository::open(working_vault)?;
    let result = import_content_unchecked(&working_repo, source);
    let report = match result {
        Ok(report) => report,
        Err(error) => {
            let _ = fs::remove_dir_all(&checkpoint);
            return Err(error);
        }
    };

    let attachment_parent = attachment_root.parent().unwrap_or(repo.root());
    let prepared_attachments =
        attachment_parent.join(format!(".attachments-import-{}", Uuid::new_v4()));
    let rollback_attachments =
        attachment_parent.join(format!(".attachments-rollback-{}", Uuid::new_v4()));
    copy_tree(&working_attachments, &prepared_attachments)?;
    let commit_root = attachment_parent.join(".transfer-commit");
    if commit_root.exists() {
        fs::remove_dir_all(&commit_root).map_err(io_import)?;
    }
    fs::create_dir_all(&commit_root).map_err(io_import)?;
    fs::copy(&original_database, commit_root.join("original.sqlite3")).map_err(io_import)?;
    if original_attachments.exists() {
        copy_tree(&original_attachments, &commit_root.join("original-attachments"))?;
    }
    let journal = TransferCommitJournal {
        prepared_name: prepared_attachments.file_name().and_then(|value| value.to_str()).unwrap_or_default().into(),
        rollback_name: rollback_attachments.file_name().and_then(|value| value.to_str()).unwrap_or_default().into(),
        attachments_existed: original_attachments.exists(),
    };
    fs::write(
        commit_root.join("journal.json"),
        serde_json::to_vec_pretty(&journal).map_err(|error| AppError::new("transfer_import_failed", error.to_string()))?,
    ).map_err(io_import)?;

    let commit = (|| {
        if attachment_root.exists() {
            move_directory_cross_device(&attachment_root, &rollback_attachments)?;
        }
        if let Err(error) = move_directory_cross_device(&prepared_attachments, &attachment_root) {
            if rollback_attachments.exists() {
                let _ = move_directory_cross_device(&rollback_attachments, &attachment_root);
            }
            return Err(error);
        }
        if let Err(error) = repo.restore_database(working_repo.database_path()) {
            let _ = fs::remove_dir_all(&attachment_root);
            if rollback_attachments.exists() {
                let _ = move_directory_cross_device(&rollback_attachments, &attachment_root);
            }
            return Err(error);
        }
        let _ = fs::remove_dir_all(&rollback_attachments);
        Ok::<(), AppError>(())
    })();
    if let Err(commit_error) = commit {
        let rollback = (|| {
            repo.restore_database(&original_database)?;
            if !attachment_root.exists() {
                if original_attachments.exists() {
                    copy_tree(&original_attachments, &attachment_root)?;
                } else {
                    fs::create_dir_all(&attachment_root).map_err(io_import)?;
                }
            }
            Ok::<(), AppError>(())
        })();
        let _ = fs::remove_dir_all(&prepared_attachments);
        let _ = fs::remove_dir_all(&rollback_attachments);
        let _ = fs::remove_dir_all(&commit_root);
        let _ = fs::remove_dir_all(&checkpoint);
        return match rollback {
            Ok(()) => Err(commit_error),
            Err(error) => Err(AppError::new(
                "transfer_rollback_failed",
                format!("提交失败且回滚失败：{}", error.message),
            )),
        };
    }
    let _ = fs::remove_dir_all(&commit_root);
    let _ = fs::remove_dir_all(checkpoint);
    Ok(report)
}

pub fn preview_content(source: &Path) -> Result<ImportPreview, AppError> {
    if !source.exists() {
        return Err(AppError::new("transfer_source_missing", "导入路径不存在"));
    }
    if source
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
    {
        let staging = temporary_root("content-preview")?;
        let result = (|| {
            extract_safe(source, &staging)?;
            preview_directory(&staging, "ZIP 压缩包")
        })();
        let _ = fs::remove_dir_all(staging);
        return result;
    }
    if source.is_dir() {
        return preview_directory(source, "文件夹");
    }
    preview_files(
        source.parent().unwrap_or(Path::new(".")),
        &[source.to_path_buf()],
        "单个文件",
    )
}

fn preview_directory(root: &Path, fallback_kind: &str) -> Result<ImportPreview, AppError> {
    let manifest = root.join("manifest.json");
    let database = root.join("qingyu.sqlite3");
    if manifest.exists() && database.exists() {
        let value = validate_lossless_package(root)?;
        let connection =
            Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|error| AppError::new("transfer_preview_failed", error.to_string()))?;
        let pages = connection
            .query_row("SELECT COUNT(*) FROM pages", [], |row| row.get::<_, i64>(0))
            .map_err(|error| AppError::new("transfer_preview_failed", error.to_string()))?
            as usize;
        let databases = connection
            .query_row("SELECT COUNT(*) FROM databases", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|error| AppError::new("transfer_preview_failed", error.to_string()))?
            as usize;
        return Ok(ImportPreview {
            pages,
            databases,
            files: value.files.len(),
            warnings: Vec::new(),
            source_kind: "轻羽无损包".into(),
        });
    }
    let mut files = Vec::new();
    collect_content_files(root, &mut files)?;
    preview_files(root, &files, fallback_kind)
}

fn preview_files(
    root: &Path,
    files: &[PathBuf],
    source_kind: &str,
) -> Result<ImportPreview, AppError> {
    let mut pages = 0usize;
    let mut databases = 0usize;
    let mut parent_directories = HashSet::new();
    let mut content_files = 0usize;
    let database_companions = files
        .iter()
        .filter(|path| path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("md")))
        .filter(|path| fs::read_to_string(path).is_ok_and(|content| content.contains(DATABASE_MARKER)))
        .map(|path| path_key(&path.with_extension("")))
        .collect::<HashSet<_>>();
    for path in files {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "md" | "txt" | "csv") {
            continue;
        }
        content_files += 1;
        if let Some(parent) = path
            .parent()
            .and_then(|parent| parent.strip_prefix(root).ok())
        {
            let mut current = PathBuf::new();
            for component in parent.components() {
                current.push(component.as_os_str());
                if !current.as_os_str().is_empty() {
                    parent_directories.insert(current.clone());
                }
            }
        }
        if extension == "csv" {
            let mut reader = csv::ReaderBuilder::new()
                .flexible(true)
                .from_path(path)
                .map_err(|error| AppError::new("transfer_csv_failed", error.to_string()))?;
            let headers = reader
                .headers()
                .map_err(|error| AppError::new("transfer_csv_failed", error.to_string()))?;
            if headers.is_empty() {
                return Err(AppError::new("transfer_csv_failed", "CSV 缺少标题行"));
            }
            pages += 1 + reader
                .records()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| AppError::new("transfer_csv_failed", error.to_string()))?
                .len();
            databases += 1;
        } else if !(extension == "md" && database_companions.contains(&path_key(&path.with_extension("")))) {
            fs::read_to_string(path).map_err(io_import)?;
            pages += 1;
        } else {
            fs::read_to_string(path).map_err(io_import)?;
        }
    }
    pages += parent_directories.len();
    let warnings = if content_files == 0 {
        vec!["未找到可导入的 Markdown、CSV 或 TXT 文件".into()]
    } else {
        Vec::new()
    };
    Ok(ImportPreview {
        pages,
        databases,
        files: content_files,
        warnings,
        source_kind: source_kind.into(),
    })
}

fn import_content_unchecked(
    repo: &PageRepository,
    source: &Path,
) -> Result<ImportReport, AppError> {
    if source
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
    {
        let staging = temporary_root("content-import")?;
        let result = (|| {
            extract_safe(source, &staging)?;
            import_directory(repo, &staging)
        })();
        let _ = fs::remove_dir_all(staging);
        return result;
    }
    if source.is_dir() {
        return import_directory(repo, source);
    }
    import_files(
        repo,
        source.parent().unwrap_or(Path::new(".")),
        &[source.to_path_buf()],
    )
}

fn import_directory(repo: &PageRepository, root: &Path) -> Result<ImportReport, AppError> {
    let manifest = root.join("manifest.json");
    if manifest.exists() && root.join("qingyu.sqlite3").exists() {
        let value = validate_lossless_package(root)?;
        repo.restore_database(&root.join("qingyu.sqlite3"))?;
        let attachments = root.join("attachments");
        let destination = repo
            .root()
            .parent()
            .unwrap_or(repo.root())
            .join("attachments");
        if destination.exists() {
            fs::remove_dir_all(&destination).map_err(io_import)?;
        }
        if attachments.exists() {
            copy_tree(&attachments, &destination)?;
        } else {
            fs::create_dir_all(&destination).map_err(io_import)?;
        }
        return Ok(ImportReport {
            pages: repo.list(false)?.len(),
            files: value.files.len(),
            warnings: Vec::new(),
        });
    }
    let mut files = Vec::new();
    collect_content_files(root, &mut files)?;
    import_files(repo, root, &files)
}

fn validate_lossless_package(root: &Path) -> Result<TransferManifest, AppError> {
    let manifest_path = root.join("manifest.json");
    let value: TransferManifest =
        serde_json::from_slice(&fs::read(&manifest_path).map_err(io_import)?)
            .map_err(|error| AppError::new("transfer_invalid_manifest", error.to_string()))?;
    if value.version != 1 || value.format != "qingyu-lossless" {
        return Err(AppError::new(
            "transfer_invalid_manifest",
            "不是受支持的轻羽无损包",
        ));
    }
    if !value.files.contains_key("qingyu.sqlite3") {
        return Err(AppError::new(
            "transfer_invalid_manifest",
            "无损包清单缺少数据库",
        ));
    }
    let mut actual = Vec::new();
    collect_all_files(root, &mut actual)?;
    let actual = actual
        .into_iter()
        .filter_map(|path| {
            path.strip_prefix(root)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        })
        .filter(|relative| relative != "manifest.json")
        .collect::<HashSet<_>>();
    let expected = value.files.keys().cloned().collect::<HashSet<_>>();
    if actual != expected {
        return Err(AppError::new(
            "transfer_invalid_manifest",
            "无损包文件与清单不一致",
        ));
    }
    for (relative, expected_hash) in &value.files {
        let bytes = fs::read(root.join(safe_relative(relative)?)).map_err(io_import)?;
        if hash(&bytes) != *expected_hash {
            return Err(AppError::new(
                "transfer_hash_mismatch",
                format!("文件校验失败：{relative}"),
            ));
        }
    }
    validate_lossless_database(root, &value)?;
    Ok(value)
}

fn validate_lossless_database(root: &Path, manifest: &TransferManifest) -> Result<(), AppError> {
    let connection = Connection::open_with_flags(
        root.join("qingyu.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| AppError::new("transfer_invalid_database", error.to_string()))?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| AppError::new("transfer_invalid_database", error.to_string()))?;
    if integrity != "ok" {
        return Err(AppError::new(
            "transfer_invalid_database",
            format!("数据库完整性检查失败：{integrity}"),
        ));
    }
    let foreign_key_errors: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|error| AppError::new("transfer_invalid_database", error.to_string()))?;
    if foreign_key_errors != 0 {
        return Err(AppError::new(
            "transfer_invalid_database",
            format!("数据库存在 {foreign_key_errors} 个外键错误"),
        ));
    }
    let schema_version: i64 = connection
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .map_err(|error| AppError::new("transfer_invalid_database", error.to_string()))?;
    if !(1..=2).contains(&schema_version) {
        return Err(AppError::new(
            "transfer_invalid_database",
            format!("不支持的数据库版本：{schema_version}"),
        ));
    }

    let mut referenced = HashSet::new();
    for table in ["attachments", "conversation_attachments"] {
        let mut statement = connection
            .prepare(&format!("SELECT relative_path FROM {table}"))
            .map_err(|error| AppError::new("transfer_invalid_database", error.to_string()))?;
        let paths = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::new("transfer_invalid_database", error.to_string()))?;
        for relative in paths {
            let relative = relative
                .map_err(|error| AppError::new("transfer_invalid_database", error.to_string()))?
                .replace('\\', "/");
            let safe = safe_relative(&relative)?;
            if !relative.starts_with("attachments/")
                || !root.join(&safe).is_file()
                || !manifest.files.contains_key(&relative)
            {
                return Err(AppError::new(
                    "transfer_missing_attachment",
                    format!("附件缺失或不在清单中：{relative}"),
                ));
            }
            referenced.insert(relative);
        }
    }
    let packaged = manifest
        .files
        .keys()
        .filter(|path| path.starts_with("attachments/"))
        .cloned()
        .collect::<HashSet<_>>();
    if referenced != packaged {
        return Err(AppError::new(
            "transfer_orphan_attachment",
            "无损包包含未被数据库引用的附件",
        ));
    }
    Ok(())
}

fn import_files(
    repo: &PageRepository,
    root: &Path,
    files: &[PathBuf],
) -> Result<ImportReport, AppError> {
    let mut pages = 0;
    let mut warnings = Vec::new();
    let mut parents: HashMap<PathBuf, String> = HashMap::new();
    let database_companions = files
        .iter()
        .filter(|path| path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("md")))
        .filter(|path| fs::read_to_string(path).is_ok_and(|content| content.contains(DATABASE_MARKER)))
        .map(|path| path_key(&path.with_extension("")))
        .collect::<HashSet<_>>();
    let page_files = files
        .iter()
        .filter(|path| {
            let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("");
            (extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("txt"))
                && !(extension.eq_ignore_ascii_case("md")
                    && database_companions.contains(&path_key(&path.with_extension(""))))
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut imported_pages = Vec::new();
    let mut page_ids = HashMap::new();

    for path in &page_files {
        let parent_id = ensure_parent_pages(repo, root, path, &mut parents, &mut pages)?;
        let title = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("未命名页面")
            .trim()
            .to_owned();
        let page = repo.create(title, parent_id, String::new())?;
        page_ids.insert(path_key(path), page.id.clone());
        imported_pages.push((path.clone(), page));
        pages += 1;
    }

    let imported_files = files
        .iter()
        .filter(|path| {
            matches!(
                path.extension().and_then(|value| value.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),
                "md" | "txt" | "csv"
            )
        })
        .count();
    for path in files {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if extension == "csv" {
            let parent_id = ensure_parent_pages(repo, root, path, &mut parents, &mut pages)?;
            let (created, database) = import_csv_database(repo, path, parent_id)?;
            pages += created;
            page_ids.insert(path_key(path), database.id.clone());
            if let Some(companion) = files.iter().find(|candidate| {
                candidate.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("md"))
                    && path_key(&candidate.with_extension("")) == path_key(&path.with_extension(""))
                    && database_companions.contains(&path_key(&candidate.with_extension("")))
            }) {
                page_ids.insert(path_key(companion), database.id.clone());
                imported_pages.push((companion.clone(), database));
            }
            continue;
        }
    }

    let mut imported_assets = HashMap::new();
    for (path, page) in imported_pages {
        let content = fs::read_to_string(&path).map_err(io_import)?;
        let rewritten = rewrite_import_markdown(
            repo,
            root,
            &path,
            &page.id,
            &content,
            &page_ids,
            &mut imported_assets,
            &mut warnings,
        )?;
        repo.update(&page.id, page.title, rewritten)?;
    }
    if pages == 0 {
        warnings.push("未找到可导入的 Markdown、CSV 或 TXT 文件".into());
    }
    Ok(ImportReport {
        pages,
        files: imported_files,
        warnings,
    })
}

fn rewrite_import_markdown(
    repo: &PageRepository,
    import_root: &Path,
    source: &Path,
    page_id: &str,
    markdown: &str,
    page_ids: &HashMap<PathBuf, String>,
    imported_assets: &mut HashMap<(String, PathBuf), String>,
    warnings: &mut Vec<String>,
) -> Result<String, AppError> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES;
    let events = Parser::new_ext(markdown, options).map(|event| match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from(rewrite_import_href(
                repo,
                import_root,
                source,
                page_id,
                dest_url.as_ref(),
                false,
                page_ids,
                imported_assets,
                warnings,
            )),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: CowStr::from(rewrite_import_href(
                repo,
                import_root,
                source,
                page_id,
                dest_url.as_ref(),
                true,
                page_ids,
                imported_assets,
                warnings,
            )),
            title,
            id,
        }),
        event => event,
    });
    let mut output = String::new();
    cmark(events, &mut output)
        .map_err(|error| AppError::new("transfer_markdown_failed", error.to_string()))?;
    Ok(output)
}

fn rewrite_import_href(
    repo: &PageRepository,
    import_root: &Path,
    source: &Path,
    page_id: &str,
    href: &str,
    image: bool,
    page_ids: &HashMap<PathBuf, String>,
    imported_assets: &mut HashMap<(String, PathBuf), String>,
    warnings: &mut Vec<String>,
) -> String {
    let href = href.trim().trim_start_matches('<').trim_end_matches('>');
    if href.is_empty()
        || href.starts_with('#')
        || href.starts_with("qingyu:")
        || href.starts_with("data:")
        || href.starts_with("http:")
        || href.starts_with("https:")
        || href.starts_with("mailto:")
    {
        return href.to_owned();
    }
    let (path_part, anchor) = href
        .split_once('#')
        .map(|(path, anchor)| (path, Some(anchor)))
        .unwrap_or((href, None));
    let decoded = percent_decode_str(path_part)
        .decode_utf8_lossy()
        .replace('/', &std::path::MAIN_SEPARATOR.to_string());
    let resolved = source
        .parent()
        .unwrap_or(Path::new("."))
        .join(decoded.as_str());
    let canonical_root = fs::canonicalize(import_root).unwrap_or_else(|_| import_root.to_path_buf());
    let canonical_resolved = fs::canonicalize(&resolved).unwrap_or_else(|_| resolved.clone());
    if !canonical_resolved.starts_with(&canonical_root) {
        warnings.push(format!("未导入目录之外的链接目标 {}", resolved.display()));
        return href.to_owned();
    }
    if let Some(target_id) = page_ids.get(&path_key(&resolved)) {
        return match anchor {
            Some(anchor) if !anchor.is_empty() => format!("qingyu://page/{target_id}#{anchor}"),
            _ => format!("qingyu://page/{target_id}"),
        };
    }
    if resolved.is_file() {
        let key = (page_id.to_owned(), path_key(&resolved));
        if let Some(imported) = imported_assets.get(&key) {
            return imported.clone();
        }
        let name = resolved
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(if image {
                "导入图片.png"
            } else {
                "导入附件"
            });
        match fs::read(&resolved)
            .map_err(io_import)
            .and_then(|bytes| repo.store_asset(page_id, name, &bytes))
        {
            Ok(imported) => {
                imported_assets.insert(key, imported.clone());
                return imported;
            }
            Err(error) => warnings.push(format!(
                "未导入附件 {}：{}",
                resolved.display(),
                error.message
            )),
        }
    } else {
        warnings.push(format!("未找到链接目标 {}", resolved.display()));
    }
    href.to_owned()
}

fn path_key(path: &Path) -> PathBuf {
    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    PathBuf::from(absolute.to_string_lossy().to_lowercase())
}

fn ensure_parent_pages(
    repo: &PageRepository,
    root: &Path,
    path: &Path,
    parents: &mut HashMap<PathBuf, String>,
    pages: &mut usize,
) -> Result<Option<String>, AppError> {
    let relative_parent = path
        .parent()
        .and_then(|parent| parent.strip_prefix(root).ok())
        .unwrap_or(Path::new(""));
    if relative_parent.as_os_str().is_empty() {
        return Ok(None);
    }
    if let Some(id) = parents.get(relative_parent) {
        return Ok(Some(id.clone()));
    }
    let mut current = PathBuf::new();
    let mut parent = None;
    for component in relative_parent.components() {
        current.push(component.as_os_str());
        if let Some(id) = parents.get(&current) {
            parent = Some(id.clone());
            continue;
        }
        let folder = component.as_os_str().to_string_lossy().to_string();
        let page = repo.create(folder, parent.clone(), String::new())?;
        parent = Some(page.id.clone());
        parents.insert(current.clone(), page.id);
        *pages += 1;
    }
    Ok(parent)
}

fn import_csv_database(
    repo: &PageRepository,
    path: &Path,
    parent_id: Option<String>,
) -> Result<(usize, Page), AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .map_err(|error| AppError::new("transfer_csv_failed", error.to_string()))?;
    let headers = reader
        .headers()
        .map_err(|error| AppError::new("transfer_csv_failed", error.to_string()))?
        .clone();
    if headers.is_empty() {
        return Err(AppError::new("transfer_csv_failed", "CSV 缺少标题行"));
    }
    let rows = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::new("transfer_csv_failed", error.to_string()))?;
    let title = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("未命名数据库")
        .trim()
        .to_owned();
    let databases = DatabaseRepository::from_pages(repo.clone())?;
    let (database, definition) = databases.create_database(title, parent_id)?;
    let mut fields = Vec::new();
    for (index, name) in headers.iter().enumerate().skip(1) {
        let values = rows
            .iter()
            .filter_map(|row| row.get(index))
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>();
        let field_type =
            if !values.is_empty() && values.iter().all(|value| value.parse::<f64>().is_ok()) {
                DatabaseFieldType::Number
            } else if !values.is_empty()
                && values.iter().all(|value| {
                    matches!(
                        value.trim().to_ascii_lowercase().as_str(),
                        "true" | "false" | "yes" | "no" | "是" | "否"
                    )
                })
            {
                DatabaseFieldType::Checkbox
            } else {
                DatabaseFieldType::Text
            };
        fields.push(databases.add_field(
            &definition.id,
            if name.trim().is_empty() {
                format!("字段 {index}")
            } else {
                name.trim().to_owned()
            },
            field_type,
            Vec::new(),
        )?);
    }
    for row in &rows {
        let mut properties = BTreeMap::new();
        for (index, field) in fields.iter().enumerate() {
            let raw = row.get(index + 1).unwrap_or("").trim();
            let value = match field.field_type {
                DatabaseFieldType::Number => raw
                    .parse::<f64>()
                    .map(serde_json::Number::from_f64)
                    .ok()
                    .flatten()
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null),
                DatabaseFieldType::Checkbox => serde_json::Value::Bool(matches!(
                    raw.to_ascii_lowercase().as_str(),
                    "true" | "yes" | "是"
                )),
                _ => serde_json::Value::String(raw.to_owned()),
            };
            properties.insert(field.id.clone(), value);
        }
        databases.create_record(
            &definition.id,
            row.get(0)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("未命名记录")
                .trim()
                .to_owned(),
            properties,
        )?;
    }
    Ok((rows.len() + 1, database))
}

fn export_paths<'a>(
    pages: &'a [PageSummary],
    by_id: &HashMap<&'a str, &'a PageSummary>,
) -> HashMap<String, PathBuf> {
    let mut result = HashMap::new();
    let mut used = HashSet::new();
    for page in pages {
        let mut directory = PathBuf::new();
        let mut parents = Vec::new();
        let mut parent = page.parent_id.as_deref();
        while let Some(id) = parent {
            let Some(value) = by_id.get(id) else {
                break;
            };
            parents.push(safe_name(&value.title));
            parent = value.parent_id.as_deref();
        }
        for value in parents.into_iter().rev() {
            directory.push(value);
        }
        let base = safe_name(&page.title);
        let mut candidate = directory.join(format!("{base}.md"));
        let mut suffix = 0usize;
        while !used.insert(candidate.to_string_lossy().to_lowercase()) {
            suffix += 1;
            let id = &page.id[..page.id.len().min(8)];
            candidate = directory.join(format!("{base}-{id}-{suffix}.md"));
        }
        result.insert(page.id.clone(), candidate);
    }
    result
}

fn document_markdown(
    blocks: &[BlockNode],
    source: &Path,
    paths: &HashMap<String, PathBuf>,
) -> String {
    fn append(
        blocks: &[BlockNode],
        source: &Path,
        paths: &HashMap<String, PathBuf>,
        depth: usize,
        output: &mut Vec<String>,
    ) {
        for block in blocks {
            let markdown =
                block
                    .attrs
                    .get("node")
                    .map(|node| node_markdown(node, source, paths, 0))
                    .unwrap_or_else(|| block.text.clone());
            let indent = "  ".repeat(depth);
            let body = markdown
                .lines()
                .map(|line| format!("{indent}{line}"))
                .collect::<Vec<_>>()
                .join("\n");
            output.push(format!("{indent}<!-- qingyu-block:{} -->\n{body}", block.id));
            append(&block.children, source, paths, depth + 1, output);
        }
    }
    let mut output = Vec::new();
    append(blocks, source, paths, 0, &mut output);
    output.join("\n\n") + "\n"
}

fn node_markdown(
    node: &serde_json::Value,
    source: &Path,
    paths: &HashMap<String, PathBuf>,
    depth: usize,
) -> String {
    let node_type = node
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("paragraph");
    let content = node
        .get("content")
        .and_then(serde_json::Value::as_array)
        .map(|children| {
            children
                .iter()
                .map(|child| node_markdown(child, source, paths, depth + 1))
                .collect::<String>()
        })
        .unwrap_or_default();
    match node_type {
        "doc" => node
            .get("content")
            .and_then(serde_json::Value::as_array)
            .map(|children| {
                children
                    .iter()
                    .map(|child| node_markdown(child, source, paths, depth))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            })
            .unwrap_or_default(),
        "text" => apply_marks(
            node.get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
            node.get("marks"),
            source,
            paths,
        ),
        "paragraph" => content,
        "heading" => format!(
            "{} {}",
            "#".repeat(
                node.pointer("/attrs/level")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1)
                    .clamp(1, 6) as usize
            ),
            content
        ),
        "bulletList" | "orderedList" | "taskList" => node
            .get("content")
            .and_then(serde_json::Value::as_array)
            .map(|children| {
                children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| {
                        let body = node_markdown(child, source, paths, depth + 1);
                        let marker = if node_type == "orderedList" {
                            format!("{}. ", index + 1)
                        } else {
                            "- ".into()
                        };
                        format!("{marker}{}", body.replace('\n', "\n  "))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        "listItem" => content,
        "taskItem" => format!(
            "[{}] {}",
            if node
                .pointer("/attrs/checked")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                "x"
            } else {
                " "
            },
            content
        ),
        "blockquote" => content
            .lines()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        "horizontalRule" => "---".into(),
        "hardBreak" => "  \n".into(),
        "codeBlock" => format!(
            "```{}\n{}\n```",
            node.pointer("/attrs/language")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
            content
        ),
        "image" => {
            let alt = node
                .pointer("/attrs/alt")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("图片");
            let src = node
                .pointer("/attrs/title")
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    node.pointer("/attrs/src")
                        .and_then(serde_json::Value::as_str)
                })
                .unwrap_or("");
            format!("![{}]({})", alt, export_href(src, source, paths))
        }
        "table" => table_markdown(node, source, paths),
        "tableRow" | "tableHeader" | "tableCell" => content,
        _ => content,
    }
}

fn apply_marks(
    text: &str,
    marks: Option<&serde_json::Value>,
    source: &Path,
    paths: &HashMap<String, PathBuf>,
) -> String {
    let mut value = text.to_owned();
    if let Some(marks) = marks.and_then(serde_json::Value::as_array) {
        for mark in marks {
            match mark
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
            {
                "bold" => value = format!("**{value}**"),
                "italic" => value = format!("*{value}*"),
                "strike" => value = format!("~~{value}~~"),
                "code" => value = format!("`{value}`"),
                "link" => {
                    if let Some(href) = mark
                        .pointer("/attrs/href")
                        .and_then(serde_json::Value::as_str)
                    {
                        value = format!("[{value}]({})", export_href(href, source, paths));
                    }
                }
                _ => {}
            }
        }
    }
    value
}

fn export_href(href: &str, source: &Path, paths: &HashMap<String, PathBuf>) -> String {
    if let Some(target) = href.strip_prefix("qingyu://page/") {
        let (page_id, anchor) = target
            .split_once('#')
            .map(|(page, block)| (page, format!("#{block}")))
            .unwrap_or((target, String::new()));
        if let Some(path) = paths.get(page_id) {
            return format!(
                "{}{}",
                relative_path(source.parent().unwrap_or(Path::new("")), path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                anchor
            );
        }
    }
    if let Some(asset) = href.strip_prefix("/.noteapp/assets/") {
        return relative_path(
            source.parent().unwrap_or(Path::new("")),
            &PathBuf::from(MARKDOWN_ASSETS_DIRECTORY).join(asset),
        )
        .to_string_lossy()
        .replace('\\', "/");
    }
    href.to_owned()
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = from.components().collect::<Vec<_>>();
    let to = to.components().collect::<Vec<_>>();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = PathBuf::new();
    for _ in common..from.len() {
        result.push("..");
    }
    for component in &to[common..] {
        result.push(component.as_os_str());
    }
    result
}

fn table_markdown(
    node: &serde_json::Value,
    source: &Path,
    paths: &HashMap<String, PathBuf>,
) -> String {
    let rows = node
        .get("content")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let values = rows
        .iter()
        .map(|row| {
            row.get("content")
                .and_then(serde_json::Value::as_array)
                .map(|cells| {
                    cells
                        .iter()
                        .map(|cell| {
                            node_markdown(cell, source, paths, 0)
                                .replace('|', "\\|")
                                .replace('\n', " ")
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let columns = values.first().map(Vec::len).unwrap_or(0);
    if columns == 0 {
        return String::new();
    }
    let mut lines = Vec::new();
    lines.push(format!("| {} |", values[0].join(" | ")));
    lines.push(format!("| {} |", vec!["---"; columns].join(" | ")));
    for row in values.iter().skip(1) {
        lines.push(format!("| {} |", row.join(" | ")));
    }
    lines.join("\n")
}

fn export_database_csv(
    repo: &PageRepository,
    page: &PageSummary,
    destination: &Path,
) -> Result<(), AppError> {
    let databases = DatabaseRepository::from_pages(repo.clone())?;
    let definition = databases.get(&page.id)?;
    let records = databases.query(&page.id, Default::default())?;
    let mut writer = csv::Writer::from_path(destination)
        .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
    let mut headers = vec!["Name".to_owned()];
    headers.extend(definition.fields.iter().map(|field| field.name.clone()));
    writer
        .write_record(headers)
        .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
    for record in records {
        let mut row = vec![record.title];
        row.extend(definition.fields.iter().map(|field| {
            record
                .properties
                .get(&field.id)
                .map(csv_value)
                .unwrap_or_default()
        }));
        writer
            .write_record(row)
            .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
    }
    writer
        .flush()
        .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))
}

fn csv_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => {
            values.iter().map(csv_value).collect::<Vec<_>>().join(", ")
        }
        value => value.to_string(),
    }
}

fn zip_directory(root: &Path, destination: &Path) -> Result<PathBuf, AppError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
    }
    let file = fs::File::create(destination)
        .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut files = Vec::new();
    collect_all_files(root, &mut files)?;
    for path in files {
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        archive
            .start_file(relative, options)
            .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
        archive
            .write_all(
                &fs::read(path)
                    .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?,
            )
            .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
    }
    archive
        .finish()
        .map_err(|error| AppError::new("transfer_export_failed", error.to_string()))?;
    Ok(destination.to_path_buf())
}

fn extract_safe(source: &Path, destination: &Path) -> Result<(), AppError> {
    let mut archive = ZipArchive::new(fs::File::open(source).map_err(io_import)?)
        .map_err(|error| AppError::new("transfer_import_failed", error.to_string()))?;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| AppError::new("transfer_import_failed", error.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| AppError::new("transfer_unsafe_archive", "压缩包包含不安全路径"))?;
        let output = destination.join(enclosed);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(io_import)?;
        }
        let mut file = fs::File::create(output).map_err(io_import)?;
        std::io::copy(&mut entry.take(128 * 1024 * 1024), &mut file).map_err(io_import)?;
    }
    Ok(())
}
fn collect_all_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), AppError> {
    for entry in fs::read_dir(root).map_err(io_import)? {
        let path = entry.map_err(io_import)?.path();
        if path.is_dir() {
            collect_all_files(&path, output)?
        } else {
            output.push(path)
        }
    }
    Ok(())
}
fn collect_content_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), AppError> {
    fn visit(import_root: &Path, directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), AppError> {
        for entry in fs::read_dir(directory).map_err(io_import)? {
            let path = entry.map_err(io_import)?.path();
            if path == import_root.join(MARKDOWN_ASSETS_DIRECTORY) {
                continue;
            }
            if path.is_dir() {
                visit(import_root, &path, output)?
            } else {
                output.push(path)
            }
        }
        Ok(())
    }
    visit(root, root, output)?;
    output.sort();
    Ok(())
}
fn collect_hashes(
    root: &Path,
    directory: &Path,
    output: &mut HashMap<String, String>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(directory).map_err(io_import)? {
        let path = entry.map_err(io_import)?.path();
        if path.is_dir() {
            collect_hashes(root, &path, output)?
        } else {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            output.insert(relative, hash(&fs::read(path).map_err(io_import)?));
        }
    }
    Ok(())
}
fn copy_tree(source: &Path, destination: &Path) -> Result<(), AppError> {
    fs::create_dir_all(destination).map_err(io_import)?;
    for entry in fs::read_dir(source).map_err(io_import)? {
        let entry = entry.map_err(io_import)?;
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?
        } else {
            fs::copy(entry.path(), target).map_err(io_import)?;
        }
    }
    Ok(())
}
fn move_directory_cross_device(source: &Path, destination: &Path) -> Result<(), AppError> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(17) => {
            copy_tree(source, destination)?;
            fs::remove_dir_all(source).map_err(io_import)?;
            Ok(())
        }
        Err(error) => Err(io_import(error)),
    }
}
fn temporary_root(prefix: &str) -> Result<PathBuf, AppError> {
    let path = std::env::temp_dir().join(format!("qingyu-{prefix}-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).map_err(io_import)?;
    Ok(path)
}
fn safe_relative(value: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        Err(AppError::new(
            "transfer_unsafe_path",
            "数据包包含不安全路径",
        ))
    } else {
        Ok(path)
    }
}
fn safe_name(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    let value = cleaned.trim().trim_matches('.');
    let value = if value.is_empty() { "未命名页面" } else { value };
    let value: String = value.chars().take(80).collect();
    let stem = value.split('.').next().unwrap_or("");
    if matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    ) {
        format!("_{value}")
    } else {
        value
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn io_import(error: std::io::Error) -> AppError {
    AppError::new("transfer_io_failed", error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::fs;

    use serde_json::json;
    use uuid::Uuid;
    use zip::ZipArchive;

    use super::{
        collect_hashes, copy_tree, export_lossless, export_markdown, import_content,
        preview_content, recover_interrupted_import, TransferManifest,
    };
    use crate::database::repository::DatabaseRepository;
    use crate::pages::model::{BlockNode, PageDocument, PageKind};
    use crate::pages::repository::PageRepository;
    use rusqlite::Connection;

    fn temporary_repository() -> (PageRepository, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("qingyu-transfer-test-{}", Uuid::new_v4()));
        let repository = PageRepository::open(root.join("vault")).unwrap();
        (repository, root)
    }

    #[test]
    fn markdown_export_preserves_rich_blocks_and_internal_links() {
        let (repository, root) = temporary_repository();
        let target = repository
            .create("目标页面".into(), None, String::new())
            .unwrap();
        let source = repository
            .create("来源页面".into(), None, String::new())
            .unwrap();
        let heading = BlockNode {
            id: "heading-1".into(),
            block_type: "heading".into(),
            text: "章节".into(),
            attrs: BTreeMap::from([(
                "node".into(),
                json!({
                    "type": "heading", "attrs": { "level": 2 },
                    "content": [{ "type": "text", "text": "章节" }]
                }),
            )]),
            children: Vec::new(),
        };
        let link = BlockNode {
            id: "link-1".into(),
            block_type: "paragraph".into(),
            text: "打开目标页面".into(),
            attrs: BTreeMap::from([(
                "node".into(),
                json!({
                    "type": "paragraph",
                    "content": [
                        { "type": "text", "text": "打开" },
                        { "type": "text", "text": "目标页面", "marks": [
                            { "type": "bold" },
                            { "type": "link", "attrs": { "href": format!("qingyu://page/{}", target.id) } }
                        ] }
                    ]
                }),
            )]),
            children: Vec::new(),
        };
        repository
            .save_document(
                PageDocument {
                    page_id: source.id,
                    title: "来源页面".into(),
                    blocks: vec![heading, link],
                    revision: 0,
                },
                0,
            )
            .unwrap();

        let destination = root.join("export.zip");
        export_markdown(&repository, &destination).unwrap();
        let mut archive = ZipArchive::new(fs::File::open(destination).unwrap()).unwrap();
        let mut markdown = String::new();
        use std::io::Read;
        archive
            .by_name("来源页面.md")
            .unwrap()
            .read_to_string(&mut markdown)
            .unwrap();
        assert!(markdown.contains("## 章节"));
        assert!(markdown.contains("[**目标页面**](目标页面.md)"));
        assert!(!markdown.contains("qingyu://"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn markdown_export_includes_nested_block_children() {
        let (repository, root) = temporary_repository();
        let page = repository.create("嵌套导出".into(), None, String::new()).unwrap();
        repository
            .save_document(
                PageDocument {
                    page_id: page.id,
                    title: page.title,
                    blocks: vec![BlockNode {
                        id: "parent".into(),
                        block_type: "paragraph".into(),
                        text: "父区块".into(),
                        attrs: BTreeMap::new(),
                        children: vec![BlockNode {
                            id: "child".into(),
                            block_type: "paragraph".into(),
                            text: "子区块正文".into(),
                            attrs: BTreeMap::new(),
                            children: Vec::new(),
                        }],
                    }],
                    revision: 0,
                },
                0,
            )
            .unwrap();

        let destination = root.join("nested.zip");
        export_markdown(&repository, &destination).unwrap();
        let mut archive = ZipArchive::new(fs::File::open(destination).unwrap()).unwrap();
        let mut markdown = String::new();
        use std::io::Read;
        archive
            .by_name("嵌套导出.md")
            .unwrap()
            .read_to_string(&mut markdown)
            .unwrap();

        assert!(markdown.contains("父区块"));
        assert!(markdown.contains("子区块正文"));
        assert!(markdown.contains("  <!-- qingyu-block:child -->\n  子区块正文"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn notion_csv_import_creates_a_database_with_fields_and_records() {
        let (repository, root) = temporary_repository();
        let import_root = root.join("notion");
        fs::create_dir_all(&import_root).unwrap();
        fs::write(
            import_root.join("任务.csv"),
            "Name,状态,优先级\n修复搜索,进行中,3\n完成导出,已完成,2\n",
        )
        .unwrap();

        let report = import_content(&repository, &import_root).unwrap();
        assert_eq!(report.pages, 3);
        let pages = repository.list(false).unwrap();
        let database = pages
            .iter()
            .find(|page| page.kind == PageKind::Database)
            .unwrap();
        let databases = DatabaseRepository::from_pages(repository.clone()).unwrap();
        let definition = databases.get(&database.id).unwrap();
        assert_eq!(
            definition
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            vec!["状态", "优先级"]
        );
        let records = databases.query(&database.id, Default::default()).unwrap();
        assert_eq!(
            records
                .iter()
                .map(|record| record.title.as_str())
                .collect::<Vec<_>>(),
            vec!["修复搜索", "完成导出"]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn database_markdown_and_csv_companions_import_as_one_database_page() {
        let (repository, root) = temporary_repository();
        let import_root = root.join("notion-database");
        fs::create_dir_all(&import_root).unwrap();
        fs::write(import_root.join("任务.md"), "<!-- qingyu-database -->\n\n# 任务\n\n数据库说明\n").unwrap();
        fs::write(
            import_root.join("任务.csv"),
            "Name,状态\n修复搜索,进行中\n完成导出,已完成\n",
        )
        .unwrap();

        let preview = preview_content(&import_root).unwrap();
        let report = import_content(&repository, &import_root).unwrap();
        let pages = repository.list(false).unwrap();
        let databases = pages
            .iter()
            .filter(|page| page.kind == PageKind::Database)
            .collect::<Vec<_>>();

        assert_eq!(preview.pages, 3);
        assert_eq!(preview.databases, 1);
        assert_eq!(report.pages, 3);
        assert_eq!(pages.len(), 3);
        assert_eq!(databases.len(), 1);
        assert!(!pages.iter().any(|page| page.kind == PageKind::Page && page.title == "任务"));
        assert!(repository.get(&databases[0].id, false).unwrap().content.contains("数据库说明"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ordinary_markdown_and_csv_with_the_same_name_remain_separate() {
        let (repository, root) = temporary_repository();
        let import_root = root.join("ordinary-pair");
        fs::create_dir_all(&import_root).unwrap();
        fs::write(import_root.join("报告.md"), "# 报告\n\n正文\n").unwrap();
        fs::write(import_root.join("报告.csv"), "Name,状态\n记录,完成\n").unwrap();

        import_content(&repository, &import_root).unwrap();
        let pages = repository.list(false).unwrap();
        assert!(pages.iter().any(|page| page.kind == PageKind::Page && page.title == "报告"));
        assert!(pages.iter().any(|page| page.kind == PageKind::Database && page.title == "报告"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_content_import_rolls_back_every_created_page() {
        let (repository, root) = temporary_repository();
        let existing = repository
            .create("原有页面".into(), None, "保留".into())
            .unwrap();
        let import_root = root.join("broken-import");
        fs::create_dir_all(&import_root).unwrap();
        fs::write(import_root.join("a.md"), "# 会被回滚").unwrap();
        fs::write(import_root.join("z.md"), [0xff, 0xfe, 0xfd]).unwrap();

        assert!(import_content(&repository, &import_root).is_err());
        let pages = repository.list(false).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].id, existing.id);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_commit_is_recovered_on_the_next_open() {
        let (repository, root) = temporary_repository();
        repository.create("导入前页面".into(), None, String::new()).unwrap();
        let data_root = repository.root().parent().unwrap().to_path_buf();
        let attachments = data_root.join("attachments");
        fs::write(attachments.join("original.txt"), "原附件").unwrap();
        let commit_root = data_root.join(".transfer-commit");
        fs::create_dir_all(&commit_root).unwrap();
        repository.snapshot_database(&commit_root.join("original.sqlite3")).unwrap();
        copy_tree(&attachments, &commit_root.join("original-attachments")).unwrap();

        repository.create("未完成导入".into(), None, String::new()).unwrap();
        fs::write(attachments.join("original.txt"), "新附件").unwrap();
        fs::write(
            commit_root.join("journal.json"),
            r#"{"preparedName":".attachments-import-test","rollbackName":".attachments-rollback-test","attachmentsExisted":true}"#,
        ).unwrap();
        drop(repository);

        recover_interrupted_import(&data_root.join("vault")).unwrap();
        let recovered = PageRepository::open(data_root.join("vault")).unwrap();
        let pages = recovered.list(false).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].title, "导入前页面");
        assert_eq!(fs::read_to_string(attachments.join("original.txt")).unwrap(), "原附件");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn markdown_import_never_reads_files_outside_the_import_root() {
        let (repository, root) = temporary_repository();
        let import_root = root.join("incoming");
        fs::create_dir_all(import_root.join("nested")).unwrap();
        let secret = root.join("secret.pdf");
        fs::write(&secret, b"private local file").unwrap();
        fs::write(
            import_root.join("nested").join("page.md"),
            "[不应导入](../../secret.pdf)",
        )
        .unwrap();

        let report = import_content(&repository, &import_root).unwrap();

        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("导入目录之外")));
        let connection = Connection::open(repository.database_path()).unwrap();
        let attachments: i64 = connection
            .query_row("SELECT COUNT(*) FROM attachments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(attachments, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn import_preview_reports_content_without_writing_the_library() {
        let (repository, root) = temporary_repository();
        let import_root = root.join("preview");
        fs::create_dir_all(&import_root).unwrap();
        fs::write(import_root.join("说明.md"), "# 说明").unwrap();
        fs::write(
            import_root.join("任务.csv"),
            "Name,状态\n事项一,待办\n事项二,完成\n",
        )
        .unwrap();

        let preview = preview_content(&import_root).unwrap();
        assert_eq!(preview.pages, 4);
        assert_eq!(preview.databases, 1);
        assert_eq!(preview.files, 2);
        assert!(repository.list(false).unwrap().is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn markdown_round_trip_rewrites_page_links_and_imports_assets() {
        let (source, source_root) = temporary_repository();
        let target_page = source
            .create("目标页面".into(), None, "目标内容".into())
            .unwrap();
        source
            .save_document(
                PageDocument {
                    page_id: target_page.id.clone(),
                    title: target_page.title.clone(),
                    blocks: vec![BlockNode {
                        id: "target-section".into(),
                        block_type: "heading".into(),
                        text: "目标章节".into(),
                        attrs: BTreeMap::new(),
                        children: Vec::new(),
                    }],
                    revision: 0,
                },
                0,
            )
            .unwrap();
        let source_page = source
            .create("来源页面".into(), None, String::new())
            .unwrap();
        let image_path = source
            .store_asset(
                &source_page.id,
                "截图.png",
                &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 0],
            )
            .unwrap();
        let pdf_bytes = b"%PDF-1.4\nqingyu attachment\n";
        let pdf_path = source
            .store_asset(&source_page.id, "说明书.pdf", pdf_bytes)
            .unwrap();
        source
            .save_document(
                PageDocument {
                    page_id: source_page.id.clone(),
                    title: source_page.title.clone(),
                    blocks: vec![BlockNode {
                        id: "content".into(),
                        block_type: "paragraph".into(),
                        text: "目标页面截图".into(),
                        attrs: BTreeMap::from([(
                            "node".into(),
                            json!({
                                "type": "doc",
                                "content": [
                                    { "type": "paragraph", "content": [
                                        { "type": "text", "text": "目标页面", "marks": [
                                            { "type": "link", "attrs": { "href": format!("qingyu://page/{}#target-section", target_page.id) } }
                                        ] }
                                    ] },
                                    { "type": "image", "attrs": { "src": image_path, "alt": "截图" } }
                                    ,{ "type": "paragraph", "content": [
                                        { "type": "text", "text": "说明书", "marks": [
                                            { "type": "link", "attrs": { "href": pdf_path } }
                                        ] }
                                    ] }
                                ]
                            }),
                        )]),
                        children: Vec::new(),
                    }],
                    revision: 0,
                },
                0,
            )
            .unwrap();

        let package = source_root.join("round-trip.zip");
        export_markdown(&source, &package).unwrap();
        let (destination, destination_root) = temporary_repository();
        import_content(&destination, &package).unwrap();

        let pages = destination.list(false).unwrap();
        let imported_source = pages.iter().find(|page| page.title == "来源页面").unwrap();
        let imported_target = pages.iter().find(|page| page.title == "目标页面").unwrap();
        let content = destination
            .open_bundle(&imported_source.id)
            .unwrap()
            .document
            .blocks[0]
            .text
            .clone();
        assert!(content.contains(&format!("qingyu://page/{}#target-section", imported_target.id)));
        assert!(!content.contains(&target_page.id));
        let target_content = destination.open_bundle(&imported_target.id).unwrap().document.blocks[0].text.clone();
        assert!(target_content.contains("<!-- qingyu-block:target-section -->"));
        let asset_marker = format!("/.noteapp/assets/{}/", imported_source.id);
        let asset_start = content.find(&asset_marker).unwrap();
        let asset_end = content[asset_start..].find(')').unwrap() + asset_start;
        let imported_asset = &content[asset_start..asset_end];
        assert!(destination.read_asset(imported_asset).is_ok());
        let pdf_start = content.find("/.noteapp/assets/").and_then(|first| {
            content[first + 1..]
                .find("/.noteapp/assets/")
                .map(|next| first + 1 + next)
        }).unwrap();
        let pdf_end = content[pdf_start..].find(')').unwrap() + pdf_start;
        let imported_pdf = &content[pdf_start..pdf_end];
        assert!(imported_pdf.ends_with(".pdf"));
        assert_eq!(destination.read_asset(imported_pdf).unwrap().1, pdf_bytes);

        let _ = fs::remove_dir_all(source_root);
        let _ = fs::remove_dir_all(destination_root);
    }

    #[test]
    fn lossless_preview_rejects_a_database_with_foreign_key_errors() {
        let (repository, root) = temporary_repository();
        repository
            .create("原页面".into(), None, String::new())
            .unwrap();
        let package = root.join("invalid-lossless");
        fs::create_dir_all(&package).unwrap();
        let database = package.join("qingyu.sqlite3");
        repository.snapshot_database(&database).unwrap();
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys=OFF;")
            .unwrap();
        connection.execute(
            "INSERT INTO attachments(id,page_id,relative_path,mime,size,created_at) VALUES('broken','missing','attachments/missing/file.png','image/png',1,'0')",
            [],
        ).unwrap();
        drop(connection);
        let mut files = HashMap::new();
        collect_hashes(&package, &package, &mut files).unwrap();
        fs::write(
            package.join("manifest.json"),
            serde_json::to_vec_pretty(&TransferManifest {
                version: 1,
                format: "qingyu-lossless".into(),
                files,
            })
            .unwrap(),
        )
        .unwrap();

        let error = preview_content(&package).unwrap_err();
        assert_eq!(error.code, "transfer_invalid_database");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lossless_package_round_trip_preserves_all_authoritative_data() {
        let (source, source_root) = temporary_repository();
        let target = source.create("目标".into(), None, "目标正文".into()).unwrap();
        let page = source
            .create(
                "来源".into(),
                None,
                format!("[目标](qingyu://page/{})", target.id),
            )
            .unwrap();
        let asset_bytes = b"%PDF-1.4\nlossless\n";
        let asset_path = source.store_asset(&page.id, "资料.pdf", asset_bytes).unwrap();
        source
            .update(
                &page.id,
                page.title.clone(),
                format!("[目标](qingyu://page/{})\n\n[资料]({asset_path})", target.id),
            )
            .unwrap();
        source
            .with_transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO conversations(id,thread_id,title,scope_json,created_at,updated_at,active_turn_id) VALUES('conversation-1','thread-1','历史对话',?1,'10','20',NULL)",
                    [format!(r#"{{"kind":"page","pageId":"{}"}}"#, page.id)],
                ).map_err(|error| crate::error::AppError::new("test_sql", error.to_string()))?;
                transaction.execute(
                    "INSERT INTO conversation_events(id,conversation_id,sequence,event_type,payload_json,created_at) VALUES('event-1','conversation-1',0,'userMessage','{\"text\":\"保留我\"}','10')",
                    [],
                ).map_err(|error| crate::error::AppError::new("test_sql", error.to_string()))?;
                Ok(())
            })
            .unwrap();
        let expected_revision = source.open_bundle(&page.id).unwrap().document.revision;
        let source_connection = Connection::open(source.database_path()).unwrap();
        let expected_revisions: i64 = source_connection
            .query_row("SELECT COUNT(*) FROM page_revisions", [], |row| row.get(0))
            .unwrap();
        drop(source_connection);

        let package = source_root.join("lossless.zip");
        export_lossless(&source, &package).unwrap();
        let (destination, destination_root) = temporary_repository();
        destination.create("将被替换".into(), None, String::new()).unwrap();
        import_content(&destination, &package).unwrap();

        let pages = destination.list(false).unwrap();
        assert_eq!(pages.len(), 2);
        assert!(pages.iter().any(|candidate| candidate.id == page.id));
        assert!(pages.iter().any(|candidate| candidate.id == target.id));
        assert_eq!(destination.links(&page.id).unwrap()[0].target_id, Some(target.id));
        assert_eq!(destination.read_asset(&asset_path).unwrap().1, asset_bytes);
        assert_eq!(destination.open_bundle(&page.id).unwrap().document.revision, expected_revision);
        let connection = Connection::open(destination.database_path()).unwrap();
        let revisions: i64 = connection
            .query_row("SELECT COUNT(*) FROM page_revisions", [], |row| row.get(0))
            .unwrap();
        let conversations: i64 = connection
            .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
            .unwrap();
        let events: i64 = connection
            .query_row("SELECT COUNT(*) FROM conversation_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(revisions, expected_revisions);
        assert_eq!(conversations, 1);
        assert_eq!(events, 1);

        let _ = fs::remove_dir_all(source_root);
        let _ = fs::remove_dir_all(destination_root);
    }

    #[test]
    fn attachment_free_lossless_package_clears_target_attachments() {
        let (source, source_root) = temporary_repository();
        source.create("干净库".into(), None, String::new()).unwrap();
        let package = source_root.join("clean.zip");
        export_lossless(&source, &package).unwrap();

        let (destination, destination_root) = temporary_repository();
        let old_page = destination.create("旧页面".into(), None, String::new()).unwrap();
        destination.store_asset(&old_page.id, "旧附件.pdf", b"%PDF-1.4\nold").unwrap();
        import_content(&destination, &package).unwrap();

        let attachments = destination_root.join("attachments");
        assert!(attachments.exists());
        assert!(fs::read_dir(attachments).unwrap().next().is_none());
        let _ = fs::remove_dir_all(source_root);
        let _ = fs::remove_dir_all(destination_root);
    }
}
