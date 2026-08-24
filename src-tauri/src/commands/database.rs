use std::collections::BTreeMap;

use serde_json::Value;
use tauri::State;

use super::pages::PageState;
use crate::database::model::{
    DatabaseCreateResult, DatabaseDefinition, DatabaseField, DatabaseFieldType, DatabaseQuery,
};
use crate::database::repository::DatabaseRepository;
use crate::error::AppError;
use crate::pages::model::Page;

async fn repository(state: &State<'_, PageState>) -> Result<DatabaseRepository, AppError> {
    DatabaseRepository::from_pages(super::pages::repo(state).await?)
}

#[tauri::command]
pub async fn database_create(
    state: State<'_, PageState>,
    title: String,
    parent_id: Option<String>,
) -> Result<DatabaseCreateResult, AppError> {
    let (page, definition) = repository(&state)
        .await?
        .create_database(title, parent_id)?;
    Ok(DatabaseCreateResult { page, definition })
}

#[tauri::command]
pub async fn database_get(
    state: State<'_, PageState>,
    database_id: String,
) -> Result<DatabaseDefinition, AppError> {
    repository(&state).await?.get(&database_id)
}

#[tauri::command]
pub async fn database_add_field(
    state: State<'_, PageState>,
    database_id: String,
    name: String,
    field_type: DatabaseFieldType,
    options: Vec<String>,
) -> Result<DatabaseField, AppError> {
    repository(&state)
        .await?
        .add_field(&database_id, name, field_type, options)
}

#[tauri::command]
pub async fn database_create_record(
    state: State<'_, PageState>,
    database_id: String,
    title: String,
    properties: BTreeMap<String, Value>,
) -> Result<Page, AppError> {
    repository(&state)
        .await?
        .create_record(&database_id, title, properties)
}

#[tauri::command]
pub async fn database_update_record(
    state: State<'_, PageState>,
    database_id: String,
    record_id: String,
    title: String,
    properties: BTreeMap<String, Value>,
) -> Result<Page, AppError> {
    repository(&state)
        .await?
        .update_record(&database_id, &record_id, title, properties)
}

#[tauri::command]
pub async fn database_query(
    state: State<'_, PageState>,
    database_id: String,
    query: DatabaseQuery,
) -> Result<Vec<Page>, AppError> {
    repository(&state).await?.query(&database_id, query)
}
