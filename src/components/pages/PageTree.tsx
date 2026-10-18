import { ChevronDown, ChevronRight, Copy, Database, FilePlus2, FileText, Trash2 } from "lucide-react";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { usePagesStore } from "../../stores/pagesStore";
import type { PageSummary } from "../../lib/tauri/pages";
import { useDatabaseStore } from "../../stores/databaseStore";
import { DeleteConfirmDialog, countDescendants } from "./DeleteConfirmDialog";

import { useSessionStore } from "../../stores/sessionStore";

const TreeNode = memo(function TreeNode({ page, childrenByParent, onOpen, depth = 0, onRequestDelete }: { page: PageSummary; childrenByParent: Map<string | null, PageSummary[]>; onOpen?: (pageId: string) => void; depth?: number; onRequestDelete?: (page: PageSummary) => void }) {
  const expanded = useSessionStore((state) => !state.collapsedIds.includes(page.id));
  const toggleNode = useSessionStore((state) => state.toggleNode);
  const setExpanded = (value: boolean | ((current: boolean) => boolean)) => { const next = typeof value === "function" ? value(expanded) : value; if (next !== expanded) toggleNode(page.id); };
  const [copyNotice, setCopyNotice] = useState("");
  const copyId = async () => {
    try { await navigator.clipboard.writeText(page.id); setCopyNotice("页面 ID 已复制"); setMenu(null); }
    catch { setCopyNotice("复制失败，请重试"); }
  };
  const children = childrenByParent.get(page.id) ?? [];
  const isActive = usePagesStore((state) => (state.openingPageId ?? state.activePage?.id) === page.id);
  const open = usePagesStore((state) => state.open);
  const prefetch = usePagesStore((state) => state.prefetch);
  const move = usePagesStore((state) => state.move);
  const createLinkedChild = usePagesStore((state) => state.createLinkedChild);
  const create = usePagesStore((state) => state.create);
  const addRecord = useDatabaseStore((state) => state.addRecord);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menu) return;
    const close = (event: PointerEvent) => { if (!menuRef.current?.contains(event.target as Node)) setMenu(null); };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") setMenu(null); };
    document.addEventListener("keydown", escape);
    document.addEventListener("pointerdown", close);
    return () => { document.removeEventListener("pointerdown", close); document.removeEventListener("keydown", escape); };
  }, [menu]);
  const createChild = async (kind: "page" | "database") => {
    const { activePage, activeDocument } = usePagesStore.getState();
    setMenu(null);
    if (!createLinkedChild && kind === "page") {
      await create("未命名页面", page.id);
      return;
    }
    if (activePage?.id !== page.id) await open(page.id);
    if (createLinkedChild) {
      const getState = (usePagesStore as typeof usePagesStore & { getState?: () => { activeDocument: typeof activeDocument } }).getState;
      const currentDocument = getState?.().activeDocument ?? activeDocument;
      await createLinkedChild(kind, currentDocument?.pageId === page.id ? currentDocument.blocks.length : 0);
    }
  };
  const createEntry = async () => { setMenu(null); await useDatabaseStore.getState().open(page.id); const record = await addRecord(); if (record) usePagesStore.getState().open(record.id); };
  const removePage = () => {
    setMenu(null);
    onRequestDelete?.(page);
  };
  const siblings = childrenByParent.get(page.parentId) ?? [];
  const handleDrop = (event: React.DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.stopPropagation();
    const id = event.dataTransfer.getData("text/qingyu-page");
    if (!id || id === page.id) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const position = bounds.height > 0 ? (event.clientY - bounds.top) / bounds.height : 0.5;
    if (position >= 0.25 && position <= 0.75) {
      void move(id, page.id, children.length);
      return;
    }
    const targetIndex = siblings.findIndex((sibling) => sibling.id === page.id);
    const sourceIndex = siblings.findIndex((sibling) => sibling.id === id);
    let insertionIndex = targetIndex + (position > 0.75 ? 1 : 0);
    if (sourceIndex >= 0 && sourceIndex < insertionIndex) insertionIndex -= 1;
    void move(id, page.parentId, insertionIndex);
  };
  return <div className="qy-tree-node min-w-0 w-full" onClick={(event) => {
    const target = event.target as HTMLElement;
    if (!target.closest("button") && target.closest("[draggable]") === event.currentTarget.firstElementChild) {
      if (onOpen) onOpen(page.id); else void open(page.id);
      event.stopPropagation();
    }
  }}>
    <div role="treeitem" aria-expanded={children.length ? expanded : undefined} data-page-id={page.id} draggable onKeyDown={(event) => {
      if (event.nativeEvent.isComposing || (event.target as HTMLElement).closest("[data-tree-menu]")) return;
      const button = event.currentTarget.querySelector<HTMLButtonElement>("[data-tree-open]");
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const buttons = Array.from(event.currentTarget.closest('[role="tree"]')?.querySelectorAll<HTMLButtonElement>("[data-tree-open]") ?? []);
        const index = buttons.indexOf(button!);
        buttons[index + (event.key === "ArrowDown" ? 1 : -1)]?.focus();
      } else if (event.key === "ArrowRight" && children.length) { event.preventDefault(); setExpanded(true); }
      else if (event.key === "ArrowLeft") { event.preventDefault(); if (expanded && children.length) setExpanded(false); else event.currentTarget.closest('[role="tree"]')?.querySelector<HTMLButtonElement>(`[data-page-id="${page.parentId}"] [data-tree-open]`)?.focus(); }
      else if (event.key === "F2") { event.preventDefault(); void open(page.id).then(() => { onOpen?.(page.id); requestAnimationFrame(() => { const title = document.querySelector<HTMLInputElement>('[aria-label="页面标题"]'); title?.focus(); title?.select(); }); }); }
    }} onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); setMenu({ x: event.clientX, y: event.clientY }); }} onDragStart={(event) => event.dataTransfer.setData("text/qingyu-page", page.id)} onDragOver={(event) => event.preventDefault()} onDrop={handleDrop} className={`qy-tree-row group relative flex h-9 min-w-0 w-full cursor-pointer items-center gap-1 rounded-xl pr-1 text-[14px] font-semibold ${isActive ? "bg-[var(--qy-selected)] text-[var(--qy-ink)] shadow-sm" : "text-[var(--qy-muted)] hover:bg-white"}`} style={{ paddingLeft: `${8 + depth * 16}px` }}>
      <button type="button" className="grid h-5 w-5 shrink-0 place-items-center" aria-label={expanded ? `收起${page.title}` : `展开${page.title}`} onClick={() => setExpanded((value) => !value)}>{children.length ? (expanded ? <ChevronDown size={15} strokeWidth={2} /> : <ChevronRight size={15} strokeWidth={2} />) : null}</button>
      <button type="button" data-tree-open title={page.title} className="flex min-w-0 flex-1 items-center gap-2 truncate text-left" onMouseEnter={() => prefetch(page.id)} onClick={() => onOpen ? onOpen(page.id) : void open(page.id)}>{page.kind === "database" ? <Database size={15} strokeWidth={2}/> : <FileText size={15} strokeWidth={2}/>}<span className="min-w-0 flex-1 truncate">{page.title}</span></button>
      {menu && <div data-tree-menu ref={menuRef} className="fixed z-50 min-w-44 rounded-2xl border border-[var(--qy-border)] bg-white p-1.5 text-[12px] shadow-[var(--qy-float-shadow)]" style={{ left: Math.min(menu.x, window.innerWidth - 200), top: Math.max(8, Math.min(menu.y, window.innerHeight - 160)) }} onClick={(event) => event.stopPropagation()}>{page.kind === "database" ? <button type="button" className="flex w-full items-center gap-2 rounded-xl px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]" onClick={() => void createEntry()}><FilePlus2 size={14}/>新建条目页面</button> : <><button type="button" className="flex w-full items-center gap-2 rounded-xl px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]" onClick={() => void createChild("page")}><FilePlus2 size={14}/>新建子页面</button><button type="button" className="flex w-full items-center gap-2 rounded-xl px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]" onClick={() => void createChild("database")}><Database size={13}/>新建数据库</button></>}<button type="button" className="flex w-full items-center gap-2 rounded-xl px-2 py-1.5 text-left text-[#b42318] hover:bg-[#fef3f2]" onClick={removePage}><Trash2 size={14}/>递归删除</button><button type="button" className="flex w-full items-center gap-2 rounded-xl px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]" onClick={() => void copyId()}><Copy size={14}/>复制页面 ID</button></div>}
    </div>
    {copyNotice && <span role="status" className="qy-tree-copy-notice" onAnimationEnd={() => setCopyNotice("")}>{copyNotice}</span>}
    {expanded && children.map((child) => <TreeNode key={child.id} page={child} childrenByParent={childrenByParent} onOpen={onOpen} depth={depth + 1} onRequestDelete={onRequestDelete} />)}
  </div>;
});

