import { WorkspaceShell } from "../components/layout/WorkspaceShell";
const AgentPanel = lazy(() => import("../components/agent/AgentPanel").then((module) => ({ default: module.AgentPanel })));
import { BlockEditor } from "../components/editor/BlockEditor";
import { lazy, Suspense, useCallback, useEffect } from "react";
import { usePagesStore } from "../stores/pagesStore";
import {
  DatabaseRecordProperties,
  DatabaseTable,
} from "../components/database/DatabaseTable";
import {
  subscribeProjectionStatus,
  subscribeVaultChanges,
} from "../lib/tauri/pages";
import { useDatabaseStore } from "../stores/databaseStore";
import { useAppearanceStore } from "../stores/appearanceStore";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { parseQingyuPageLink } from "../lib/internalLinks";
import { useCodexStore } from "../stores/codexStore";
import { useAgentStore } from "../stores/agentStore";
import { useAgentDraftStore } from "../stores/agentDraftStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { useState, useRef } from "react";
import { readSession, saveSession, useSessionStore } from "../stores/sessionStore";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import type { SidebarNav } from "../components/layout/Sidebar";
import { DocumentOutline } from "../components/pages/DocumentOutline";
import type { PageTab } from "../components/layout/WorkspaceShell";
import { Feather } from "lucide-react";
import { PageSearch } from "../components/pages/PageSearch";

