use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PageKind {
    #[serde(rename = "page")]
    Page,
    #[serde(rename = "database")]
    Database,
    #[serde(rename = "databaseRecord")]
    DatabaseRecord,
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
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
    pub database_id: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlockNode {
    pub id: String,
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub attrs: BTreeMap<String, Value>,
    #[serde(default)]
    pub children: Vec<BlockNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PageDocument {
    pub page_id: String,
    pub title: String,
    pub blocks: Vec<BlockNode>,
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PageOpenBundle {
    pub page: Page,
    pub document: PageDocument,
    pub links: Vec<PageLink>,
    pub backlinks: Vec<PageLink>,
}
