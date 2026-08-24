import { ChevronDown, ChevronRight, FilePlus2, FileText, RotateCcw, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { usePagesStore } from "../../stores/pagesStore";
import type { PageSummary } from "../../lib/tauri/pages";

function TreeNode({ page, childrenByParent, depth = 0 }: { page: PageSummary; childrenByParent: Map<string | null, PageSummary[]>; depth?: number }) {
  const [expanded, setExpanded] = useState(true);
  const children = childrenByParent.get(page.id) ?? [];
  const activeId = usePagesStore((state) => state.activePage?.id);
  const open = usePagesStore((state) => state.open);
  const move = usePagesStore((state) => state.move);
  const create = usePagesStore((state) => state.create);
  return <div>
    <div draggable onDragStart={(event) => event.dataTransfer.setData("text/qingyu-page", page.id)} onDragOver={(event) => event.preventDefault()} onDrop={(event) => { event.preventDefault(); const id = event.dataTransfer.getData("text/qingyu-page"); if (id && id !== page.id) void move(id, page.id); }} className={`group flex h-8 items-center gap-1 rounded-md pr-1 text-[13px] ${activeId === page.id ? "bg-[var(--qy-selected)] text-[var(--qy-ink)]" : "text-[var(--qy-muted)] hover:bg-white"}`} style={{ paddingLeft: `${8 + depth * 16}px` }}>
      <button type="button" className="grid h-5 w-5 place-items-center" aria-label={expanded ? `收起${page.title}` : `展开${page.title}`} onClick={() => setExpanded((value) => !value)}>{children.length ? (expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />) : null}</button>
      <button type="button" className="flex min-w-0 flex-1 items-center gap-2 truncate text-left" onClick={() => void open(page.id)}><FileText size={14} /><span className="truncate">{page.title}</span></button>
      <button type="button" className="icon-button h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 focus:opacity-100" title="新建子页面" aria-label={`在${page.title}下新建子页面`} onClick={() => void create("未命名页面", page.id)}><FilePlus2 size={13} /></button>
    </div>
    {expanded && children.map((child) => <TreeNode key={child.id} page={child} childrenByParent={childrenByParent} depth={depth + 1} />)}
  </div>;
}

export function PageTree() {
  const pages = usePagesStore((state) => state.pages);
  const trash = usePagesStore((state) => state.trash);
  const create = usePagesStore((state) => state.create);
  const restore = usePagesStore((state) => state.restore);
  const [showTrash, setShowTrash] = useState(false);
  const childrenByParent = useMemo(() => pages.reduce((map, page) => { const list = map.get(page.parentId) ?? []; list.push(page); map.set(page.parentId, list); return map; }, new Map<string | null, PageSummary[]>()), [pages]);
  return <div className="mt-2">
    <div className="mb-1 flex items-center justify-between px-2 text-[11px] font-semibold uppercase tracking-wider text-[var(--qy-faint)]"><span>我的页面</span><button type="button" className="icon-button h-6 w-6" title="新建页面" aria-label="新建页面" onClick={() => void create()}><FilePlus2 size={14} /></button></div>
    <div>{(childrenByParent.get(null) ?? []).map((page) => <TreeNode key={page.id} page={page} childrenByParent={childrenByParent} />)}</div>
    <button type="button" className="mt-3 flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[12px] text-[var(--qy-muted)] hover:bg-white" onClick={() => setShowTrash((value) => !value)}><Trash2 size={14} />回收站</button>
    {showTrash && <div className="mt-1 space-y-1">{trash.map((page) => <div key={page.id} className="flex items-center justify-between px-2 text-[12px] text-[var(--qy-faint)]"><span className="truncate">{page.title}</span><button type="button" className="icon-button h-6 w-6" title="恢复页面" aria-label={`恢复${page.title}`} onClick={() => void restore(page.id)}><RotateCcw size={13} /></button></div>)}</div>}
  </div>;
}
