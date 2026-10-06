import { create } from "zustand";
import * as api from "../lib/tauri/pages";
import * as databaseApi from "../lib/tauri/databases";
import type { Page, PageDocument, PageHistoryEntry, PageLink, PageSummary, ProjectionStatus, SearchResult, VaultChange } from "../lib/tauri/pages";

type SaveState = "idle" | "dirty" | "saving" | "saved" | "error" | "conflict";
type PagesStore = {
  vaultPath: string | null;
  pages: PageSummary[];
  trash: PageSummary[];
  activePage: Page | null;
  openingPageId: string | null;
  activeDocument: PageDocument | null;
  persistedPage: Page | null;
  targetBlockId: string | null;
  links: PageLink[];
  backlinks: PageLink[];
  history: PageHistoryEntry[];
  searchResults: SearchResult[];
  searchQuery: string;
  saveState: SaveState;
  error: string | null;
  projectionStatus: ProjectionStatus;
  setProjectionStatus: (status: ProjectionStatus) => void;
  reportError: (message: string) => void;
  init: (path?: string) => Promise<void>;
  refresh: () => Promise<void>;
  open: (id: string, targetBlockId?: string | null) => Promise<void>;
  reload: (id: string) => Promise<void>;
  prefetch: (id: string) => void;
  create: (title?: string, parentId?: string | null) => Promise<void>;
  createLinkedChild: (kind: "page" | "database", position: number) => Promise<void>;
  edit: (content: string) => void;
  editDocument: (document: PageDocument) => void;
  editTitle: (title: string) => void;
  save: () => Promise<void>;
  move: (id: string, parentId: string | null, sortOrder?: number) => Promise<void>;
  trashPage: (id: string) => Promise<void>;
  trashPageRecursive: (id: string) => Promise<void>;
  restore: (id: string) => Promise<void>;
  purge: (id: string) => Promise<void>;
  search: (query: string) => Promise<void>;
  handleExternalChange: (change: VaultChange) => Promise<void>;
  discardConflictAndReload: () => Promise<void>;
  keepConflictAsCopy: () => Promise<void>;
};

const failure = (error: unknown) => (typeof error === "object" && error && "message" in error ? String(error.message) : String(error));
let autoSaveTimer: ReturnType<typeof setTimeout> | null = null;
let saveInFlight: Promise<void> | null = null;
let openSequence = 0;
type PageBundle = Awaited<ReturnType<typeof api.openPageBundle>>;
const pageBundleCache = new Map<string, { bundle: PageBundle; timestamp: number }>();
const pendingBundles = new Map<string, Promise<PageBundle>>();
const CACHE_LIMIT = 24;
const CACHE_TTL = 30_000;
let cacheGeneration = 0;
let searchSequence = 0;

const invalidateBundles = () => { cacheGeneration++; pageBundleCache.clear(); pendingBundles.clear(); };
const cacheBundle = (id: string, bundle: PageBundle) => {
  pageBundleCache.delete(id);
  pageBundleCache.set(id, { bundle, timestamp: Date.now() });
  if (pageBundleCache.size > CACHE_LIMIT) pageBundleCache.delete(pageBundleCache.keys().next().value!);
};
const cachedBundle = (id: string) => {
  const cached = pageBundleCache.get(id);
  if (!cached || Date.now() - cached.timestamp > CACHE_TTL) { pageBundleCache.delete(id); return null; }
  pageBundleCache.delete(id);
  pageBundleCache.set(id, cached);
  return cached.bundle;
};
const fetchBundle = (id: string) => {
  const pending = pendingBundles.get(id);
  if (pending) return pending;
  const generation = cacheGeneration;
  const request = api.openPageBundle(id).then((bundle) => {
    if (generation === cacheGeneration) cacheBundle(id, bundle);
    return bundle;
  }).finally(() => { if (pendingBundles.get(id) === request) pendingBundles.delete(id); });
  pendingBundles.set(id, request);
  return request;
};

const cancelAutoSave = () => {
  if (autoSaveTimer != null) clearTimeout(autoSaveTimer);
  autoSaveTimer = null;
};

const scheduleAutoSave = (save: () => Promise<void>) => {
  cancelAutoSave();
  autoSaveTimer = setTimeout(() => {
    autoSaveTimer = null;
    void save();
  }, 180_000);
};

