import { PanelLeft, PanelRight, Search } from "lucide-react";
import { useWorkspaceStore } from "../../stores/workspaceStore";

export function Topbar() {
  const { toggleSidebar, toggleAgentPanel } = useWorkspaceStore();
  return (
    <header className="flex items-center justify-between border-b border-[var(--qy-border)] bg-white px-2">
      <div className="flex items-center gap-1">
        <button className="icon-button" title="切换侧栏" aria-label="切换侧栏" onClick={toggleSidebar}><PanelLeft size={16} /></button>
        <span className="px-2 text-[13px] text-[var(--qy-muted)]">我的笔记库</span>
      </div>
      <div className="flex items-center gap-1">
        <button className="icon-button" title="搜索" aria-label="搜索"><Search size={16} /></button>
        <button className="icon-button" title="切换 Agent 面板" aria-label="切换 Agent 面板" onClick={toggleAgentPanel}><PanelRight size={16} /></button>
      </div>
    </header>
  );
}
