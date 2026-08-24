import { create } from "zustand";
import * as api from "../lib/tauri/databases";
import type { DatabaseBundle, DatabaseFieldType, DatabaseFilter, DatabaseSort } from "../lib/tauri/databases";
import type { Page } from "../lib/tauri/pages";
import { getPage } from "../lib/tauri/pages";
import { usePagesStore } from "./pagesStore";

type DatabaseStore = {
  bundle: DatabaseBundle | null;
  records: Page[];
  filters: DatabaseFilter[];
  sorts: DatabaseSort[];
  loading: boolean;
  error: string | null;
  create: (title?: string, parentId?: string | null) => Promise<void>;
  open: (id: string) => Promise<void>;
  addField: (name: string, fieldType: DatabaseFieldType, options?: string[]) => Promise<void>;
  addRecord: () => Promise<void>;
  updateRecord: (record: Page, title: string, properties: Record<string, unknown>) => Promise<void>;
  setFilters: (filters: DatabaseFilter[]) => Promise<void>;
  setSorts: (sorts: DatabaseSort[]) => Promise<void>;
  refreshExternalChange: () => Promise<void>;
};

const message = (error: unknown) => typeof error === "object" && error && "message" in error ? String(error.message) : String(error);

export const useDatabaseStore = create<DatabaseStore>((set, get) => ({
  bundle: null, records: [], filters: [], sorts: [], loading: false, error: null,
  create: async (title = "未命名数据库", parentId = null) => { set({ loading: true }); try { const bundle = await api.createDatabase(title, parentId); set({ bundle, records: [], error: null }); await usePagesStore.getState().refresh(); await usePagesStore.getState().open(bundle.page.id); } catch (error) { set({ error: message(error) }); } finally { set({ loading: false }); } },
  open: async (id) => { set({ loading: true }); try { const page = await getPage(id); const definition = await api.getDatabase(id); const view = definition.views.find((item) => item.viewType === "table"); const filters = view?.filters ?? []; const sorts = view?.sorts ?? []; const records = await api.queryDatabase(id, filters, sorts); set({ bundle: { page, definition }, records, filters, sorts, error: null }); } catch (error) { set({ error: message(error) }); } finally { set({ loading: false }); } },
  addField: async (name, fieldType, options = []) => { const id = get().bundle?.page.id; if (!id) return; try { await api.addDatabaseField(id, name, fieldType, options); const definition = await api.getDatabase(id); set((state) => state.bundle ? { bundle: { ...state.bundle, definition } } : state); } catch (error) { set({ error: message(error) }); } },
  addRecord: async () => { const id = get().bundle?.page.id; if (!id) return; try { await api.createDatabaseRecord(id, "未命名记录"); await get().open(id); await usePagesStore.getState().refresh(); } catch (error) { set({ error: message(error) }); } },
  updateRecord: async (record, title, properties) => { const databaseId = get().bundle?.page.id; if (!databaseId) return; try { const updated = await api.updateDatabaseRecord(databaseId, record.id, title, properties); set((state) => ({ records: state.records.map((item) => item.id === updated.id ? updated : item), error: null })); await usePagesStore.getState().refresh(); if (usePagesStore.getState().activePage?.id === updated.id) await usePagesStore.getState().open(updated.id); } catch (error) { set({ error: message(error) }); } },
  setFilters: async (filters) => { const id = get().bundle?.page.id; const sorts = get().sorts; set({ filters }); if (id) { const [definition, records] = await Promise.all([api.updateTableView(id, filters, sorts), api.queryDatabase(id, filters, sorts)]); set((state) => ({ bundle: state.bundle ? { ...state.bundle, definition } : state.bundle, records })); } },
  setSorts: async (sorts) => { const id = get().bundle?.page.id; const filters = get().filters; set({ sorts }); if (id) { const [definition, records] = await Promise.all([api.updateTableView(id, filters, sorts), api.queryDatabase(id, filters, sorts)]); set((state) => ({ bundle: state.bundle ? { ...state.bundle, definition } : state.bundle, records })); } },
  refreshExternalChange: async () => { const id = get().bundle?.page.id; if (id) await get().open(id); },
}));
