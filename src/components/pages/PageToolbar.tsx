import { Check, History, Link2, Plus, Save, Search, Trash2 } from "lucide-react";
import { usePagesStore } from "../../stores/pagesStore";

export function PageToolbar() {
  const activePage = usePagesStore((state) => state.activePage);
  const saveState = usePagesStore((state) => state.saveState);
  const save = usePagesStore((state) => state.save);
  const create = usePagesStore((state) => state.create);
  const trashPage = usePagesStore((state) => state.trashPage);
  const links = usePagesStore((state) => state.links);
  const backlinks = usePagesStore((state) => state.backlinks);
  const history = usePagesStore((state) => state.history);
  const query = usePagesStore((state) => state.searchQuery);
  const search = usePagesStore((state) => state.search);
  return <div className="flex min-h-12 items-center justify-between border-b border-[var(--qy-border)] px-5">
    <div className="flex items-center gap-1 text-[12px] text-[var(--qy-faint)]"><span>页面</span>{activePage && <><span>/</span><span className="max-w-48 truncate text-[var(--qy-muted)]">{activePage.title}</span></>}</div>
    <div className="flex items-center gap-1">
      <label className="flex h-7 items-center gap-1 rounded-md border border-[var(--qy-border)] px-2 text-[12px] text-[var(--qy-muted)]"><Search size={13} /><input value={query} onChange={(event) => void search(event.target.value)} placeholder="搜索页面" aria-label="搜索页面" className="w-28 bg-transparent outline-none" /></label>
      <button type="button" className="icon-button" title="新建页面" aria-label="新建页面" onClick={() => void create(activePage ? undefined : "未命名页面", activePage?.id ?? null)}><Plus size={15} /></button>
      {activePage && <><button type="button" className="icon-button" title="保存页面" aria-label="保存页面" onClick={() => void save()}><Save size={15} /></button><button type="button" className="icon-button" title="移入回收站" aria-label="移入回收站" onClick={() => void trashPage(activePage.id)}><Trash2 size={15} /></button></>}
      {activePage && <details className="relative"><summary className="icon-button flex cursor-pointer list-none items-center justify-center" title="查看历史" aria-label="查看历史"><History size={15} /></summary><div className="absolute right-0 top-8 z-10 w-72 border border-[var(--qy-border)] bg-white p-2 shadow-lg"><div className="mb-1 text-[11px] font-semibold text-[var(--qy-muted)]">基础历史记录</div>{history.slice(-6).reverse().map((entry) => <div key={entry.id} className="border-t border-[var(--qy-border)] py-1 text-[11px] text-[var(--qy-faint)]"><span>{entry.action}</span><span className="ml-2">{entry.timestamp}</span></div>)}</div></details>}
      <span className="ml-2 flex items-center gap-1 text-[11px] text-[var(--qy-faint)]">{saveState === "saved" ? <><Check size={13} />已保存</> : saveState === "saving" ? "保存中" : saveState === "dirty" ? "未保存" : links.length || backlinks.length ? <><Link2 size={13} />{links.length} 个链接 · {backlinks.length} 个反链</> : ""}</span>
    </div>
  </div>;
}
