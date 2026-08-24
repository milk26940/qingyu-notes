use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pages::model::Page;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DatabaseFieldType {
    Text,
    Number,
    Select,
    MultiSelect,
    Date,
    Checkbox,
    Relation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseField {
    pub id: String,
    pub name: String,
    pub field_type: DatabaseFieldType,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseDefinition {
    pub id: String,
    pub page_id: String,
    pub fields: Vec<DatabaseField>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseCreateResult {
    pub page: Page,
    pub definition: DatabaseDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    Equals,
    NotEquals,
    Contains,
    GreaterThan,
    LessThan,
    IsEmpty,
    IsNotEmpty,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseFilter {
    pub field_id: String,
    pub operator: FilterOperator,
    #[serde(default)]
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSort {
    pub field_id: String,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseQuery {
    #[serde(default)]
    pub filters: Vec<DatabaseFilter>,
    #[serde(default)]
    pub sorts: Vec<DatabaseSort>,
}
