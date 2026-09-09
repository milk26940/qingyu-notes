import type { ReactNode } from "react";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { Sidebar, type SidebarNav } from "./Sidebar";
import { StatusBar } from "./StatusBar";
import { Topbar } from "./Topbar";
import { PanelResizeHandle } from "./PanelResizeHandle";

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
  const columns = [
    sidebarOpen ? `${sidebarWidth}px` : "0px",
    sidebarOpen ? "12px" : "0px",
    "minmax(0, 1fr)",
    agentPanelOpen ? "12px" : "0px",
    agentPanelOpen ? `${agentPanelWidth}px` : "0px",
  ].join(" ");
  return (
    <div className="grid h-screen min-w-0 grid-rows-[56px_minmax(0,1fr)_24px] gap-y-2 overflow-hidden bg-[var(--qy-bg)] px-3 pt-3 text-[var(--qy-ink)]">
      <Topbar
        tabs={tabs}
        activeTabId={activeTabId}
        onSelectTab={onSelectTab}
        onCloseTab={onCloseTab}
        onCloseTabs={onCloseTabs}
      />
      <div className="relative z-0 grid min-h-0" style={{ gridTemplateColumns: columns }}>
        {sidebarOpen ? (
          <Sidebar activeNav={activeNav} onNavigate={onNavigate} onOpenPage={onOpenPage} />
        ) : (
          <div aria-hidden />
        )}
        {sidebarOpen ? <PanelResizeHandle side="left" value={sidebarWidth} min={220} max={420} onResize={setSidebarWidth} /> : <div aria-hidden />}
        <main className="min-w-0 overflow-hidden rounded-[22px] border border-white bg-white shadow-[var(--qy-panel-shadow)]">
          {children}
        </main>
        {agentPanelOpen ? <PanelResizeHandle side="right" value={agentPanelWidth} min={300} max={560} onResize={setAgentPanelWidth} /> : <div aria-hidden />}
        {agentPanelOpen ? agentPanel : <div aria-hidden />}
      </div>
      <StatusBar />
    </div>
  );
}
