use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use uuid::Uuid;

use super::model::{
    default_views, DatabaseDefinition, DatabaseField, DatabaseFieldType, DatabaseQuery,
    DatabaseViewType, FilterOperator, SortDirection,
};
use crate::error::AppError;
use crate::pages::model::{Page, PageKind};
use crate::pages::repository::{shared_vault_lock, PageRepository};

#[derive(Clone)]
pub struct DatabaseRepository {
    pages: PageRepository,
    vault_lock: Arc<Mutex<()>>,
}

impl DatabaseRepository {
    #[cfg(test)]
    pub fn open(root: PathBuf) -> Result<Self, AppError> {
        let pages = PageRepository::open(root)?;
        Self::from_pages(pages)
    }

    pub fn from_pages(pages: PageRepository) -> Result<Self, AppError> {
        let vault_lock = shared_vault_lock(pages.root());
        Ok(Self {
            pages,
            vault_lock,
        })
    }

    pub fn create_database(
        &self,
        title: String,
        parent_id: Option<String>,
    ) -> Result<(Page, DatabaseDefinition), AppError> {
        self.create_database_with_hook(title, parent_id, |_| {})
    }

    fn create_database_with_hook<F>(
        &self,
        title: String,
        parent_id: Option<String>,
        after_page_created: F,
    ) -> Result<(Page, DatabaseDefinition), AppError>
    where
        F: FnOnce(&Page),
    {
        self.pages.with_transaction(|transaction| {
            let page = self.pages.create_typed_tx(transaction, title, parent_id, String::new(), PageKind::Database, None, BTreeMap::new())?;
            after_page_created(&page);
            let timestamp = timestamp();
            let definition = DatabaseDefinition { id: page.id.clone(), page_id: page.id.clone(), fields: Vec::new(), views: default_views(), created_at: timestamp.clone(), updated_at: timestamp };
            write_definition_tx(transaction, &definition, &page.title)?;
            Ok((page, definition))
        })
    }

    pub fn get(&self, id: &str) -> Result<DatabaseDefinition, AppError> {
        self.read_definition_sqlite(id)?
            .ok_or_else(|| AppError::new("database_not_found", "数据库不存在"))
    }

    pub fn add_field(
        &self,
        database_id: &str,
        name: String,
        field_type: DatabaseFieldType,
        options: Vec<String>,
    ) -> Result<DatabaseField, AppError> {
        let _guard = self.lock_vault()?;
        let mut definition = self.get(database_id)?;
        self.add_field_unlocked(&mut definition, name, field_type, options)
    }

    pub fn add_field_checked(
        &self,
        database_id: &str,
        expected: &DatabaseDefinition,
        name: String,
        field_type: DatabaseFieldType,
        options: Vec<String>,
    ) -> Result<DatabaseField, AppError> {
        let _guard = self.lock_vault()?;
        let mut definition = self.get(database_id)?;
        if &definition != expected {
            return Err(AppError::new(
                "approval_stale_data",
                "数据库结构在审批期间已发生变化，请重新发起操作",
            ));
        }
        self.add_field_unlocked(&mut definition, name, field_type, options)
    }

    fn add_field_unlocked(
        &self,
        definition: &mut DatabaseDefinition,
        name: String,
        field_type: DatabaseFieldType,
        options: Vec<String>,
    ) -> Result<DatabaseField, AppError> {
        if name.trim().is_empty() {
            return Err(AppError::new("database_invalid_field", "字段名称不能为空"));
        }
        let options = normalize_options(options);
        let field = DatabaseField {
            id: Uuid::new_v4().to_string(),
            name: name.trim().to_owned(),
            field_type,
            options,
        };
        definition.fields.push(field.clone());
        definition.updated_at = timestamp();
        self.write_definition_unlocked(definition)?;
        Ok(field)
    }

    pub fn create_record(
        &self,
        database_id: &str,
        title: String,
        properties: BTreeMap<String, Value>,
    ) -> Result<Page, AppError> {
        let definition = self.get(database_id)?;
        self.validate_properties(&definition, &properties)?;
        self.pages.with_transaction(|transaction| {
            let page = self.pages.create_typed_tx(transaction, title, Some(definition.page_id), String::new(), PageKind::DatabaseRecord, Some(definition.id), properties.clone())?;
            write_record_values_tx(transaction, &page, &properties)?;
            Ok(page)
        })
    }

