import type { ReactNode } from "react";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { Sidebar } from "./Sidebar";
import { StatusBar } from "./StatusBar";
import { Topbar } from "./Topbar";

export function WorkspaceShell({ children, agentPanel }: { children: ReactNode; agentPanel: ReactNode }) {
  const { sidebarOpen, agentPanelOpen, sidebarWidth, agentPanelWidth } = useWorkspaceStore();
  const columns = [sidebarOpen ? `${sidebarWidth}px` : "0px", "minmax(0, 1fr)", agentPanelOpen ? `${agentPanelWidth}px` : "0px"].join(" ");
  return (
    <div className="grid h-screen min-w-[960px] grid-rows-[40px_minmax(0,1fr)_24px] overflow-hidden bg-[var(--qy-bg)] text-[var(--qy-ink)]">
      <Topbar />
      <div className="grid min-h-0" style={{ gridTemplateColumns: columns }}>
        {sidebarOpen ? <Sidebar /> : <div aria-hidden />}
        <main className="min-w-0 overflow-hidden border-x border-[var(--qy-border)] bg-white">{children}</main>
        {agentPanelOpen ? agentPanel : <div aria-hidden />}
      </div>
      <StatusBar />
    </div>
  );
}
