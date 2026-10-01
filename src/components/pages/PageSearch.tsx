import { Database, FileText, Search } from "lucide-react";
import { useState } from "react";
import { usePagesStore } from "../../stores/pagesStore";

export function PageSearch({ onOpen }: { onOpen: (pageId: string) => void }) {
  const results = usePagesStore((state) => state.searchResults);
  const search = usePagesStore((state) => state.search);
  const [query, setQuery] = useState(usePagesStore.getState().searchQuery);
  const [error, setError] = useState("");
  const run = async (value: string) => {
    setError("");
    try { await search(value); }
    catch (error) { setError(error instanceof Error ? error.message : String(error)); }
  };
  return <section className="h-full overflow-y-auto px-8 py-7" aria-label="笔记搜索"><div className="mx-auto w-full max-w-3xl">
    <h1 className="mb-6 text-xl font-bold">搜索</h1>
    <form onSubmit={(event) => { event.preventDefault(); void run(query); }} className="flex h-12 items-center gap-3 rounded-2xl border border-[#d5d8dd] bg-white px-4">
      <Search size={18} /><input autoFocus type="search" aria-label="搜索笔记" placeholder="输入关键词搜索笔记" value={query} onChange={(event) => { setQuery(event.target.value); void run(event.target.value); }} className="min-w-0 flex-1 bg-transparent text-base outline-none" /><button type="submit" className="rounded-xl px-3 py-1 text-sm font-semibold">搜索</button>
    </form>
    {error && <p className="mt-3 text-sm text-[#8b4b20]">搜索失败：{error}</p>}
    <div className="mt-5 space-y-2">{results.map(({ page, snippet }) => {
      const Icon = page.kind === "database" ? Database : FileText;
      return <button key={page.id} className="flex w-full items-start gap-3 rounded-2xl p-4 text-left transition-colors hover:bg-[#eef0f3]" onClick={() => onOpen(page.id)}>
        <Icon size={18} className="mt-1 shrink-0" /><span className="min-w-0 flex-1"><span className="block text-base font-semibold">{page.title}</span><span className="mt-1 block text-sm text-[#515965]">{snippet}</span></span>
      </button>;
    })}{query.trim() && results.length === 0 && !error && <p className="py-10 text-center text-sm">没有找到相关笔记</p>}</div>
  </div></section>;
}