    pub fn update_record(
        &self,
        database_id: &str,
        record_id: &str,
        title: String,
        properties: BTreeMap<String, Value>,
    ) -> Result<Page, AppError> {
        self.update_record_unlocked(database_id, record_id, title, properties, None)
    }

    pub fn update_record_checked(
        &self,
        database_id: &str,
        record_id: &str,
        title: String,
        properties: BTreeMap<String, Value>,
        expected: &Page,
    ) -> Result<Page, AppError> {
        self.update_record_unlocked(database_id, record_id, title, properties, Some(expected))
    }

    fn update_record_unlocked(
        &self,
        database_id: &str,
        record_id: &str,
        title: String,
        properties: BTreeMap<String, Value>,
        expected: Option<&Page>,
    ) -> Result<Page, AppError> {
        let definition = self.get(database_id)?;
        let record = self.pages.get(record_id, false)?;
        if expected.is_some_and(|expected| expected != &record) {
            return Err(AppError::new(
                "approval_stale_data",
                "数据库记录在审批期间已发生变化，请重新发起操作",
            ));
        }
        if record.kind != PageKind::DatabaseRecord
            || record.database_id.as_deref() != Some(database_id)
        {
            return Err(AppError::new(
                "database_record_not_found",
                "记录不属于当前数据库",
            ));
        }
        self.validate_properties(&definition, &properties)?;
        self.pages.with_transaction(|transaction| {
            let updated = self.pages.update_properties_tx(transaction, record, title, properties.clone())?;
            write_record_values_tx(transaction, &updated, &properties)?;
            Ok(updated)
        })
    }

    pub fn query(&self, database_id: &str, query: DatabaseQuery) -> Result<Vec<Page>, AppError> {
        let definition = self.get(database_id)?;
        self.validate_query(&definition, &query)?;
        self.query_validated(database_id, query)
    }

    pub fn update_table_view(
        &self,
        database_id: &str,
        query: DatabaseQuery,
    ) -> Result<DatabaseDefinition, AppError> {
        let _guard = self.lock_vault()?;
        let mut definition = self.get(database_id)?;
        self.validate_query(&definition, &query)?;
        let view = definition
            .views
            .iter_mut()
            .find(|view| view.view_type == DatabaseViewType::Table)
            .ok_or_else(|| AppError::new("database_table_view_not_found", "数据库缺少表格视图"))?;
        view.filters = query.filters;
        view.sorts = query.sorts;
        definition.updated_at = timestamp();
        self.write_definition_unlocked(&definition)?;
        Ok(definition)
    }

    fn validate_query(
        &self,
        definition: &DatabaseDefinition,
        query: &DatabaseQuery,
    ) -> Result<(), AppError> {
        let fields: HashMap<_, _> = definition
            .fields
            .iter()
            .map(|field| (field.id.as_str(), field))
            .collect();
        for filter in &query.filters {
            if !fields.contains_key(filter.field_id.as_str()) {
                return Err(AppError::new("database_field_not_found", "筛选字段不存在"));
            }
        }
        for sort in &query.sorts {
            if !fields.contains_key(sort.field_id.as_str()) {
                return Err(AppError::new("database_field_not_found", "排序字段不存在"));
            }
        }
        Ok(())
    }

