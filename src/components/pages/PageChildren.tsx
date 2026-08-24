import { FileText, Plus } from "lucide-react";
import { usePagesStore } from "../../stores/pagesStore";

export function PageChildren({ pageId }: { pageId: string }) {
  const pages = usePagesStore((state) => state.pages);
  const create = usePagesStore((state) => state.create);
  const open = usePagesStore((state) => state.open);
  const children = pages.filter((page) => page.parentId === pageId);
  return <div className="mb-4 border-y border-[var(--qy-border)] py-2">
    {children.map((page) => <button key={page.id} type="button" onClick={() => void open(page.id)} className="flex h-8 w-full items-center gap-2 rounded px-2 text-left text-[13px] text-[var(--qy-muted)] hover:bg-[var(--qy-bg)] hover:text-[var(--qy-ink)]"><FileText size={14} /><span className="truncate">{page.title}</span></button>)}
    <button type="button" onClick={() => void create("未命名页面", pageId)} className="flex h-8 items-center gap-2 rounded px-2 text-[12px] text-[var(--qy-faint)] hover:bg-[var(--qy-bg)] hover:text-[var(--qy-muted)]"><Plus size={14} />新建子页面</button>
  </div>;
}
