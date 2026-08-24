use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use uuid::Uuid;

use super::model::{
    default_views, DatabaseDefinition, DatabaseField, DatabaseFieldType, DatabaseQuery,
    DatabaseViewType, FilterOperator, SortDirection,
};
use crate::error::AppError;
use crate::pages::model::{Page, PageKind};
use crate::pages::repository::PageRepository;

#[derive(Clone)]
pub struct DatabaseRepository {
    pages: PageRepository,
    definitions_dir: PathBuf,
}

impl DatabaseRepository {
    #[cfg(test)]
    pub fn open(root: PathBuf) -> Result<Self, AppError> {
        let pages = PageRepository::open(root)?;
        Self::from_pages(pages)
    }

    pub fn from_pages(pages: PageRepository) -> Result<Self, AppError> {
        let definitions_dir = pages.root().join(".noteapp").join("databases");
        fs::create_dir_all(&definitions_dir)
            .map_err(|error| AppError::new("database_init_failed", error.to_string()))?;
        Ok(Self {
            pages,
            definitions_dir,
        })
    }

    pub fn create_database(
        &self,
        title: String,
        parent_id: Option<String>,
    ) -> Result<(Page, DatabaseDefinition), AppError> {
        let page = self.pages.create_typed(
            title,
            parent_id,
            String::new(),
            PageKind::Database,
            None,
            BTreeMap::new(),
        )?;
        let timestamp = timestamp();
        let definition = DatabaseDefinition {
            id: page.id.clone(),
            page_id: page.id.clone(),
            fields: Vec::new(),
            views: default_views(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
        };
        self.write_definition(&definition)?;
        Ok((page, definition))
    }

    pub fn get(&self, id: &str) -> Result<DatabaseDefinition, AppError> {
        let path = self.definition_path(id);
        let text = fs::read_to_string(path)
            .map_err(|error| AppError::new("database_not_found", error.to_string()))?;
        serde_json::from_str(&text)
            .map_err(|error| AppError::new("database_invalid_definition", error.to_string()))
    }

    pub fn add_field(
        &self,
        database_id: &str,
        name: String,
        field_type: DatabaseFieldType,
        options: Vec<String>,
    ) -> Result<DatabaseField, AppError> {
        let mut definition = self.get(database_id)?;
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
        self.write_definition(&definition)?;
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
        self.pages.create_typed(
            title,
            Some(definition.page_id),
            String::new(),
            PageKind::DatabaseRecord,
            Some(definition.id),
            properties,
        )
    }

    pub fn update_record(
        &self,
        database_id: &str,
        record_id: &str,
        title: String,
        properties: BTreeMap<String, Value>,
    ) -> Result<Page, AppError> {
        let definition = self.get(database_id)?;
        let record = self.pages.get(record_id, false)?;
        if record.kind != PageKind::DatabaseRecord
            || record.database_id.as_deref() != Some(database_id)
        {
            return Err(AppError::new(
                "database_record_not_found",
                "记录不属于当前数据库",
            ));
        }
        self.validate_properties(&definition, &properties)?;
        self.pages.update_properties(record_id, title, properties)
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
        self.write_definition(&definition)?;
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
            .pages
            .list(false)?
            .into_iter()
            .filter_map(|summary| self.pages.get(&summary.id, false).ok())
            .filter(|page| {
                page.kind == PageKind::DatabaseRecord
                    && page.database_id.as_deref() == Some(database_id)
            })
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

    fn definition_path(&self, id: &str) -> PathBuf {
        self.definitions_dir.join(format!("{}.json", id))
    }

    fn write_definition(&self, definition: &DatabaseDefinition) -> Result<(), AppError> {
        let path = self.definition_path(&definition.id);
        let temporary = path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(definition)
            .map_err(|error| AppError::new("database_write_failed", error.to_string()))?;
        fs::write(&temporary, json)
            .map_err(|error| AppError::new("database_write_failed", error.to_string()))?;
        fs::rename(temporary, path)
            .map_err(|error| AppError::new("database_write_failed", error.to_string()))
    }
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
    fn database_definition_is_persisted_and_records_are_pages() {
        let (repo, root) = temporary_repository();
        let (database_page, mut definition) = repo.create_database("任务".into(), None).unwrap();
        assert_eq!(database_page.kind, PageKind::Database);
        assert!(root
            .join(".noteapp/databases")
            .join(format!("{}.json", definition.id))
            .exists());

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
