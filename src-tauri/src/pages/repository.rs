use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use uuid::Uuid;

use super::model::{Page, PageHistoryEntry, PageKind, PageLink, PageSummary, SearchResult};
use crate::error::AppError;

#[derive(Clone)]
pub struct PageRepository {
    root: PathBuf,
}

impl PageRepository {
    pub fn default_root() -> PathBuf {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Documents")
            .join("轻羽笔记库")
    }

    pub fn open(root: PathBuf) -> Result<Self, AppError> {
        fs::create_dir_all(root.join(".noteapp").join("history"))
            .map_err(|e| AppError::new("vault_init_failed", e.to_string()))?;
        fs::create_dir_all(root.join(".noteapp").join("trash"))
            .map_err(|e| AppError::new("vault_init_failed", e.to_string()))?;
        fs::write(
            root.join(".noteapp").join("vault.json"),
            b"{\"version\":1}\n",
        )
        .map_err(|e| AppError::new("vault_init_failed", e.to_string()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list(&self, include_trash: bool) -> Result<Vec<PageSummary>, AppError> {
        let mut out = Vec::new();
        self.walk(&self.root, &mut out, include_trash)?;
        if include_trash {
            let trash = self.root.join(".noteapp").join("trash");
            if trash.exists() {
                for entry in fs::read_dir(trash)
                    .map_err(|e| AppError::new("vault_read_failed", e.to_string()))?
                {
                    let path = entry
                        .map_err(|e| AppError::new("vault_read_failed", e.to_string()))?
                        .path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        let page = self.read_file(&path)?;
                        out.push(self.summary(&page, &path));
                    }
                }
            }
        }
        out.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.title.cmp(&b.title)));
        Ok(out)
    }

