import { useEffect, useState } from "react";
import { ArrowLeft, BookOpen, Search } from "lucide-react";
import { useWikiStore } from "../../stores/wikiStore";

export function WikiOverview({ onOpen }: {onOpen:(id:string)=>void}) {
  const {items,error,refresh}=useWikiStore();
  const [query,setQuery]=useState("");
  const [selected,setSelected]=useState<string|null>(null);
  useEffect(()=>{void refresh();},[refresh]);
  const current=items.find((item)=>item.pageId===selected);
  const visible=items.filter((item)=>(item.title+" "+item.summary).toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  return <section className="h-full overflow-y-auto p-6 text-[#252b35]" aria-label="知识 Wiki 概览">
    <div className="mx-auto max-w-4xl">
      {current ? <>
        <button className="mb-5 flex items-center gap-2 text-sm font-medium text-[#515b69]" onClick={()=>setSelected(null)}><ArrowLeft size={16}/>返回 Wiki</button>
        <span className="text-xs font-semibold text-[#647084]">{current.kind==="topic"?"主题":"概念"}</span>
        <h1 className="mt-2 text-2xl font-semibold">{current.title}</h1>
        <p className="mt-4 text-base leading-7 text-[#515b69]">{current.summary}</p>
        <button className="mt-5 rounded-xl bg-[#2463eb] px-4 py-2 text-sm font-semibold text-white" onClick={()=>onOpen(current.pageId)}>打开完整内容</button>
        <h2 className="mb-3 mt-8 text-base font-semibold">原始笔记 · {current.sources.length}</h2>
        <div className="space-y-2">{current.sources.map((source)=><button key={source.pageId} className="flex w-full items-center gap-2 rounded-xl border border-[#e0e4eb] bg-white p-3 text-left text-sm hover:bg-[#f4f6f9]" onClick={()=>onOpen(source.pageId)}><BookOpen size={16}/>{source.title}</button>)}</div>
        <h2 className="mb-3 mt-7 text-base font-semibold">共享来源的相关概念</h2>
        <div className="flex flex-wrap gap-2">{items.filter((item)=>item.kind==="concept" && item.pageId!==current.pageId && item.sources.some((s)=>current.sources.some((c)=>c.pageId===s.pageId))).slice(0,24).map((item)=><button className="rounded-xl bg-[#edf1f7] px-3 py-2 text-sm" key={item.pageId} onClick={()=>setSelected(item.pageId)}>{item.title}</button>)}</div>
      </> : <>
        <h1 className="text-2xl font-semibold">知识 Wiki</h1><p className="mt-2 text-sm text-[#596372]">从主题认识知识，从概念连接原始笔记。</p>
        <label className="mt-5 flex items-center gap-2 rounded-2xl border border-[#dce1e9] bg-white px-4 py-3"><Search size={17}/><input aria-label="搜索 Wiki" placeholder="搜索主题、概念或摘要" value={query} onChange={(e)=>setQuery(e.target.value)} className="min-w-0 flex-1 bg-transparent text-sm outline-none"/></label>
        {(["topic","concept"] as const).map((kind)=><section className="mt-7" key={kind}><h2 className="mb-3 text-base font-semibold">{kind==="topic"?"主题":"概念"}<span className="ml-2 text-sm font-normal text-[#647084]">{visible.filter((i)=>i.kind===kind).length}</span></h2><div className="grid grid-cols-1 gap-3 xl:grid-cols-2">{visible.filter((i)=>i.kind===kind).map((item)=><button key={item.pageId} className="rounded-2xl border border-[#dfe4eb] bg-white p-4 text-left transition duration-200 hover:border-[#9ab6e5] hover:shadow-sm active:scale-[0.99]" onClick={()=>setSelected(item.pageId)}><h3 className="font-semibold">{item.title}</h3><p className="mt-2 line-clamp-2 text-sm leading-6 text-[#596372]">{item.summary}</p><p className="mt-3 text-xs font-medium text-[#647084]">{item.sources.length} 篇来源笔记</p></button>)}</div></section>)}
        {!items.length && !error && <p className="mt-8 text-sm text-[#596372]">还没有整理内容，请到知识设置中更新知识。</p>}
      </>}
      {error && <p className="mt-5 text-sm text-[#b42318]">{error}</p>}
    </div>
  </section>;
}
