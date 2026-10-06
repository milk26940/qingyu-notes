import { Database, FileText, Search } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { usePagesStore } from "../../stores/pagesStore";

export function PageSearch({ onOpen }: { onOpen: (pageId: string) => void }) {
  const results = usePagesStore((state) => state.searchResults);
  const search = usePagesStore((state) => state.search);
  const [query, setQuery] = useState(usePagesStore.getState().searchQuery);
  const error = usePagesStore((state) => state.error);
  const pages = usePagesStore((state) => state.pages);
  const [busy, setBusy] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const timer = useRef<ReturnType<typeof setTimeout>>();
  const sequence = useRef(0);
  const run = async (value: string) => {
    clearTimeout(timer.current);
    const current = ++sequence.current;
    const started = performance.now();
    setBusy(true);
    await search(value);
    if (current === sequence.current) { setBusy(false); setElapsed(Math.round(performance.now() - started)); }
  };
  useEffect(() => {
    setBusy(Boolean(query.trim()));
    // 输入改变立即使旧响应失效，真正的查询仍等防抖结束。
    void search("");
    timer.current = setTimeout(() => { void run(query); }, query.trim() ? 200 : 0);
    return () => { clearTimeout(timer.current); sequence.current++; };
  }, [query]);
  const highlight = (text: string) => {
    const term = query.trim();
    if (!term) return text;
    const escaped = term.replace(/[.*+?^\x24{}()|[\]\\]/g, (character) => "\\" + character);
    return text.split(new RegExp(`(${escaped})`, "gi")).map((part, index) => part.toLocaleLowerCase() === term.toLocaleLowerCase() ? <mark key={index} className="rounded bg-amber-100 px-0.5 text-inherit">{part}</mark> : part);
  };
  return <section className="h-full overflow-y-auto px-8 py-7" aria-label="笔记搜索"><div className="mx-auto w-full max-w-3xl">
    <h1 className="mb-6 text-xl font-bold">搜索</h1>
    <form onSubmit={(event) => { event.preventDefault(); void run(query); }} className="flex h-12 items-center gap-3 rounded-2xl border border-[#d5d8dd] bg-white px-4">
      <Search size={18} /><input autoFocus type="search" aria-label="搜索笔记" placeholder="输入关键词搜索笔记" value={query} onChange={(event) => setQuery(event.target.value)} className="min-w-0 flex-1 bg-transparent text-base outline-none" /><button type="submit" className="rounded-xl px-3 py-1 text-sm font-semibold" aria-busy={busy}>{busy ? "搜索中" : "搜索"}</button>
    </form>
    {error && <p className="mt-3 text-sm text-[#8b4b20]">搜索失败：{error}</p>}
    {query.trim() && <p className="mt-3 text-xs text-[var(--qy-faint)]" aria-live="polite">{busy ? "正在搜索…" : `${results.length} 个结果 · ${elapsed} ms`}</p>}
    {!query.trim() && <div className="mt-5 space-y-1"><p className="mb-2 text-sm text-[var(--qy-faint)]">我的页面</p>{pages.slice(0, 8).map((page) => <button key={page.id} className="flex w-full items-center gap-2 rounded-xl px-3 py-2 text-left hover:bg-[var(--qy-bg)]" onClick={() => onOpen(page.id)}><FileText size={16} />{page.title}</button>)}</div>}
    <div className="mt-5 space-y-2">{(query.trim() ? results : []).map(({ page, snippet }) => {
      const Icon = page.kind === "database" ? Database : FileText;
      return <button key={page.id} className="flex w-full items-start gap-3 rounded-2xl p-4 text-left transition-colors hover:bg-[#eef0f3]" onClick={() => onOpen(page.id)}>
        <Icon size={18} className="mt-1 shrink-0" /><span className="min-w-0 flex-1"><span className="block text-base font-semibold">{highlight(page.title)}</span><span className="mt-1 block text-sm text-[#515965]">{highlight(snippet)}</span></span>
      </button>;
    })}{query.trim() && results.length === 0 && !busy && !error && <p className="py-10 text-center text-sm">没有找到相关笔记</p>}</div>
  </div></section>;
}
