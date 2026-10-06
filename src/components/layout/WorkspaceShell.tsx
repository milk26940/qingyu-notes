import { useEffect, type ReactNode } from "react";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { Sidebar, type SidebarNav } from "./Sidebar";
import { StatusBar } from "./StatusBar";
import { Topbar } from "./Topbar";
import { PanelResizeHandle } from "./PanelResizeHandle";
import { useViewportWidth } from "./useViewportWidth";

export type PageTab = { pageId: string; title: string };
export function WorkspaceShell({
  children,
  agentPanel,
  activeNav,
  onNavigate,
  onOpenPage,
  tabs = [],
  activeTabId,
  onSelectTab,
  onCloseTab,
  onCloseTabs,
}: {
  children: ReactNode;
  agentPanel: ReactNode;
  activeNav?: SidebarNav;
  onNavigate?: (label: SidebarNav) => void;
  onOpenPage?: (pageId: string) => void;
  tabs?: PageTab[];
  activeTabId?: string | null;
  onSelectTab?: (id: string) => void;
  onCloseTab?: (id: string) => void;
  onCloseTabs?: (ids: string[]) => void;
}) {
  const { sidebarOpen, agentPanelOpen, sidebarWidth, agentPanelWidth, setSidebarWidth, setAgentPanelWidth } = useWorkspaceStore();
  const viewportWidth = useViewportWidth();
  useEffect(() => {
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape" && window.innerWidth < 1180 && useWorkspaceStore.getState().agentPanelOpen) useWorkspaceStore.getState().toggleAgentPanel(); };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, []);
  const agentDrawer = viewportWidth < 1180;
  const sidebarDrawer = viewportWidth < 850;
  const leftWidth = Math.min(sidebarWidth, Math.max(220, viewportWidth * 0.28));
  const rightWidth = Math.min(agentPanelWidth, Math.max(300, viewportWidth - leftWidth - 500));
  const leftInline = sidebarOpen && !sidebarDrawer;
  const rightInline = agentPanelOpen && !agentDrawer;
  const columns = [
    leftInline ? `${leftWidth}px` : "0px",
    leftInline ? "12px" : "0px",
    "minmax(0, 1fr)",
    rightInline ? "12px" : "0px",
    rightInline ? `${rightWidth}px` : "0px",
  ].join(" ");
  return (
    <div className="qy-workspace grid h-screen min-w-0 grid-rows-[56px_minmax(0,1fr)_24px] gap-y-2 overflow-hidden bg-[var(--qy-bg)] px-3 pt-3 text-[var(--qy-ink)]">
      <Topbar
        tabs={tabs}
        activeTabId={activeTabId}
        onSelectTab={onSelectTab}
        onCloseTab={onCloseTab}
        onCloseTabs={onCloseTabs}
      />
      <div className="relative z-0 grid min-h-0" style={{ gridTemplateColumns: columns }}>
        {leftInline ? (
          <Sidebar activeNav={activeNav} onNavigate={onNavigate} onOpenPage={onOpenPage} />
        ) : (
          <div aria-hidden />
        )}
        {leftInline ? <PanelResizeHandle side="left" value={leftWidth} min={220} max={Math.min(420, Math.max(220, viewportWidth - (rightInline ? rightWidth + 24 : 12) - 504))} onResize={setSidebarWidth} /> : <div aria-hidden />}
        <main className="min-w-0 overflow-hidden rounded-[22px] border border-white bg-white shadow-[var(--qy-panel-shadow)]">
          {children}
        </main>
        {rightInline ? <PanelResizeHandle side="right" value={rightWidth} min={300} max={Math.min(560, Math.max(300, viewportWidth - (leftInline ? leftWidth + 24 : 12) - 504))} onResize={setAgentPanelWidth} /> : <div aria-hidden />}
        <div className={agentDrawer ? "qy-agent-drawer" : "qy-agent-slot"} style={{ display: agentPanelOpen ? "flex" : "none", ...(agentDrawer ? { width: Math.min(agentPanelWidth, viewportWidth - 32), zIndex: 91 } : {}) }}>{agentPanel}</div>
        {agentDrawer && agentPanelOpen && <button type="button" aria-label="关闭 Agent 抽屉" className="qy-panel-scrim" onClick={() => useWorkspaceStore.getState().toggleAgentPanel()} />}
        {sidebarDrawer && sidebarOpen && (!agentPanelOpen || !agentDrawer) && <div className="qy-panel-scrim" onClick={() => useWorkspaceStore.getState().toggleSidebar()}><div className="qy-sidebar-drawer" style={{ width: Math.min(sidebarWidth, viewportWidth - 32) }} onClick={(event) => event.stopPropagation()}><Sidebar activeNav={activeNav} onNavigate={onNavigate} onOpenPage={onOpenPage} /></div></div>}
      </div>
      <StatusBar />
    </div>
  );
}
