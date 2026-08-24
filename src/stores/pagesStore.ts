import { create } from "zustand";
import * as api from "../lib/tauri/pages";
import type { Page, PageHistoryEntry, PageLink, PageSummary, SearchResult, VaultChange } from "../lib/tauri/pages";

type SaveState = "idle" | "dirty" | "saving" | "saved" | "error" | "conflict";
type PagesStore = {
  vaultPath: string | null;
  pages: PageSummary[];
  trash: PageSummary[];
  activePage: Page | null;
  links: PageLink[];
  backlinks: PageLink[];
  history: PageHistoryEntry[];
  searchResults: SearchResult[];
  searchQuery: string;
  saveState: SaveState;
  error: string | null;
  reportError: (message: string) => void;
  init: (path?: string) => Promise<void>;
  refresh: () => Promise<void>;
  open: (id: string) => Promise<void>;
  create: (title?: string, parentId?: string | null) => Promise<void>;
  edit: (content: string) => void;
  editTitle: (title: string) => void;
  save: () => Promise<void>;
  move: (id: string, parentId: string | null, sortOrder?: number) => Promise<void>;
  trashPage: (id: string) => Promise<void>;
  restore: (id: string) => Promise<void>;
  search: (query: string) => Promise<void>;
  handleExternalChange: (change: VaultChange) => Promise<void>;
};

const failure = (error: unknown) => (typeof error === "object" && error && "message" in error ? String(error.message) : String(error));

export const usePagesStore = create<PagesStore>((set, get) => ({
  vaultPath: null, pages: [], trash: [], activePage: null, links: [], backlinks: [], history: [], searchResults: [], searchQuery: "", saveState: "idle", error: null,
  reportError: (error) => set({ error }),
  init: async (path) => { try { const vaultPath = await api.initVault(path); set({ vaultPath, error: null }); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  refresh: async () => { try { const [pages, trash] = await Promise.all([api.listPages(), api.listPages(true)]); set({ pages: pages.filter((page) => !page.trashedAt), trash: trash.filter((page) => page.trashedAt), error: null }); } catch (error) { set({ error: failure(error) }); } },
  open: async (id) => { try { const [activePage, links, backlinks, history] = await Promise.all([api.getPage(id), api.getPageLinks(id), api.getPageBacklinks(id), api.getPageHistory(id)]); set({ activePage, links, backlinks, history, saveState: "idle", error: null }); } catch (error) { set({ error: failure(error) }); } },
  create: async (title = "未命名页面", parentId = null) => { try { const page = await api.createPage(title, parentId, ""); set({ activePage: page, saveState: "saved", error: null }); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  edit: (content) => set((state) => state.activePage ? { activePage: { ...state.activePage, content }, saveState: state.saveState === "conflict" ? "conflict" : "dirty" } : state),
  editTitle: (title) => set((state) => state.activePage ? { activePage: { ...state.activePage, title }, saveState: state.saveState === "conflict" ? "conflict" : "dirty" } : state),
  save: async () => { const page = get().activePage; if (!page || get().saveState === "saving") return; if (get().saveState === "conflict") { set({ error: "Agent 已修改磁盘版本，请重新打开页面后再编辑" }); return; } set({ saveState: "saving" }); try { const activePage = await api.updatePage(page.id, page.title, page.content); const [links, backlinks, history] = await Promise.all([api.getPageLinks(activePage.id), api.getPageBacklinks(activePage.id), api.getPageHistory(activePage.id)]); set({ activePage, links, backlinks, history, saveState: "saved", error: null }); await get().refresh(); } catch (error) { set({ saveState: "error", error: failure(error) }); } },
  move: async (id, parentId, sortOrder = 0) => { try { await api.movePage(id, parentId, sortOrder); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  trashPage: async (id) => { try { await api.trashPage(id); set({ activePage: null }); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  restore: async (id) => { try { await api.restorePage(id); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  search: async (query) => { set({ searchQuery: query }); if (!query.trim()) { set({ searchResults: [] }); return; } try { set({ searchResults: await api.searchPages(query), error: null }); } catch (error) { set({ error: failure(error) }); } },
  handleExternalChange: async (_change) => { const active = get().activePage; await get().refresh(); if (!active) return; if (get().saveState === "dirty" || get().saveState === "conflict") { set({ saveState: "conflict", error: "Agent 已修改笔记库；当前未保存内容未被覆盖，请重新打开页面核对" }); return; } try { await api.getPage(active.id); await get().open(active.id); } catch { set({ activePage: null, saveState: "idle" }); } },
}));