    fn walk(
        &self,
        dir: &Path,
        out: &mut Vec<PageSummary>,
        include_trash: bool,
    ) -> Result<(), AppError> {
        for entry in
            fs::read_dir(dir).map_err(|e| AppError::new("vault_read_failed", e.to_string()))?
        {
            let entry = entry.map_err(|e| AppError::new("vault_read_failed", e.to_string()))?;
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()) == Some(".noteapp") {
                continue;
            }
            if path.is_dir() {
                self.walk(&path, out, include_trash)?;
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let page = self.read_file(&path)?;
            if include_trash || page.trashed_at.is_none() {
                out.push(self.summary(&page, &path));
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str, include_trash: bool) -> Result<Page, AppError> {
        let path = self.find_path(id)?;
        let page = self.read_file(&path)?;
        if !include_trash && page.trashed_at.is_some() {
            return Err(AppError::new("page_not_found", "页面不在当前视图"));
        }
        Ok(page)
    }

    pub fn create(
        &self,
        title: String,
        parent_id: Option<String>,
        content: String,
    ) -> Result<Page, AppError> {
        self.create_typed(
            title,
            parent_id,
            content,
            PageKind::Page,
            None,
            BTreeMap::new(),
        )
    }

    pub fn create_typed(
        &self,
        title: String,
        parent_id: Option<String>,
        content: String,
        kind: PageKind,
        database_id: Option<String>,
        properties: BTreeMap<String, Value>,
    ) -> Result<Page, AppError> {
        if let Some(parent) = parent_id.as_deref() {
            let _ = self.get(parent, false)?;
        }
        let now = now();
        let page = Page {
            id: Uuid::new_v4().to_string(),
            title,
            parent_id,
            kind,
            content,
            created_at: now.clone(),
            updated_at: now,
            trashed_at: None,
            sort_order: 0,
            properties,
            database_id,
        };
        let path = self.path_for(&page)?;
        self.write_atomic(&path, &page)?;
        self.record(&page, "create")?;
        Ok(page)
    }

    pub fn update_properties(
        &self,
        id: &str,
        title: String,
        properties: BTreeMap<String, Value>,
    ) -> Result<Page, AppError> {
        let mut page = self.get(id, false)?;
        self.record(&page, "before_update_properties")?;
        let old_path = self.find_path(id)?;
        page.title = title;
        page.properties = properties;
        page.updated_at = now();
        let new_path = self.path_for(&page)?;
        self.write_atomic(&new_path, &page)?;
        if old_path != new_path {
            let _ = fs::remove_file(&old_path);
        }
        self.record(&page, "update_properties")?;
        Ok(page)
    }

    pub fn update(&self, id: &str, title: String, content: String) -> Result<Page, AppError> {
        let mut page = self.get(id, false)?;
        let old_title = page.title.clone();
        self.record(&page, "before_update")?;
        let old_path = self.find_path(id)?;
        page.title = title;
        page.content = content;
        page.updated_at = now();
        let new_path = self.path_for(&page)?;
        self.write_atomic(&new_path, &page)?;
        if old_path != new_path {
            let _ = fs::remove_file(&old_path);
            self.rename_child_directory(&old_path, &new_path, &old_title, &page.title)?;
        }
        if old_title != page.title {
            self.rewrite_links(id, &old_title, &page.title)?;
        }
        self.record(&page, "update")?;
        Ok(page)
    }

    pub fn store_asset(
        &self,
        page_id: &str,
        original_name: &str,
        bytes: &[u8],
    ) -> Result<String, AppError> {
        if bytes.len() > 20 * 1024 * 1024 {
            return Err(AppError::new("asset_too_large", "图片不能超过 20 MiB"));
        }
        let name_path = Path::new(original_name);
        if original_name.is_empty()
            || name_path.components().count() != 1
            || name_path.file_name().and_then(|name| name.to_str()) != Some(original_name)
        {
            return Err(AppError::new("asset_invalid_name", "图片文件名不安全"));
        }
        let extension = image_extension(bytes)
            .ok_or_else(|| AppError::new("asset_invalid_type", "仅支持 PNG、JPEG、GIF 和 WebP"))?;
        let _ = self.get(page_id, false)?;
        let directory = self.root.join(".noteapp").join("assets").join(page_id);
        fs::create_dir_all(&directory)
            .map_err(|error| AppError::new("asset_write_failed", error.to_string()))?;
        let file_name = format!("{}.{}", Uuid::new_v4(), extension);
        let destination = directory.join(&file_name);
        let temporary = directory.join(format!("{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, bytes)
            .map_err(|error| AppError::new("asset_write_failed", error.to_string()))?;
        fs::rename(&temporary, &destination).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            AppError::new("asset_write_failed", error.to_string())
        })?;
        Ok(format!("/.noteapp/assets/{page_id}/{file_name}"))
    }

    pub fn move_page(
        &self,
        id: &str,
        parent_id: Option<String>,
        sort_order: i64,
    ) -> Result<Page, AppError> {
        let mut page = self.get(id, false)?;
        if parent_id.as_deref() == Some(id) || self.is_descendant(parent_id.as_deref(), id)? {
            return Err(AppError::new(
                "page_invalid_parent",
                "不能将页面移动到自身或子页面下",
            ));
        }
        if let Some(parent) = parent_id.as_deref() {
            let _ = self.get(parent, false)?;
        }
        let old_path = self.find_path(id)?;
        self.record(&page, "before_move")?;
        page.parent_id = parent_id;
        page.sort_order = sort_order;
        page.updated_at = now();
        let new_path = self.path_for(&page)?;
        self.write_atomic(&new_path, &page)?;
        if old_path != new_path {
            let _ = fs::remove_file(&old_path);
            self.rename_child_directory(&old_path, &new_path, &page.title, &page.title)?;
        }
        self.record(&page, "move")?;
        Ok(page)
    }

    pub fn trash(&self, id: &str) -> Result<Page, AppError> {
        let mut page = self.get(id, false)?;
        self.record(&page, "before_trash")?;
        let old = self.find_path(id)?;
        page.trashed_at = Some(now());
        page.updated_at = now();
        let new = self
            .root
            .join(".noteapp")
            .join("trash")
            .join(format!("{}.md", page.id));
        self.write_atomic(&new, &page)?;
        let _ = fs::remove_file(old);
        self.record(&page, "trash")?;
        Ok(page)
    }

    pub fn restore(&self, id: &str) -> Result<Page, AppError> {
        let path = self
            .root
            .join(".noteapp")
            .join("trash")
            .join(format!("{}.md", id));
        let mut page = self.read_file(&path)?;
        page.trashed_at = None;
        page.updated_at = now();
        let new = self.path_for(&page)?;
        self.write_atomic(&new, &page)?;
        let _ = fs::remove_file(path);
        self.record(&page, "restore")?;
        Ok(page)
    }

    pub fn history(&self, id: &str) -> Result<Vec<PageHistoryEntry>, AppError> {
        let path = self
            .root
            .join(".noteapp")
            .join("history")
            .join(format!("{}.jsonl", id));
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(path)
            .map_err(|e| AppError::new("history_read_failed", e.to_string()))?;
        Ok(text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect())
    }

    pub fn links(&self, id: &str) -> Result<Vec<PageLink>, AppError> {
        let source = self.get(id, false)?;
        let pages = self.list(false)?;
        let by_title: HashMap<_, _> = pages
            .iter()
            .map(|p| (p.title.to_lowercase(), p.id.clone()))
            .collect();
        Ok(extract_link_tokens(&source.content)
            .into_iter()
            .map(|(label, target)| PageLink {
                source_id: id.to_owned(),
                target_id: target.or_else(|| by_title.get(&label.to_lowercase()).cloned()),
                label,
            })
            .collect())
    }

    pub fn backlinks(&self, id: &str) -> Result<Vec<PageLink>, AppError> {
        let pages = self.list(false)?;
        let by_title: HashMap<_, _> = pages
            .iter()
            .map(|p| (p.title.to_lowercase(), p.id.clone()))
            .collect();
        let mut result = Vec::new();
        for page in pages {
            if page.id == id {
                continue;
            }
            let source = self.get(&page.id, false)?;
            for (label, target) in extract_link_tokens(&source.content) {
                if target.as_deref() == Some(id)
                    || (target.is_none()
                        && by_title
                            .get(&label.to_lowercase())
                            .map(|target| target == id)
                            .unwrap_or(false))
                {
                    result.push(PageLink {
                        source_id: page.id.clone(),
                        target_id: Some(id.to_owned()),
                        label,
                    });
                }
            }
        }
        Ok(result)
    }

    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, AppError> {
        let needle = query.to_lowercase();
        if needle.trim().is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .list(false)?
            .into_iter()
            .filter_map(|summary| {
                let page = self.get(&summary.id, false).ok()?;
                let hay = format!("{}\n{}", page.title, page.content);
                let pos = hay.to_lowercase().find(&needle)?;
                let start = pos.saturating_sub(48);
                let end = (pos + needle.len() + 80).min(hay.len());
                Some(SearchResult {
                    page: summary,
                    snippet: hay[start..end].replace('\n', " "),
                })
            })
            .collect())
    }

    fn summary(&self, page: &Page, path: &Path) -> PageSummary {
        PageSummary {
            id: page.id.clone(),
            title: page.title.clone(),
            parent_id: page.parent_id.clone(),
            kind: page.kind.clone(),
            path: path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/"),
            sort_order: page.sort_order,
            trashed_at: page.trashed_at.clone(),
        }
    }
    fn path_for(&self, page: &Page) -> Result<PathBuf, AppError> {
        let mut dir = self.root.clone();
        if let Some(parent) = page.parent_id.as_deref() {
            let parent_path = self.find_path(parent)?;
            dir = parent_path
                .parent()
                .unwrap_or(&self.root)
                .join(safe_name(&self.read_file(&parent_path)?.title));
            fs::create_dir_all(&dir)
                .map_err(|e| AppError::new("vault_write_failed", e.to_string()))?;
        }
        Ok(dir.join(format!("{}.md", safe_name(&page.title))))
    }
    fn find_path(&self, id: &str) -> Result<PathBuf, AppError> {
        if self
            .root
            .join(".noteapp")
            .join("trash")
            .join(format!("{}.md", id))
            .exists()
        {
            return Ok(self
                .root
                .join(".noteapp")
                .join("trash")
                .join(format!("{}.md", id)));
        }
        let mut all = Vec::new();
        self.walk(&self.root, &mut all, true)?;
        all.into_iter()
            .find(|p| p.id == id)
            .map(|p| self.root.join(&p.path))
            .ok_or_else(|| AppError::new("page_not_found", "页面不存在"))
    }
    fn read_file(&self, path: &Path) -> Result<Page, AppError> {
        let text = fs::read_to_string(path)
            .map_err(|e| AppError::new("page_read_failed", e.to_string()))?;
        parse_markdown(&text)
    }
    fn write_atomic(&self, path: &Path, page: &Page) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::new("page_write_failed", e.to_string()))?;
        }
        let tmp = path.with_extension("md.tmp");
        fs::write(&tmp, serialize_markdown(page))
            .map_err(|e| AppError::new("page_write_failed", e.to_string()))?;
        fs::rename(tmp, path).map_err(|e| AppError::new("page_write_failed", e.to_string()))
    }
    fn record(&self, page: &Page, action: &str) -> Result<(), AppError> {
        let entry = PageHistoryEntry {
            id: Uuid::new_v4().to_string(),
            page_id: page.id.clone(),
            action: action.to_owned(),
            timestamp: now(),
            title: page.title.clone(),
            content: page.content.clone(),
        };
        let path = self
            .root
            .join(".noteapp")
            .join("history")
            .join(format!("{}.jsonl", page.id));
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| AppError::new("history_write_failed", e.to_string()))?;
        writeln!(file, "{}", serde_json::to_string(&entry).unwrap())
            .map_err(|e| AppError::new("history_write_failed", e.to_string()))
    }
    fn is_descendant(&self, candidate: Option<&str>, ancestor: &str) -> Result<bool, AppError> {
        let mut current = candidate.map(str::to_owned);
        while let Some(id) = current {
            if id == ancestor {
                return Ok(true);
            }
            current = self.get(&id, false)?.parent_id;
        }
        Ok(false)
    }

    fn rewrite_links(
        &self,
        target_id: &str,
        old_title: &str,
        new_title: &str,
    ) -> Result<(), AppError> {
        let pages = self.list(false)?;
        let old_plain = format!("[[{}]]", old_title);
        let old_stable = format!("[[{}|{}]]", old_title, target_id);
        let replacement = format!("[[{}|{}]]", new_title, target_id);
        for summary in pages {
            if summary.id == target_id {
                continue;
            }
            let mut page = self.get(&summary.id, false)?;
            let updated = page
                .content
                .replace(&old_stable, &replacement)
                .replace(&old_plain, &replacement);
            if updated != page.content {
                page.content = updated;
                page.updated_at = now();
                let path = self.find_path(&page.id)?;
                self.write_atomic(&path, &page)?;
            }
        }
        Ok(())
    }

    fn rename_child_directory(
        &self,
        old_path: &Path,
        new_path: &Path,
        old_title: &str,
        new_title: &str,
    ) -> Result<(), AppError> {
        let old_dir = old_path
            .parent()
            .unwrap_or(&self.root)
            .join(safe_name(old_title));
        let new_dir = new_path
            .parent()
            .unwrap_or(&self.root)
            .join(safe_name(new_title));
        if old_dir.exists() && old_dir != new_dir && !new_dir.exists() {
            fs::rename(old_dir, new_dir)
                .map_err(|e| AppError::new("page_move_failed", e.to_string()))?;
        }
        Ok(())
    }
}