    fn query_validated(
        &self,
        database_id: &str,
        query: DatabaseQuery,
    ) -> Result<Vec<Page>, AppError> {
        let mut records: Vec<_> = self
            .record_ids(database_id)?
            .into_iter()
            .map(|record_id| self.load_record(&record_id))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|page| {
                query
                    .filters
                    .iter()
                    .all(|filter| matches_filter(page.properties.get(&filter.field_id), filter))
            })
            .collect();
        records.sort_by(|left, right| {
            for sort in &query.sorts {
                let ordering = compare_values(
                    left.properties.get(&sort.field_id),
                    right.properties.get(&sort.field_id),
                );
                let ordering = match sort.direction {
                    SortDirection::Ascending => ordering,
                    SortDirection::Descending => ordering.reverse(),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            left.sort_order
                .cmp(&right.sort_order)
                .then_with(|| left.title.cmp(&right.title))
        });
        Ok(records)
    }

    fn validate_properties(
        &self,
        definition: &DatabaseDefinition,
        properties: &BTreeMap<String, Value>,
    ) -> Result<(), AppError> {
        let fields: HashMap<_, _> = definition
            .fields
            .iter()
            .map(|field| (field.id.as_str(), field))
            .collect();
        for (field_id, value) in properties {
            let field = fields
                .get(field_id.as_str())
                .ok_or_else(|| AppError::new("database_field_not_found", "记录包含不存在的字段"))?;
            if value.is_null() {
                continue;
            }
            let valid = match field.field_type {
                DatabaseFieldType::Text | DatabaseFieldType::Date => value.is_string(),
                DatabaseFieldType::Number => value.is_number(),
                DatabaseFieldType::Checkbox => value.is_boolean(),
                DatabaseFieldType::Select => value
                    .as_str()
                    .map(|item| field.options.iter().any(|option| option == item))
                    .unwrap_or(false),
                DatabaseFieldType::MultiSelect => value
                    .as_array()
                    .map(|items| {
                        items.iter().all(|item| {
                            item.as_str()
                                .map(|item| field.options.iter().any(|option| option == item))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false),
                DatabaseFieldType::Relation => {
                    let Some(ids) = value.as_array() else {
                        return Err(AppError::new(
                            "database_invalid_property",
                            format!("字段 {} 的值类型不正确", field.name),
                        ));
                    };
                    for id in ids {
                        let Some(id) = id.as_str() else {
                            return Err(AppError::new(
                                "database_invalid_property",
                                format!("字段 {} 的值类型不正确", field.name),
                            ));
                        };
                        self.pages.get(id, false).map_err(|_| {
                            AppError::new("database_invalid_relation", "关联页面不存在")
                        })?;
                    }
                    true
                }
            };
            if !valid {
                return Err(AppError::new(
                    "database_invalid_property",
                    format!("字段 {} 的值类型不正确", field.name),
                ));
            }
        }
        Ok(())
    }

    fn write_definition_unlocked(&self, definition: &DatabaseDefinition) -> Result<(), AppError> {
        let mut connection = self.open_connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| AppError::new("database_write_failed", error.to_string()))?;
        let title = self
            .pages
            .get(&definition.page_id, true)
            .map(|page| page.title)
            .unwrap_or_default();
        let config_json = serde_json::to_string(definition)
            .map_err(|error| AppError::new("database_write_failed", error.to_string()))?;
        transaction.execute(
            "INSERT INTO databases(id,page_id,title,config_json) VALUES(?1,?2,?3,?4)
             ON CONFLICT(id) DO UPDATE SET page_id=excluded.page_id,title=excluded.title,config_json=excluded.config_json",
            params![definition.id, definition.page_id, title, config_json],
        ).map_err(|error| AppError::new("database_write_failed", error.to_string()))?;
        for (position, field) in definition.fields.iter().enumerate() {
            transaction.execute(
                "INSERT INTO database_fields(id,database_id,name,field_type,position,config_json) VALUES(?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(id) DO UPDATE SET database_id=excluded.database_id,name=excluded.name,field_type=excluded.field_type,position=excluded.position,config_json=excluded.config_json",
                params![field.id,definition.id,field.name,serde_json::to_string(&field.field_type).unwrap(),position as i64,serde_json::to_string(&field.options).unwrap()],
            ).map_err(|error| AppError::new("database_write_failed", error.to_string()))?;
        }
        let retained = definition
            .fields
            .iter()
            .map(|field| field.id.as_str())
            .collect::<Vec<_>>();
        let mut existing = transaction
            .prepare("SELECT id FROM database_fields WHERE database_id=?1")
            .map_err(database_write_error)?;
        let stale = existing
            .query_map([&definition.id], |row| row.get::<_, String>(0))
            .map_err(database_write_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_write_error)?;
        drop(existing);
        for field_id in stale
            .into_iter()
            .filter(|field_id| !retained.contains(&field_id.as_str()))
        {
            transaction
                .execute("DELETE FROM database_fields WHERE id=?1", [field_id])
                .map_err(database_write_error)?;
        }
        transaction
            .commit()
            .map_err(|error| AppError::new("database_write_failed", error.to_string()))
    }

    fn open_connection(&self) -> Result<Connection, AppError> {
        let connection = Connection::open(self.pages.database_path())
            .map_err(|error| AppError::new("database_open_failed", error.to_string()))?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")
            .map_err(|error| AppError::new("database_open_failed", error.to_string()))?;
        Ok(connection)
    }

    fn read_definition_sqlite(&self, id: &str) -> Result<Option<DatabaseDefinition>, AppError> {
        let connection = self.open_connection()?;
        let config: Option<String> = connection
            .query_row(
                "SELECT config_json FROM databases WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| AppError::new("database_read_failed", error.to_string()))?;
        let Some(config) = config else {
            return Ok(None);
        };
        let mut definition: DatabaseDefinition = serde_json::from_str(&config)
            .map_err(|error| AppError::new("database_invalid_definition", error.to_string()))?;
        let mut statement = connection.prepare(
            "SELECT id,name,field_type,config_json FROM database_fields WHERE database_id=?1 ORDER BY position",
        ).map_err(|error| AppError::new("database_read_failed", error.to_string()))?;
        let rows = statement
            .query_map([id], |row| {
                let field_type: String = row.get(2)?;
                let options: String = row.get(3)?;
                Ok(DatabaseField {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    field_type: serde_json::from_str(&field_type)
                        .unwrap_or(DatabaseFieldType::Text),
                    options: serde_json::from_str(&options).unwrap_or_default(),
                })
            })
            .map_err(|error| AppError::new("database_read_failed", error.to_string()))?;
        definition.fields = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::new("database_read_failed", error.to_string()))?;
        Ok(Some(definition))
    }

    fn write_record_values(
        &self,
        page: &Page,
        properties: &BTreeMap<String, Value>,
    ) -> Result<(), AppError> {
        let database_id = page
            .database_id
            .as_deref()
            .ok_or_else(|| AppError::new("database_record_not_found", "记录缺少数据库标识"))?;
        let mut connection = self.open_connection()?;
        let transaction = connection.transaction().map_err(database_write_error)?;
        transaction.execute(
            "INSERT INTO database_rows(id,database_id,page_id,position) VALUES(?1,?2,?3,?4)
             ON CONFLICT(id) DO UPDATE SET database_id=excluded.database_id,page_id=excluded.page_id,position=excluded.position",
            params![page.id,database_id,page.id,page.sort_order],
        ).map_err(database_write_error)?;
        transaction
            .execute("DELETE FROM property_values WHERE row_id=?1", [&page.id])
            .map_err(database_write_error)?;
        for (field_id, value) in properties {
            transaction
                .execute(
                    "INSERT INTO property_values(row_id,field_id,value_json) VALUES(?1,?2,?3)",
                    params![page.id, field_id, serde_json::to_string(value).unwrap()],
                )
                .map_err(database_write_error)?;
        }
        transaction.commit().map_err(database_write_error)
    }

    fn record_ids(&self, database_id: &str) -> Result<Vec<String>, AppError> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare("SELECT page_id FROM database_rows WHERE database_id=?1 ORDER BY position,id")
            .map_err(|error| AppError::new("database_read_failed", error.to_string()))?;
        let rows = statement
            .query_map([database_id], |row| row.get(0))
            .map_err(|error| AppError::new("database_read_failed", error.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::new("database_read_failed", error.to_string()))
    }

    fn load_record(&self, record_id: &str) -> Result<Page, AppError> {
        let mut page = self.pages.get(record_id, false)?;
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare("SELECT field_id,value_json FROM property_values WHERE row_id=?1")
            .map_err(|error| AppError::new("database_read_failed", error.to_string()))?;
        let rows = statement
            .query_map([record_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| AppError::new("database_read_failed", error.to_string()))?;
        page.properties = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_read_error)?
            .into_iter()
            .map(|(id, json)| {
                serde_json::from_str(&json)
                    .map(|value| (id, value))
                    .map_err(|error| AppError::new("database_invalid_record", error.to_string()))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok(page)
    }

    fn lock_vault(&self) -> Result<std::sync::MutexGuard<'_, ()>, AppError> {
        self.vault_lock
            .lock()
            .map_err(|_| AppError::new("vault_write_lock_failed", "笔记库写入锁已损坏"))
    }
}

fn write_definition_tx(transaction: &rusqlite::Transaction<'_>, definition: &DatabaseDefinition, title: &str) -> Result<(), AppError> {
    let config_json = serde_json::to_string(definition).map_err(|error| AppError::new("database_write_failed", error.to_string()))?;
    transaction.execute("INSERT INTO databases(id,page_id,title,config_json) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET page_id=excluded.page_id,title=excluded.title,config_json=excluded.config_json", params![definition.id,definition.page_id,title,config_json]).map_err(database_write_error)?;
    for (position,field) in definition.fields.iter().enumerate() {
        transaction.execute("INSERT INTO database_fields(id,database_id,name,field_type,position,config_json) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET database_id=excluded.database_id,name=excluded.name,field_type=excluded.field_type,position=excluded.position,config_json=excluded.config_json", params![field.id,definition.id,field.name,serde_json::to_string(&field.field_type).unwrap(),position as i64,serde_json::to_string(&field.options).unwrap()]).map_err(database_write_error)?;
    }
    Ok(())
}

fn write_record_values_tx(transaction: &rusqlite::Transaction<'_>, page: &Page, properties: &BTreeMap<String,Value>) -> Result<(), AppError> {
    let database_id = page.database_id.as_deref().ok_or_else(|| AppError::new("database_record_not_found", "记录缺少数据库标识"))?;
    transaction.execute("INSERT INTO database_rows(id,database_id,page_id,position) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET database_id=excluded.database_id,page_id=excluded.page_id,position=excluded.position", params![page.id,database_id,page.id,page.sort_order]).map_err(database_write_error)?;
    transaction.execute("DELETE FROM property_values WHERE row_id=?1", [&page.id]).map_err(database_write_error)?;
    for (field_id,value) in properties { transaction.execute("INSERT INTO property_values(row_id,field_id,value_json) VALUES(?1,?2,?3)", params![page.id,field_id,serde_json::to_string(value).unwrap()]).map_err(database_write_error)?; }
    Ok(())
}

fn normalize_options(options: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for option in options {
        let option = option.trim();
        if !option.is_empty() && !normalized.iter().any(|item| item == option) {
            normalized.push(option.to_owned());
        }
    }
    normalized
}

fn database_write_error(error: rusqlite::Error) -> AppError {
    AppError::new("database_write_failed", error.to_string())
}

fn database_read_error(error: rusqlite::Error) -> AppError {
    AppError::new("database_read_failed", error.to_string())
}

fn matches_filter(value: Option<&Value>, filter: &super::model::DatabaseFilter) -> bool {
    match filter.operator {
        FilterOperator::IsEmpty => value.map(is_empty).unwrap_or(true),
        FilterOperator::IsNotEmpty => value.map(|value| !is_empty(value)).unwrap_or(false),
        FilterOperator::Equals => value == Some(&filter.value),
        FilterOperator::NotEquals => value != Some(&filter.value),
        FilterOperator::Contains => value
            .map(|value| match value {
                Value::String(text) => filter
                    .value
                    .as_str()
                    .map(|needle| text.to_lowercase().contains(&needle.to_lowercase()))
                    .unwrap_or(false),
                Value::Array(items) => items.contains(&filter.value),
                _ => false,
            })
            .unwrap_or(false),
        FilterOperator::GreaterThan => {
            compare_values(value, Some(&filter.value)) == Ordering::Greater
        }
        FilterOperator::LessThan => compare_values(value, Some(&filter.value)) == Ordering::Less,
    }
}

fn is_empty(value: &Value) -> bool {
    value.is_null()
        || value.as_str().map(str::is_empty).unwrap_or(false)
        || value.as_array().map(Vec::is_empty).unwrap_or(false)
}

fn compare_values(left: Option<&Value>, right: Option<&Value>) -> Ordering {
    match (left, right) {
        (None | Some(Value::Null), None | Some(Value::Null)) => Ordering::Equal,
        (None | Some(Value::Null), _) => Ordering::Greater,
        (_, None | Some(Value::Null)) => Ordering::Less,
        (Some(Value::Number(left)), Some(Value::Number(right))) => left
            .as_f64()
            .partial_cmp(&right.as_f64())
            .unwrap_or(Ordering::Equal),
        (Some(Value::Bool(left)), Some(Value::Bool(right))) => left.cmp(right),
        (Some(Value::String(left)), Some(Value::String(right))) => {
            left.to_lowercase().cmp(&right.to_lowercase())
        }
        (Some(left), Some(right)) => left.to_string().cmp(&right.to_string()),
    }
}

fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use serde_json::json;
    use uuid::Uuid;

    use super::super::model::{
        DatabaseFieldType, DatabaseFilter, DatabaseQuery, DatabaseSort, DatabaseViewType,
        FilterOperator, SortDirection,
    };
    use super::DatabaseRepository;
    use crate::pages::model::PageKind;

    fn temporary_repository() -> (DatabaseRepository, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("qingyu-database-test-{}", Uuid::new_v4()));
        (DatabaseRepository::open(root.clone()).unwrap(), root)
    }

    #[test]
    fn database_creation_holds_the_vault_lock_for_page_and_definition() {
        let (repo, root) = temporary_repository();
        let worker_repo = repo.clone();
        let (entered_sender, entered_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            worker_repo.create_database_with_hook("任务".into(), None, |_| {
                entered_sender.send(()).unwrap();
                release_receiver.recv().unwrap();
            })
        });

        entered_receiver.recv().unwrap();
        assert!(repo.vault_lock.try_lock().is_err());
        release_sender.send(()).unwrap();
        let (page, definition) = worker.join().unwrap().unwrap();

        assert_eq!(page.id, definition.id);
        assert!(repo.get(&definition.id).is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_json_collision_does_not_affect_sqlite_definition_write() {
        let (repo, root) = temporary_repository();

        let result = repo.create_database_with_hook("任务".into(), None, |page| {
            fs::create_dir_all(
                root.join(".noteapp")
                    .join("databases")
                    .join(format!("{}.json", page.id)),
            )
            .unwrap();
        });

        assert!(result.is_ok());
        assert_eq!(repo.pages.list(false).unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_sqlite_definition_does_not_fall_back_to_legacy_json() {
        let root = std::env::temp_dir().join(format!(
            "qingyu-no-json-fallback-test-{}",
            Uuid::new_v4()
        ));
        let pages = crate::pages::repository::PageRepository::open(root.clone()).unwrap();
        let database_page = pages
            .create_typed(
                "旧数据库".into(),
                None,
                String::new(),
                PageKind::Database,
                None,
                BTreeMap::new(),
            )
            .unwrap();
        let definitions = root.join(".noteapp").join("databases");
        fs::create_dir_all(&definitions).unwrap();
        fs::write(
            definitions.join(format!("{}.json", database_page.id)),
            format!(
                r#"{{"id":"{0}","pageId":"{0}","fields":[],"views":[],"createdAt":"1","updatedAt":"1"}}"#,
                database_page.id
            ),
        )
        .unwrap();

        let repository = DatabaseRepository::from_pages(pages).unwrap();
        let error = repository.get(&database_page.id).unwrap_err();

        assert_eq!(error.code, "database_not_found");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn database_definition_is_persisted_and_records_are_pages() {
        let (repo, root) = temporary_repository();
        let (database_page, mut definition) = repo.create_database("任务".into(), None).unwrap();
        assert_eq!(database_page.kind, PageKind::Database);
        let status = repo
            .add_field(
                &definition.id,
                "状态".into(),
                DatabaseFieldType::Select,
                vec!["待办".into(), "完成".into()],
            )
            .unwrap();
        definition = repo.get(&definition.id).unwrap();
        assert!(definition.fields.iter().any(|field| field.id == status.id));

        let mut properties = BTreeMap::new();
        properties.insert(status.id, json!("待办"));
        let record = repo
            .create_record(&definition.id, "修复问题".into(), properties)
            .unwrap();
        assert_eq!(record.kind, PageKind::DatabaseRecord);
        assert_eq!(record.database_id.as_deref(), Some(definition.id.as_str()));
        assert_eq!(record.parent_id, Some(database_page.id));

        let legacy_json = root
            .join(".noteapp")
            .join("databases")
            .join(format!("{}.json", definition.id));
        let _ = fs::remove_file(legacy_json);
        assert_eq!(repo.get(&definition.id).unwrap().fields.len(), 1);
        let connection = rusqlite::Connection::open(repo.pages.database_path()).unwrap();
        let counts = [
            "databases",
            "database_fields",
            "database_rows",
            "property_values",
        ]
        .map(|table| {
            connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap()
        });
        assert_eq!(counts, [1, 1, 1, 1]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn records_are_validated_filtered_and_sorted_by_field_definition() {
        let (repo, root) = temporary_repository();
        let (_, definition) = repo.create_database("任务".into(), None).unwrap();
        let priority = repo
            .add_field(
                &definition.id,
                "优先级".into(),
                DatabaseFieldType::Number,
                Vec::new(),
            )
            .unwrap();
        let done = repo
            .add_field(
                &definition.id,
                "完成".into(),
                DatabaseFieldType::Checkbox,
                Vec::new(),
            )
            .unwrap();

        for (title, value, checked) in [("低", 1, false), ("高", 3, true), ("中", 2, true)] {
            let properties = BTreeMap::from([
                (priority.id.clone(), json!(value)),
                (done.id.clone(), json!(checked)),
            ]);
            repo.create_record(&definition.id, title.into(), properties)
                .unwrap();
        }

        let query = DatabaseQuery {
            filters: vec![DatabaseFilter {
                field_id: done.id,
                operator: FilterOperator::Equals,
                value: json!(true),
            }],
            sorts: vec![DatabaseSort {
                field_id: priority.id.clone(),
                direction: SortDirection::Descending,
            }],
        };
        let result = repo.query(&definition.id, query).unwrap();
        assert_eq!(
            result
                .iter()
                .map(|page| page.title.as_str())
                .collect::<Vec<_>>(),
            vec!["高", "中"]
        );

        let invalid = BTreeMap::from([(priority.id, json!("不是数字"))]);
        let error = repo
            .create_record(&definition.id, "错误".into(), invalid)
            .unwrap_err();
        assert_eq!(error.code, "database_invalid_property");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn relation_values_require_existing_page_ids() {
        let (repo, root) = temporary_repository();
        let (_, definition) = repo.create_database("任务".into(), None).unwrap();
        let relation = repo
            .add_field(
                &definition.id,
                "关联".into(),
                DatabaseFieldType::Relation,
                Vec::new(),
            )
            .unwrap();
        let properties = BTreeMap::from([(relation.id, json!(["missing-page"]))]);
        let error = repo
            .create_record(&definition.id, "错误关联".into(), properties)
            .unwrap_err();
        assert_eq!(error.code, "database_invalid_relation");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn every_field_type_and_table_view_are_persisted() {
        let (repo, root) = temporary_repository();
        let (_, definition) = repo.create_database("资料".into(), None).unwrap();
        let text = repo
            .add_field(
                &definition.id,
                "文本".into(),
                DatabaseFieldType::Text,
                vec![],
            )
            .unwrap();
        let number = repo
            .add_field(
                &definition.id,
                "数字".into(),
                DatabaseFieldType::Number,
                vec![],
            )
            .unwrap();
        let select = repo
            .add_field(
                &definition.id,
                "单选".into(),
                DatabaseFieldType::Select,
                vec!["甲".into()],
            )
            .unwrap();
        let multi = repo
            .add_field(
                &definition.id,
                "多选".into(),
                DatabaseFieldType::MultiSelect,
                vec!["乙".into(), "丙".into()],
            )
            .unwrap();
        let date = repo
            .add_field(
                &definition.id,
                "日期".into(),
                DatabaseFieldType::Date,
                vec![],
            )
            .unwrap();
        let checkbox = repo
            .add_field(
                &definition.id,
                "勾选".into(),
                DatabaseFieldType::Checkbox,
                vec![],
            )
            .unwrap();
        let target = repo
            .pages
            .create("目标".into(), None, String::new())
            .unwrap();
        let relation = repo
            .add_field(
                &definition.id,
                "关系".into(),
                DatabaseFieldType::Relation,
                vec![],
            )
            .unwrap();
        let properties = BTreeMap::from([
            (text.id.clone(), json!("内容")),
            (number.id.clone(), json!(42)),
            (select.id.clone(), json!("甲")),
            (multi.id.clone(), json!(["乙", "丙"])),
            (date.id.clone(), json!("2026-08-24")),
            (checkbox.id.clone(), json!(true)),
            (relation.id.clone(), json!([target.id])),
        ]);
        repo.create_record(&definition.id, "记录".into(), properties)
            .unwrap();

        let query = DatabaseQuery {
            filters: vec![DatabaseFilter {
                field_id: checkbox.id,
                operator: FilterOperator::Equals,
                value: json!(true),
            }],
            sorts: vec![DatabaseSort {
                field_id: number.id,
                direction: SortDirection::Descending,
            }],
        };
        repo.update_table_view(&definition.id, query.clone())
            .unwrap();
        let reopened = DatabaseRepository::open(root.clone()).unwrap();
        let persisted = reopened.get(&definition.id).unwrap();
        let table = persisted
            .views
            .iter()
            .find(|view| view.view_type == DatabaseViewType::Table)
            .unwrap();
        assert_eq!(table.filters, query.filters);
        assert_eq!(table.sorts, query.sorts);
        assert_eq!(reopened.query(&definition.id, query).unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }
}
