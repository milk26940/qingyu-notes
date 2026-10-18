import { FileText, Search } from "lucide-react";
import { PageTree } from "../pages/PageTree";
import { useEffect, useRef, useState } from "react";

import { useSessionStore } from "../../stores/sessionStore";
import { usePagesStore } from "../../stores/pagesStore";

const items = [
  [Search, "搜索"],
  [FileText, "页面"],
] as const;

export type SidebarNav = "搜索" | "页面";

export function Sidebar({
  activeNav,
  onNavigate,
  onOpenPage,
}: {
  activeNav?: SidebarNav;
  onNavigate?: (label: SidebarNav) => void;
  onOpenPage?: (pageId: string) => void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const sessionVault = useSessionStore((state) => state.vault);
  useEffect(() => { if (scroller.current) scroller.current.scrollTop = useSessionStore.getState().treeScroll; }, [sessionVault]);
  const [selectedNav, setSelectedNav] = useState<SidebarNav>("页面");
  const currentNav = activeNav ?? selectedNav;
  return (
    <aside
      className="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-[22px] border border-white bg-[var(--qy-sidebar)] px-3 py-4 shadow-[var(--qy-panel-shadow)]"
      aria-label="工作区导航"
    >
      <div className="mb-5 h-7" aria-hidden="true" />
      <nav className="space-y-0.5">
        {items.map(([Icon, label]) => (
          <button
            key={label}
            type="button"
            aria-current={currentNav === label ? "page" : undefined}
            className={`flex h-10 w-full items-center gap-2 rounded-xl px-3 text-left text-[14px] font-semibold hover:bg-white hover:text-[var(--qy-ink)] ${currentNav === label ? "bg-white text-[var(--qy-ink)] shadow-sm" : "text-[var(--qy-muted)]"}`}
            onClick={() => {
              setSelectedNav(label);
              onNavigate?.(label);
            }}
          >
            <Icon size={16} strokeWidth={2} aria-hidden="true" />
            <span>{label}</span>
          </button>
        ))}
      </nav>
      <div ref={scroller} onScroll={(event) => { const session = useSessionStore.getState(); if (session.vault === usePagesStore.getState().readyVaultPath) session.setTreeScroll(event.currentTarget.scrollTop); }} aria-label="页面树滚动区域" className="min-h-0 flex-1 overflow-y-auto overscroll-contain pr-1">
        <PageTree onOpen={onOpenPage} />
      </div>
    </aside>
  );
}