function App() {
  const {
    activePage,
    openingPageId,
    activeDocument,
    targetBlockId,
    pages,
    vaultPath,
    readyVaultPath,
    init,
    editDocument,
    editTitle,
    createEditorChild,
    save,
    open,
    error,
    saveState,
    discardConflictAndReload,
    keepConflictAsCopy,
  } = usePagesStore();
  const applyFontScale = useAppearanceStore((state) => state.applyFontScale);
  const codexStatus = useCodexStore((state) => state.status);
  const connectCodex = useCodexStore((state) => state.connect);
  const subscribeCodex = useCodexStore((state) => state.subscribe);
  const initializeAgent = useAgentStore((state) => state.initialize);
  const [activeNav, setActiveNav] = useState<SidebarNav>("页面");
  const [tabs, setTabs] = useState<PageTab[]>([]);
  const [restoredVault, setRestoredVault] = useState<string | null>(null);
  const sessionSnapshot = useRef<() => void>(() => undefined);
  const restoringVault = useRef<string | null>(null);
  const activeConversationId = useAgentStore((state) => state.activeId);
  useEffect(() => { useAgentDraftStore.getState().setTargetConversation(activeConversationId); }, [activeConversationId]);
  useEffect(() => {
    useAgentDraftStore.getState().clearReferences();
    useAgentDraftStore.getState().setTargetConversation(null);
    useAgentStore.setState({ activeId: null, records: [], conversations: [], queue: {}, queuePaused: {}, sending: false, error: null });
  }, [vaultPath]);
  const [showLoading, setShowLoading] = useState(false);
  useEffect(() => {
    setShowLoading(false);
    if (!openingPageId) return;
    const timer = setTimeout(() => setShowLoading(true), 120);
    return () => clearTimeout(timer);
  }, [openingPageId]);
  useEffect(() => {
    if (!activePage) return;
    setTabs((current) =>
      current.some((tab) => tab.pageId === activePage.id)
        ? current.map((tab) =>
            tab.pageId === activePage.id
              ? { ...tab, title: activePage.title }
              : tab,
          )
        : [...current, { pageId: activePage.id, title: activePage.title }],
    );
  }, [activePage]);
  const openPage = useCallback((id: string, blockId?: string | null) => {
    setActiveNav("页面");
    void open(id, blockId);
  }, [open]);
  const closeTabs = async (ids: string[]) => {
    if (!(await usePagesStore.getState().flush())) return;
    const closing = new Set(ids);
    setTabs((current) => {
      const activeIndex = current.findIndex(
        (tab) => tab.pageId === activePage?.id,
      );
      const next = current.filter((tab) => !closing.has(tab.pageId));
      if (activePage && closing.has(activePage.id)) {
        const replacement =
          current
            .slice(activeIndex + 1)
            .find((tab) => !closing.has(tab.pageId)) ??
          current
            .slice(0, activeIndex)
            .reverse()
            .find((tab) => !closing.has(tab.pageId));
        if (replacement) void openPage(replacement.pageId);
        else
          void save().then(() =>
            usePagesStore.setState({
              activePage: null,
              activeDocument: null,
              persistedPage: null,
              targetBlockId: null,
              saveState: "idle",
            }),
          );
      }
      return next;
    });
  };
  const closeTab = (id: string) => closeTabs([id]);
  const handleNavigate = (label: SidebarNav) => {
    setActiveNav(label);
    if (label === "页面" && activePage?.kind !== "page") {
      const firstPage = pages.find((page) => page.kind === "page");
      if (firstPage) void open(firstPage.id);
    }
  };
  useEffect(() => { void init(); }, [init]);
  useEffect(() => {
    if (!vaultPath || readyVaultPath !== vaultPath || restoredVault === vaultPath || restoringVault.current === vaultPath) return;
    restoringVault.current = vaultPath;
    const saved = readSession(vaultPath);
    useSessionStore.getState().bind(vaultPath);
    const existing = new Map(pages.map((page) => [page.id, page]));
    setTabs((saved.tabs ?? []).filter((tab) => existing.has(tab.pageId)).map((tab) => ({ ...tab, title: existing.get(tab.pageId)!.title })));
    const workspace = useWorkspaceStore.getState();
    useWorkspaceStore.setState({ sidebarOpen: saved.sidebarOpen ?? workspace.sidebarOpen, agentPanelOpen: saved.agentPanelOpen ?? workspace.agentPanelOpen });
    if (saved.sidebarWidth) workspace.setSidebarWidth(saved.sidebarWidth);
    if (saved.agentPanelWidth) workspace.setAgentPanelWidth(saved.agentPanelWidth);
    if (saved.fontScale) useAppearanceStore.getState().setFontScale(saved.fontScale);
    const restorePage = saved.activePageId && existing.has(saved.activePageId) ? open(saved.activePageId) : Promise.resolve();
    void restorePage.finally(() => { if (usePagesStore.getState().vaultPath === vaultPath) setRestoredVault(vaultPath); restoringVault.current = null; });
    if (saved.window && saved.window.width > 0 && saved.window.height > 0 && "__TAURI_INTERNALS__" in window) {
      const view = getCurrentWindow();
      // 忽略旧版本记录的零尺寸，恢复时遵守主窗口最小尺寸。
      void view.setSize(new LogicalSize(Math.max(960, saved.window.width), Math.max(640, saved.window.height))).then(() => saved.window?.maximized ? view.maximize() : undefined).catch(() => undefined);
    }
  }, [vaultPath, readyVaultPath, pages, restoredVault, open]);
  useEffect(() => {
    if (!vaultPath || restoredVault !== vaultPath) return;
    const snapshot = () => {
      if (usePagesStore.getState().vaultPath !== vaultPath || usePagesStore.getState().readyVaultPath !== vaultPath) return;
      const view = useWorkspaceStore.getState();
      saveSession(vaultPath, { tabs, activePageId: usePagesStore.getState().activePage?.id ?? null, sidebarOpen: view.sidebarOpen, sidebarWidth: view.sidebarWidth, agentPanelOpen: view.agentPanelOpen, agentPanelWidth: view.agentPanelWidth, fontScale: useAppearanceStore.getState().fontScale });
    };
    sessionSnapshot.current = snapshot;
    snapshot();
    const offWorkspace = useWorkspaceStore.subscribe(snapshot);
    const offAppearance = useAppearanceStore.subscribe(snapshot);
    return () => { offWorkspace(); offAppearance(); };
  }, [tabs, activePage?.id, vaultPath, restoredVault]);
  useEffect(() => {
    if (!vaultPath || restoredVault !== vaultPath || !("__TAURI_INTERNALS__" in window)) return;
    const view = getCurrentWindow();
    let disposed = false, closing = false;
    let offClose: (() => void) | undefined, offResize: (() => void) | undefined;
    let normal = readSession(vaultPath).window;
    const rememberWindow = async () => {
      // 最小化时内容区为零，不能覆盖上次正常窗口大小。
      if (await view.isMinimized()) return;
      const maximized = await view.isMaximized();
      if (!maximized) {
        const size = await view.innerSize(); const scale = await view.scaleFactor();
        if (size.width <= 0 || size.height <= 0) return;
        normal = { width: size.width / scale, height: size.height / scale, maximized };
      }
      if (normal) saveSession(vaultPath, { window: { ...normal, maximized } });
    };
    void rememberWindow();
    void view.onResized(() => { void rememberWindow(); }).then((off) => { if (disposed) off(); else offResize = off; });
    void view.onCloseRequested((event) => {
      event.preventDefault();
      if (closing) return;
      closing = true;
      void (async () => {
        window.dispatchEvent(new Event("qingyu:remember-position"));
        sessionSnapshot.current();
        await rememberWindow();
        if (!(await usePagesStore.getState().flush())) {
          usePagesStore.getState().reportError("关闭前未能保存正文，请先处理保存错误或版本冲突，再关闭窗口。");
          closing = false; return;
        }
        await view.destroy();
      })().catch((failure) => { closing = false; usePagesStore.getState().reportError(String(failure)); });
    }).then((off) => { if (disposed) off(); else offClose = off; });
    return () => { disposed = true; offClose?.(); offResize?.(); };
  }, [vaultPath, restoredVault]);
  useEffect(() => {
    const handleSave = (event: KeyboardEvent) => {
      if (event.isComposing) return;
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault(); setActiveNav("搜索");
        requestAnimationFrame(() => document.querySelector<HTMLInputElement>('[aria-label="搜索笔记"]')?.focus());
      }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void save();
      }
    };
    window.addEventListener("keydown", handleSave);
    return () => window.removeEventListener("keydown", handleSave);
  }, [save]);
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (usePagesStore.getState().saveState === "dirty")
        void usePagesStore.getState().save();
    }, 180_000);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeCodex()
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      })
      .catch(() => undefined);
    void connectCodex();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [connectCodex, subscribeCodex]);
  useEffect(() => {
    if (!vaultPath || codexStatus.state !== "ready") return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void initializeAgent()
      .then((value) => {
        if (disposed) value();
        else cleanup = value;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      cleanup?.();
    };
  }, [codexStatus.state, initializeAgent, vaultPath]);
  useEffect(() => {
    applyFontScale();
    const handleFontShortcut = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey || event.isComposing) return;
      const appearance = useAppearanceStore.getState();
      if (event.key === "+" || event.key === "=") appearance.increaseFont();
      else if (event.key === "-") appearance.decreaseFont();
      else if (event.key === "0") appearance.resetFont();
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", handleFontShortcut);
    return () => window.removeEventListener("keydown", handleFontShortcut);
  }, [applyFontScale]);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeVaultChanges((change) => {
      void usePagesStore.getState().handleExternalChange(change);
      void useDatabaseStore.getState().refreshExternalChange();
    })
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeProjectionStatus((status) =>
      usePagesStore.getState().setProjectionStatus(status),
    )
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  useEffect(() => {
    if (!vaultPath) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const handleUrls = (urls: string[]) => {
      for (const value of urls) {
        const link = parseQingyuPageLink(value);
        if (link) {
          void usePagesStore.getState().open(link.pageId, link.blockId);
          break;
        }
      }
    };
    void getCurrent()
      .then((urls) => {
        if (urls) handleUrls(urls);
      })
      .catch(() => undefined);
    void onOpenUrl(handleUrls)
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [vaultPath]);
  const rightPanel = (
    <Suspense fallback={<aside aria-label="Agent 面板" className="grid h-full w-full place-items-center rounded-[22px] bg-[var(--qy-sidebar)] text-sm text-[var(--qy-faint)]">加载 Agent…</aside>}><AgentPanel /></Suspense>
  );
  return (
    <WorkspaceShell
      activeNav={activeNav}
      onNavigate={handleNavigate}
      onOpenPage={openPage}
      tabs={tabs}
      activeTabId={activePage?.id}
      onSelectTab={openPage}
      onCloseTab={closeTab}
      onCloseTabs={closeTabs}
      agentPanel={rightPanel}
    >
      <div className="relative flex h-full flex-col">
        {showLoading && openingPageId && <div className="qy-page-loading" role="status" aria-label="加载页面"><span /><span /><span /><p className="text-sm text-[var(--qy-faint)]">加载页面…</p></div>}
        {error && (
          <div className="flex items-center gap-2 border-b border-red-200 bg-red-50 px-5 py-2 text-[12px] text-red-700">
            <span className="min-w-0 flex-1 truncate">{error}</span>
            {saveState === "conflict" && (
              <>
                <button
                  type="button"
                  className="shrink-0 hover:underline"
                  onClick={() => void keepConflictAsCopy()}
                >
                  保留为副本
                </button>
                <button
                  type="button"
                  className="shrink-0 font-medium hover:underline"
                  onClick={() => void discardConflictAndReload()}
                >
                  丢弃本地并重载
                </button>
              </>
            )}
          </div>
        )}
        {activeNav === "搜索" ? (
          <PageSearch onOpen={openPage} />
        ) : activePage?.kind === "database" ? (
          <DatabaseTable databaseId={activePage.id} />
        ) : activePage ? (
          <div className="relative min-h-0 flex-1 overflow-hidden">
            <DocumentOutline
              markdown={activePage.content}
              document={activeDocument}
              onSelect={(heading) => {
                const target = document.querySelector(`[data-block-id="${CSS.escape(heading.id)}"]`)
                  ?? document.querySelectorAll(".qy-block-editor h1, .qy-block-editor h2, .qy-block-editor h3, .qy-block-editor h4, .qy-block-editor h5, .qy-block-editor h6")[heading.position];
                target?.scrollIntoView({ behavior: "smooth", block: "center" });
              }}
            />
            <div className="qy-page-scroll h-full overflow-y-auto px-5 py-5">
              <input
                aria-label="页面标题"
                value={activePage.title}
                onChange={(event) => editTitle(event.target.value)}
                className="mb-5 w-full border-0 pr-12 text-2xl font-semibold outline-none"
              />
              {activePage.kind === "databaseRecord" && (
                <DatabaseRecordProperties record={activePage} />
              )}
              {activeDocument && <BlockEditor
                key={activePage.id}
                document={activeDocument}
                pages={pages}
                targetBlockId={targetBlockId}
                onChange={editDocument}
                onSelectionAction={(reference) => { useAgentDraftStore.getState().addReferences([reference]); if (!useWorkspaceStore.getState().agentPanelOpen) useWorkspaceStore.getState().toggleAgentPanel(); }}
                onCreateChild={createEditorChild}
                onOpenPage={openPage}
                pageKind={activePage.kind}
              />}
            </div>
          </div>
        ) : (
          <div className="relative grid flex-1 place-items-center text-center">
            <DocumentOutline markdown="" onSelect={() => undefined} />
            <div className="flex flex-col items-center">
              <div aria-label="轻羽羽毛标志" className="mb-6 grid h-20 w-24 place-items-center rounded-[22px] border border-[#c9cbd0] bg-[#f7f7f5] text-[#55585e] shadow-[var(--qy-float-shadow)]">
                <Feather size={42} strokeWidth={1.15} aria-hidden="true" />
              </div>
              <h1 className="text-2xl font-bold text-[var(--qy-ink)]">选择或新建一个页面</h1>
              <p className="mt-3 text-sm font-medium text-[var(--qy-muted)]">
                从左侧页面树打开笔记
              </p>
            </div>
          </div>
        )}
      </div>
    </WorkspaceShell>
  );
}

export default App;