fn now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
fn safe_name(title: &str) -> String {
    let value: String = title
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = value.trim().trim_matches('.');
    if trimmed.is_empty() {
        "未命名页面".to_owned()
    } else {
        trimmed.chars().take(80).collect()
    }
}
fn extract_link_tokens(content: &str) -> Vec<(String, Option<String>)> {
    let mut found = Vec::new();
    let mut rest = content;
    let mut offset = 0;
    while let Some(start) = rest.find("[[") {
        let tail = &rest[start + 2..];
        if let Some(end) = tail.find("]]") {
            let raw = tail[..end].trim();
            let (label, target) = raw
                .split_once('|')
                .map(|(label, target)| (label.trim(), Some(target.trim().to_owned())))
                .unwrap_or((raw, None));
            if !label.is_empty() {
                found.push((offset + start, label.to_owned(), target));
            }
            offset += start + 2 + end + 2;
            rest = &tail[end + 2..];
        } else {
            break;
        }
    }

    let marker = "](qingyu://page/";
    let mut search_from = 0;
    while let Some(relative) = content[search_from..].find(marker) {
        let marker_start = search_from + relative;
        let Some(label_start) = content[..marker_start].rfind('[') else {
            search_from = marker_start + marker.len();
            continue;
        };
        if label_start > 0 && content.as_bytes()[label_start - 1] == b'!' {
            search_from = marker_start + marker.len();
            continue;
        }
        let target_start = marker_start + marker.len();
        let Some(target_end) = content[target_start..].find(')') else {
            break;
        };
        let label = content[label_start + 1..marker_start].trim();
        let target = content[target_start..target_start + target_end]
            .split('#')
            .next()
            .unwrap_or("")
            .trim();
        if !label.is_empty() && !target.is_empty() {
            found.push((label_start, label.to_owned(), Some(target.to_owned())));
        }
        search_from = target_start + target_end + 1;
    }
    found.sort_by_key(|item| item.0);
    found
        .into_iter()
        .map(|(_, label, target)| (label, target))
        .collect()
}
fn serialize_markdown(page: &Page) -> String {
    let kind = match page.kind {
        PageKind::Page => "page",
        PageKind::Database => "database",
        PageKind::DatabaseRecord => "databaseRecord",
    };
    let properties = serde_json::to_string(&page.properties).unwrap_or_else(|_| "{}".to_owned());
    format!("---\nid: {}\ntitle: {}\nparent_id: {}\nkind: {}\ncreated_at: {}\nupdated_at: {}\ntrashed_at: {}\nsort_order: {}\ndatabase_id: {}\nproperties: {}\n---\n{}", page.id, page.title.replace('\n', " "), page.parent_id.as_deref().unwrap_or(""), kind, page.created_at, page.updated_at, page.trashed_at.as_deref().unwrap_or(""), page.sort_order, page.database_id.as_deref().unwrap_or(""), properties, page.content)
}

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}
fn parse_markdown(text: &str) -> Result<Page, AppError> {
    let (front, content) = text
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---\n"))
        .ok_or_else(|| AppError::new("page_invalid_metadata", "Markdown 页面缺少元数据"))?;
    let mut fields = HashMap::new();
    for line in front.lines() {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim(), value.trim());
        }
    }
    let required = |key: &str| {
        fields
            .get(key)
            .copied()
            .ok_or_else(|| AppError::new("page_invalid_metadata", format!("缺少字段 {}", key)))
    };
    Ok(Page {
        id: required("id")?.to_owned(),
        title: required("title")?.to_owned(),
        parent_id: fields
            .get("parent_id")
            .filter(|v| !v.is_empty())
            .map(|v| (*v).to_owned()),
        kind: match fields.get("kind").copied().unwrap_or("page") {
            "database" => PageKind::Database,
            "databaseRecord" => PageKind::DatabaseRecord,
            _ => PageKind::Page,
        },
        content: content.to_owned(),
        created_at: required("created_at")?.to_owned(),
        updated_at: required("updated_at")?.to_owned(),
        trashed_at: fields
            .get("trashed_at")
            .filter(|v| !v.is_empty())
            .map(|v| (*v).to_owned()),
        sort_order: fields
            .get("sort_order")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        properties: fields
            .get("properties")
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default(),
        database_id: fields
            .get("database_id")
            .filter(|value| !value.is_empty())
            .map(|value| (*value).to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_names_are_sanitized() {
        assert_eq!(safe_name("a:b?.md"), "a_b_.md");
    }
    #[test]
    fn links_are_extracted() {
        assert_eq!(
            extract_link_tokens("[[A]] and [[ B ]] and [新标题](qingyu://page/page-uuid)."),
            vec![
                ("A".into(), None),
                ("B".into(), None),
                ("新标题".into(), Some("page-uuid".into()))
            ]
        );
    }

    #[test]
    fn stable_page_links_survive_target_rename_and_move() {
        let root = std::env::temp_dir().join(format!("qingyu-link-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let folder = repo.create("目录".into(), None, String::new()).unwrap();
        let target = repo.create("旧标题".into(), None, String::new()).unwrap();
        let source = repo
            .create(
                "来源".into(),
                None,
                format!("[旧标题](qingyu://page/{})", target.id),
            )
            .unwrap();

        assert_eq!(
            repo.links(&source.id).unwrap()[0].target_id,
            Some(target.id.clone())
        );
        assert_eq!(repo.backlinks(&target.id).unwrap()[0].source_id, source.id);
        repo.update(&target.id, "新标题".into(), String::new())
            .unwrap();
        repo.move_page(&target.id, Some(folder.id), 0).unwrap();
        assert_eq!(
            repo.links(&source.id).unwrap()[0].target_id,
            Some(target.id)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn page_lifecycle_preserves_uuid_and_supports_trash_restore() {
        let root = std::env::temp_dir().join(format!("qingyu-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let parent = repo.create("项目".into(), None, "[[记录]]".into()).unwrap();
        let child = repo
            .create("记录".into(), Some(parent.id.clone()), "正文".into())
            .unwrap();
        let updated = repo
            .update(&child.id, "记录2".into(), "更新".into())
            .unwrap();
        assert_eq!(updated.id, child.id);
        assert!(root.join("项目").join("记录2.md").exists());
        assert!(repo
            .search("更新")
            .unwrap()
            .iter()
            .any(|result| result.page.id == child.id));
        assert_eq!(repo.backlinks(&child.id).unwrap().len(), 1);
        repo.trash(&child.id).unwrap();
        assert_eq!(repo.list(true).unwrap().len(), 2);
        repo.restore(&child.id).unwrap();
        assert!(repo.get(&child.id, false).is_ok());
        assert!(repo.history(&child.id).unwrap().len() >= 4);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pages_support_deep_nesting_and_reject_parent_cycles() {
        let root = std::env::temp_dir().join(format!("qingyu-nested-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let level_one = repo.create("一级".into(), None, String::new()).unwrap();
        let level_two = repo
            .create("二级".into(), Some(level_one.id.clone()), String::new())
            .unwrap();
        let level_three = repo
            .create("三级".into(), Some(level_two.id.clone()), String::new())
            .unwrap();
        let level_four = repo
            .create("四级".into(), Some(level_three.id.clone()), "正文".into())
            .unwrap();

        assert_eq!(repo.get(&level_four.id, false).unwrap().content, "正文");
        let error = repo
            .move_page(&level_one.id, Some(level_four.id), 0)
            .unwrap_err();
        assert_eq!(error.code, "page_invalid_parent");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn image_assets_are_validated_and_stored_inside_the_page_directory() {
        let root = std::env::temp_dir().join(format!("qingyu-asset-test-{}", Uuid::new_v4()));
        let repo = PageRepository::open(root.clone()).unwrap();
        let page = repo.create("图片".into(), None, String::new()).unwrap();
        let png = [137, 80, 78, 71, 13, 10, 26, 10, 0];

        let relative = repo.store_asset(&page.id, "image.png", &png).unwrap();
        assert!(relative.starts_with(&format!("/.noteapp/assets/{}/", page.id)));
        assert!(relative.ends_with(".png"));
        assert_eq!(
            fs::read(root.join(relative.trim_start_matches('/'))).unwrap(),
            png
        );

        assert_eq!(
            repo.store_asset(&page.id, "../image.png", &png)
                .unwrap_err()
                .code,
            "asset_invalid_name"
        );
        assert_eq!(
            repo.store_asset(&page.id, "image.png", b"not an image")
                .unwrap_err()
                .code,
            "asset_invalid_type"
        );
        assert_eq!(
            repo.store_asset("missing-page", "image.png", &png)
                .unwrap_err()
                .code,
            "page_not_found"
        );
        assert_eq!(
            repo.store_asset(&page.id, "large.png", &vec![0; 20 * 1024 * 1024 + 1])
                .unwrap_err()
                .code,
            "asset_too_large"
        );
        let _ = fs::remove_dir_all(root);
    }
}
