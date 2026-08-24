import { invoke } from "@tauri-apps/api/core";
import type { Page } from "./pages";

export type DatabaseFieldType = "text" | "number" | "select" | "multiSelect" | "date" | "checkbox" | "relation";
export type FieldDefinition = { id: string; name: string; fieldType: DatabaseFieldType; options: string[] };
export type DatabaseDefinition = { id: string; pageId: string; fields: FieldDefinition[]; createdAt: string; updatedAt: string };
export type DatabaseBundle = { page: Page; definition: DatabaseDefinition };
export type FilterOperator = "equals" | "notEquals" | "contains" | "greaterThan" | "lessThan" | "isEmpty" | "isNotEmpty";
export type DatabaseFilter = { fieldId: string; operator: FilterOperator; value?: unknown };
export type DatabaseSort = { fieldId: string; direction: "ascending" | "descending" };

export const createDatabase = (title: string, parentId: string | null) => invoke<DatabaseBundle>("database_create", { title, parentId });
export const getDatabase = (databaseId: string) => invoke<DatabaseDefinition>("database_get", { databaseId });
export const addDatabaseField = (databaseId: string, name: string, fieldType: DatabaseFieldType, options: string[] = []) => invoke<FieldDefinition>("database_add_field", { databaseId, name, fieldType, options });
export const createDatabaseRecord = (databaseId: string, title: string, properties: Record<string, unknown> = {}) => invoke<Page>("database_create_record", { databaseId, title, properties });
export const updateDatabaseRecord = (databaseId: string, recordId: string, title: string, properties: Record<string, unknown>) => invoke<Page>("database_update_record", { databaseId, recordId, title, properties });
export const queryDatabase = (databaseId: string, filters: DatabaseFilter[] = [], sorts: DatabaseSort[] = []) => invoke<Page[]>("database_query", { databaseId, query: { filters, sorts } });
