import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type PageKind = "page" | "database" | "databaseRecord";
export type Page = { id: string; title: string; parentId: string | null; kind: PageKind; databaseId?: string | null; properties?: Record<string, unknown>; content: string; createdAt: string; updatedAt: string; trashedAt: string | null; sortOrder: number };
export type PageSummary = Pick<Page, "id" | "title" | "parentId" | "kind" | "sortOrder" | "trashedAt"> & { path: string };
export type PageLink = { sourceId: string; targetId: string | null; label: string };
export type PageHistoryEntry = { id: string; pageId: string; action: string; timestamp: string; title: string; content: string };
export type SearchResult = { page: PageSummary; snippet: string };
export type VaultChange = { source: "agent" | "restore" | "import"; namespace: string; tool: string; pageIds: string[]; databaseIds: string[] };

export const initVault = (path?: string) => invoke<string>("vault_init", { path: path ?? null });
export const listPages = (includeTrash = false) => invoke<PageSummary[]>("page_list", { includeTrash });
export const getPage = (id: string, includeTrash = false) => invoke<Page>("page_get", { id, includeTrash });
export const createPage = (title: string, parentId: string | null, content: string) => invoke<Page>("page_create", { title, parentId, content });
export const updatePage = (id: string, title: string, content: string) => invoke<Page>("page_update", { id, title, content });
export const storePageAsset = (pageId: string, originalName: string, bytes: number[]) => invoke<string>("page_store_asset", { pageId, originalName, bytes });
export const movePage = (id: string, parentId: string | null, sortOrder: number) => invoke<Page>("page_move", { id, parentId, sortOrder });
export const trashPage = (id: string) => invoke<Page>("page_trash", { id });
export const restorePage = (id: string) => invoke<Page>("page_restore", { id });
export const getPageHistory = (id: string) => invoke<PageHistoryEntry[]>("page_history", { id });
export const searchPages = (query: string) => invoke<SearchResult[]>("page_search", { query });
export const getPageLinks = (id: string) => invoke<PageLink[]>("page_links", { id });
export const getPageBacklinks = (id: string) => invoke<PageLink[]>("page_backlinks", { id });
export const subscribeVaultChanges = (handler: (change: VaultChange) => void): Promise<UnlistenFn> => listen<VaultChange>("vault://changed", (event) => handler(event.payload));
