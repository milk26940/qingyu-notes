import { Archive, Database, FileText, Inbox, Search, Star, Trash2 } from "lucide-react";
import { PageTree } from "../pages/PageTree";

const items = [
  [Search, "搜索"], [Inbox, "收件箱"], [Star, "收藏"], [FileText, "页面"], [Database, "数据库"], [Trash2, "回收站"], [Archive, "归档"],
] as const;

export function Sidebar() {
  return (
    <aside className="min-h-0 overflow-y-auto bg-[var(--qy-sidebar)] px-2 py-3" aria-label="工作区导航">
      <div className="mb-5 px-2 text-[15px] font-semibold tracking-normal">轻羽</div>
      <nav className="space-y-0.5">
        {items.map(([Icon, label]) => (
          <button key={label} type="button" className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] text-[var(--qy-muted)] transition-colors hover:bg-white hover:text-[var(--qy-ink)]" disabled={label !== "页面"}>
            <Icon size={15} strokeWidth={1.8} aria-hidden="true" />
            <span>{label}</span>
          </button>
        ))}
      </nav>
      <PageTree />
    </aside>
  );
}