export function PageTree({ onOpen }: { onOpen?: (pageId: string) => void }) {
  const pages = usePagesStore((state) => state.pages);
  const create = usePagesStore((state) => state.create);
  const move = usePagesStore((state) => state.move);
  const trashPageRecursive = usePagesStore((state) => state.trashPageRecursive);
  const [deleteTarget, setDeleteTarget] = useState<PageSummary | null>(null);
  const childrenByParent = useMemo(() => pages.reduce((map, page) => { const list = map.get(page.parentId) ?? []; list.push(page); map.set(page.parentId, list); return map; }, new Map<string | null, PageSummary[]>()), [pages]);
  return <div className="mt-2">
    <div className="mb-1 flex items-center justify-between px-2 text-[12px] font-semibold uppercase tracking-wider text-[var(--qy-faint)]"><span>我的页面</span><button type="button" className="icon-button h-6 w-6" title="新建页面" aria-label="新建页面" onClick={() => void create()}><FilePlus2 size={15} strokeWidth={2} /></button></div>
    <div role="tree" aria-label="移动到根目录" className="min-h-8" onDragOver={(event) => event.preventDefault()} onDrop={(event) => { event.preventDefault(); const id = event.dataTransfer.getData("text/qingyu-page"); if (id) void move(id, null); }}>{(childrenByParent.get(null) ?? []).map((page) => <TreeNode key={page.id} page={page} childrenByParent={childrenByParent} onOpen={onOpen} onRequestDelete={setDeleteTarget} />)}</div>
    {deleteTarget && (
      <DeleteConfirmDialog
        open={Boolean(deleteTarget)}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
        title={deleteTarget.title}
        kind={deleteTarget.kind}
        descendantCount={countDescendants(deleteTarget.id, pages)}
        onConfirm={async () => {
          await trashPageRecursive(deleteTarget.id);
          setDeleteTarget(null);
        }}
      />
    )}
  </div>;
}