const flushPendingSave = async (get: () => PagesStore) => {
  cancelAutoSave();
  if (saveInFlight) await saveInFlight;
  if (get().saveState === "dirty") await get().save();
  return !["dirty", "saving", "error", "conflict"].includes(get().saveState);
};

export const usePagesStore = create<PagesStore>((set, get) => ({
  openingPageId: null,
  vaultPath: null, pages: [], trash: [], activePage: null, activeDocument: null, persistedPage: null, targetBlockId: null, links: [], backlinks: [], history: [], searchResults: [], searchQuery: "", saveState: "idle", error: null, projectionStatus: { state: "initializing" },
  setProjectionStatus: (projectionStatus) => set({ projectionStatus }),
  reportError: (error) => set({ error }),
  init: async (path) => {
    const currentPath = get().vaultPath;
    if (currentPath && path && path !== currentPath && !(await flushPendingSave(get))) return;
    try {
      set({ projectionStatus: { state: "initializing" } });
      const vaultPath = await api.initVault(path);
      invalidateBundles();
      openSequence++;
      searchSequence++;
      set({ vaultPath, pages: [], trash: [], activePage: null, activeDocument: null, persistedPage: null, targetBlockId: null, links: [], backlinks: [], history: [], searchResults: [], searchQuery: "", saveState: "idle", error: null, projectionStatus: { state: "ready" } });
      await get().refresh();
    } catch (error) {
      set({ error: failure(error) });
    }
  },
  refresh: async () => { invalidateBundles(); try { const [pages, trash] = await Promise.all([api.listPages(), api.listPages(true)]); set({ pages: pages.filter((page) => !page.trashedAt), trash: trash.filter((page) => page.trashedAt), error: null }); } catch (error) { set({ error: failure(error) }); } },
  prefetch: (id) => {
    if (cachedBundle(id) || get().activePage?.id === id) return;
    void fetchBundle(id).catch(() => undefined);
  },
  open: async (id, targetBlockId = null) => {
    const sequence = ++openSequence;
    if (get().activePage?.id === id && !targetBlockId) { set({ openingPageId: null }); return; }
    set({ openingPageId: id });
    if (!(await flushPendingSave(get))) { if (sequence === openSequence) set({ openingPageId: null }); return; }
    if (sequence !== openSequence) return;
    try {
      const cached = cachedBundle(id);
      if (cached) {
        set({ activePage: cached.page, activeDocument: cached.document, persistedPage: cached.page, targetBlockId, links: cached.links, backlinks: cached.backlinks, history: [], saveState: "idle", error: null });
        const localDocument = get().activeDocument;
        const generation = cacheGeneration;
        void fetchBundle(id).then((bundle) => {
          if (sequence !== openSequence || generation !== cacheGeneration || get().activeDocument !== localDocument || ["dirty", "saving", "conflict"].includes(get().saveState)) return;
          if (bundle.document.revision !== cached.document.revision) set({ activePage: bundle.page, activeDocument: bundle.document, persistedPage: bundle.page, links: bundle.links, backlinks: bundle.backlinks });
        }).catch(() => undefined);
        return;
      }
      const bundle = await fetchBundle(id);
      if (sequence !== openSequence) return;
      set({ activePage: bundle.page, activeDocument: bundle.document, persistedPage: bundle.page, targetBlockId, links: bundle.links, backlinks: bundle.backlinks, history: [], saveState: "idle", error: null });
    } catch (error) { if (sequence === openSequence) set({ error: failure(error) }); }
    finally { if (sequence === openSequence) set({ openingPageId: null }); }
  },
  reload: async (id) => {
    if (get().saveState === "dirty" || get().saveState === "saving" || get().saveState === "conflict") return;
    const localDocument = get().activeDocument;
    try {
      invalidateBundles();
      const bundle = await fetchBundle(id);
      if (get().activePage?.id !== id || get().activeDocument !== localDocument || ["dirty", "saving", "conflict"].includes(get().saveState)) return;
      set({ activePage: bundle.page, activeDocument: bundle.document, persistedPage: bundle.page, targetBlockId: null, links: bundle.links, backlinks: bundle.backlinks, saveState: "saved", error: null });
      pageBundleCache.delete(id);
      await get().refresh();
    } catch (error) { set({ error: failure(error) }); }
  },
  create: async (title = "未命名页面", parentId = null) => { if (!(await flushPendingSave(get))) return; try { const page = await api.createPage(title, parentId, ""); const bundle = await api.openPageBundle(page.id); set({ activePage: bundle.page, activeDocument: bundle.document, persistedPage: bundle.page, links: bundle.links, backlinks: bundle.backlinks, saveState: "saved", error: null }); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  createLinkedChild: async (kind, position) => {
    if (!(await flushPendingSave(get))) return;
    const parent = get().activePage;
    if (!parent || parent.kind === "database") return;
    let child: Page | null = null;
    try {
      child = kind === "database" ? (await databaseApi.createDatabase("未命名数据库", parent.id)).page : await api.createPage("未命名页面", parent.id, "");
      const parentDocument = get().activeDocument;
      if (!parentDocument || parentDocument.pageId !== parent.id) throw new Error("当前页面缺少区块文档");
      const linkBlock = {
        id: crypto.randomUUID(), type: "paragraph", text: child.title, children: [],
        attrs: { node: { type: "paragraph", content: [{ type: "text", text: child.title, marks: [{ type: "link", attrs: { href: `qingyu://page/${child.id}` } }, { type: "bold" }] }] } },
      };
      const insertAt = Math.max(0, Math.min(position, parentDocument.blocks.length));
      const blocks = [...parentDocument.blocks];
      blocks.splice(insertAt, 0, linkBlock);
      await api.savePageDocument({ ...parentDocument, blocks }, parentDocument.revision);
      const bundle = await api.openPageBundle(child.id);
      set({ activePage: bundle.page, activeDocument: bundle.document, persistedPage: bundle.page, links: bundle.links, backlinks: bundle.backlinks, saveState: "saved", error: null });
      await get().refresh();
    } catch (error) {
      if (child) { try { await api.trashPage(child.id); } catch { /* 回滚失败由原始错误统一提示。 */ } }
      set({ error: failure(error) });
    }
  },
  edit: (content) => { set((state) => state.activePage ? { activePage: { ...state.activePage, content }, saveState: state.saveState === "conflict" ? "conflict" : "dirty" } : state); if (get().saveState === "dirty") scheduleAutoSave(get().save); },
  editDocument: (document) => { set((state) => state.activePage ? { activeDocument: document, activePage: { ...state.activePage, content: document.blocks.map((block) => block.text).filter(Boolean).join("\n") }, saveState: state.saveState === "conflict" ? "conflict" : "dirty" } : state); if (get().saveState === "dirty") scheduleAutoSave(get().save); },
  editTitle: (title) => { set((state) => state.activePage ? { activePage: { ...state.activePage, title }, activeDocument: state.activeDocument ? { ...state.activeDocument, title } : null, saveState: state.saveState === "conflict" ? "conflict" : "dirty" } : state); if (get().saveState === "dirty") scheduleAutoSave(get().save); },
  save: () => {
    cancelAutoSave();
    if (saveInFlight) return saveInFlight;
    const page = get().activePage;
    const document = get().activeDocument;
    if (!page) return Promise.resolve();
    if (get().saveState === "conflict") { set({ error: "Agent 已修改磁盘版本，请重新打开页面后再编辑" }); return Promise.resolve(); }
    const task = (async () => {
      set({ saveState: "saving" });
      try {
        if (document && document.pageId === page.id) {
          const savedDocument = await api.savePageDocument(document, document.revision);
          invalidateBundles();
          const current = get().activeDocument;
          if (current?.pageId === document.pageId && current === document) {
            set({ activeDocument: savedDocument, persistedPage: { ...page, content: savedDocument.blocks.map((block) => block.text).filter(Boolean).join("\n") }, saveState: "saved", error: null });
          } else if (current?.pageId === document.pageId) {
            set({ activeDocument: { ...current, revision: savedDocument.revision }, saveState: "dirty", error: null });
            scheduleAutoSave(get().save);
          }
          set((state) => ({ pages: state.pages.map((entry) => entry.id === page.id && entry.title !== savedDocument.title ? { ...entry, title: savedDocument.title } : entry) }));
          return;
        }
        const persisted = get().persistedPage;
        if (!persisted || persisted.id !== page.id) throw new Error("缺少页面保存基线，请重新打开页面");
        const savedPage = await api.updatePage(page.id, page.title, page.content, persisted.title, persisted.content);
        invalidateBundles();
        const [links, backlinks] = await Promise.all([api.getPageLinks(savedPage.id), api.getPageBacklinks(savedPage.id)]);
        const current = get().activePage;
        const unchanged = current?.id === page.id && current.title === page.title && current.content === page.content;
        if (unchanged) set({ activePage: savedPage, persistedPage: savedPage, links, backlinks, saveState: "saved", error: null });
        else if (current?.id === page.id) { set({ persistedPage: savedPage, links, backlinks, saveState: "dirty", error: null }); scheduleAutoSave(get().save); }
        set((state) => ({ pages: state.pages.map((entry) => entry.id === savedPage.id && entry.title !== savedPage.title ? { ...entry, title: savedPage.title } : entry) }));
      } catch (error) {
        const code = typeof error === "object" && error && "code" in error ? String(error.code) : "";
        set({ saveState: code === "page_write_conflict" ? "conflict" : "error", error: failure(error) });
      }
    })();
    saveInFlight = task.finally(() => { saveInFlight = null; });
    return saveInFlight;
  },
  move: async (id, parentId, sortOrder = 0) => { if (!(await flushPendingSave(get))) return; try { await api.movePage(id, parentId, sortOrder); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  trashPage: async (id) => { if (!(await flushPendingSave(get))) return; try { await api.trashPage(id); set({ activePage: null, activeDocument: null, persistedPage: null }); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  trashPageRecursive: async (id) => { if (!(await flushPendingSave(get))) return; try { await api.trashPageRecursive(id); set({ activePage: null, activeDocument: null, persistedPage: null, targetBlockId: null }); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  restore: async (id) => { try { await api.restorePage(id); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  purge: async (id) => { try { await api.purgePage(id); await get().refresh(); } catch (error) { set({ error: failure(error) }); } },
  search: async (query) => { const sequence = ++searchSequence; set({ searchQuery: query }); if (!query.trim()) { set({ searchResults: [] }); return; } try { const results = await api.searchPages(query); if (sequence === searchSequence) set({ searchResults: results, error: null }); } catch (error) { if (sequence === searchSequence) set({ error: failure(error) }); } },
  handleExternalChange: async (change) => {
    invalidateBundles();
    const active = get().activePage;
    await get().refresh();
    if (!active) return;
    if (change.pageIds.length > 0 && !change.pageIds.includes(active.id)) return;
    if (get().saveState === "dirty" || get().saveState === "conflict") {
      try {
        const diskPage = await api.getPage(active.id);
        const persisted = get().persistedPage;
        if (persisted && diskPage.title === persisted.title && diskPage.content === persisted.content) return;
      } catch {
        // 页面在磁盘上被删除时也需要保留编辑缓冲。
      }
      cancelAutoSave();
      set({ saveState: "conflict", error: "磁盘版本已发生变化；当前未保存内容已保留，请重新打开页面核对" });
      return;
    }
    try { await api.getPage(active.id); await get().reload(active.id); } catch { set({ activePage: null, activeDocument: null, persistedPage: null, saveState: "idle" }); }
  },
  discardConflictAndReload: async () => {
    const active = get().activePage;
    if (!active || get().saveState !== "conflict") return;
    cancelAutoSave();
    try {
      const [diskPage, bundle, history] = await Promise.all([
        api.getPage(active.id),
        api.openPageBundle(active.id),
        api.getPageHistory(active.id),
      ]);
      const validBundle = bundle.page.id === diskPage.id;
      set({ activePage: diskPage, activeDocument: validBundle ? bundle.document : null, persistedPage: diskPage, links: validBundle ? bundle.links : [], backlinks: validBundle ? bundle.backlinks : [], history, saveState: "idle", error: null });
    } catch (error) {
      set({ error: failure(error) });
    }
  },
  keepConflictAsCopy: async () => {
    const active = get().activePage;
    if (!active || get().saveState !== "conflict") return;
    cancelAutoSave();
    try {
      const copy = await api.createPage(`${active.title} (本地副本)`, active.parentId, active.content);
      const bundle = await api.openPageBundle(copy.id);
      const validBundle = bundle.page.id === copy.id;
      set({ activePage: copy, activeDocument: validBundle ? bundle.document : null, persistedPage: copy, links: validBundle ? bundle.links : [], backlinks: validBundle ? bundle.backlinks : [], history: [], saveState: "saved", error: null });
      await get().refresh();
    } catch (error) {
      set({ error: failure(error) });
    }
  },
}));
