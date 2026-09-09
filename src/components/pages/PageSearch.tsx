import { Database, FileText, Search } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { usePagesStore } from "../../stores/pagesStore";
import { semanticSearch } from "../../lib/tauri/semantic";
import { useRef, useState } from "react";

type SourceLink = { pageId: string; title: string; link: string };
type Hit = { id: string; pageId: string; title: string; snippet: string; blockId?: string; kind?: string; source: string; sources?: SourceLink[] };
type KnowledgeSearch = {
  queryTerms: string[];
  keyword: { page: { id: string; title: string; kind: string }; snippet: string; matchedTerm: string; link: string }[];
  wiki: { pageId: string; title: string; summary: string; matchedTerms: string[]; link: string; sources: SourceLink[] }[];
  relations: { sourceId: string; pageId: string; pageTitle: string; source: string; target: string; relation: string; evidence: string; link: string }[];
};
export function PageSearch({ onOpen }: { onOpen: (pageId: string) => void }) {
  const results = usePagesStore((state) => state.searchResults);
  const search = usePagesStore((state) => state.search);
  const [query, setQuery] = useState(usePagesStore.getState().searchQuery);
  const [mode, setMode] = useState<"exact" | "semantic" | "knowledge">("exact");
  const [hits, setHits] = useState<Hit[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const sequence = useRef(0);
  const run = async () => {
    const current = ++sequence.current;
    if (!query.trim()) { setHits([]); return; }
    setBusy(true); setError("");
    try {
      if (mode === "exact") { await search(query); return; }
      const combined: Hit[] = [];
      const notices: string[] = [];
      if (mode === "knowledge") {
        try {
          const knowledge = await invoke<KnowledgeSearch>("knowledge_search", { query });
          combined.push(...knowledge.keyword.map(({ page, snippet, matchedTerm }) => ({ id: page.id, pageId: page.id, title: page.title, snippet, kind: page.kind, source: `关键词 · ${matchedTerm}` })));
          combined.push(...knowledge.wiki.map((item) => ({ id: item.pageId, pageId: item.pageId, title: item.title, snippet: item.summary, kind: "wiki", source: `知识 Wiki · ${item.matchedTerms.join("、")}`, sources: item.sources })));
          combined.push(...knowledge.relations.map((r) => ({ id: r.sourceId + r.relation + r.target, pageId: r.pageId, title: r.source + " · " + r.relation + " · " + r.target, snippet: r.evidence, source: `关系证据 · ${r.pageTitle}` })));
        } catch (e) { notices.push(message(e)); }
      }
      try { combined.push(...(await semanticSearch(query)).map((r, index) => ({ id: r.pageId + r.blockId + index, pageId: r.pageId, title: r.pageTitle, snippet: r.snippet, blockId: r.blockId, source: "语义片段" }))); }
      catch (e) { notices.push(message(e)); }
      if (current === sequence.current) { setHits(combined); setError(notices.join("；")); }
    } finally { if (current === sequence.current) setBusy(false); }
  };
  const displayed: Hit[] = mode === "exact" ? results.map(({ page, snippet }) => ({ id: page.id, pageId: page.id, title: page.title, snippet, kind: page.kind, source: "关键词" })) : hits;
  return <section className="h-full overflow-y-auto px-8 py-7" aria-label="笔记搜索"><div className="mx-auto w-full max-w-3xl">
    <h1 className="mb-6 text-xl font-bold">搜索</h1>
    <div className="mb-3 flex gap-2">{(["exact", "semantic", "knowledge"] as const).map((value) => <button key={value} className={"rounded-xl px-3 py-2 text-sm font-semibold " + (mode === value ? "bg-[var(--qy-selected)]" : "hover:bg-[#eef0f3]")} onClick={() => { ++sequence.current; setBusy(false); setMode(value); setHits([]); setError(""); }}>{value === "exact" ? "精确匹配" : value === "semantic" ? "语义搜索" : "知识搜索"}</button>)}</div>
    <form onSubmit={(e) => { e.preventDefault(); void run(); }} className="flex h-12 items-center gap-3 rounded-2xl border border-[#d5d8dd] bg-white px-4">
      <Search size={18} /><input autoFocus type="search" aria-label="搜索笔记" placeholder="输入问题，按回车搜索" value={query} onChange={(e) => { setQuery(e.target.value); if (mode === "exact") void search(e.target.value); }} className="min-w-0 flex-1 bg-transparent text-base outline-none" /><button type="submit" disabled={busy} className="rounded-xl px-3 py-1 text-sm font-semibold">{busy ? "检索中" : "搜索"}</button>
    </form>
    {error && <p className="mt-3 text-sm text-[#8b4b20]">部分检索不可用：{error}</p>}
    <div className="mt-5 space-y-2">{displayed.map((hit) => {
      const Icon = hit.kind === "database" ? Database : FileText;
      return <div key={hit.source + hit.id} className="rounded-2xl transition-colors hover:bg-[#eef0f3]">
        <button className="flex w-full items-start gap-3 p-4 text-left" onClick={() => { onOpen(hit.pageId); if (hit.blockId) void usePagesStore.getState().open(hit.pageId, hit.blockId); }}>
          <Icon size={18} className="mt-1 shrink-0" /><span className="min-w-0 flex-1"><span className="block text-base font-semibold">{hit.title}</span><span className="mt-1 block text-sm text-[#515965]">{hit.snippet}</span><span className="mt-2 block text-xs text-[#515965]">{hit.source}</span></span>
        </button>
        {hit.sources?.length ? <div className="flex flex-wrap gap-2 px-4 pb-4 pl-11"><span className="py-1 text-xs font-semibold text-[#515965]">原始来源</span>{hit.sources.map((source) => <button key={source.pageId} type="button" className="rounded-lg bg-white px-2 py-1 text-xs font-semibold text-[#1769d2] shadow-sm hover:underline" onClick={() => onOpen(source.pageId)}>{source.title}</button>)}</div> : null}
      </div>;
    })}{query.trim() && !busy && displayed.length === 0 && <p className="py-10 text-center text-sm">没有找到相关笔记</p>}</div>
  </div></section>;
}
function message(error: unknown) { return error && typeof error === "object" && "message" in error ? String(error.message) : String(error); }
