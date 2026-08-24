use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    Page,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub id: String,
    pub title: String,
    pub parent_id: Option<String>,
    pub kind: PageKind,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
    pub trashed_at: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PageSummary {
    pub id: String,
    pub title: String,
    pub parent_id: Option<String>,
    pub kind: PageKind,
    pub path: String,
    pub sort_order: i64,
    pub trashed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PageLink {
    pub source_id: String,
    pub target_id: Option<String>,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PageHistoryEntry {
    pub id: String,
    pub page_id: String,
    pub action: String,
    pub timestamp: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub page: PageSummary,
    pub snippet: String,
}
