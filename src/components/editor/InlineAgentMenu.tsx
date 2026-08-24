import { Check, FileText, LoaderCircle, PenLine, Sparkles, X } from "lucide-react";
import { useInlineAgentStore, type InlineAgentCandidate, type InlineAgentContext } from "../../stores/inlineAgentStore";

export type EditorSelection = { from: number; to: number };

export function InlineAgentMenu({ pageId, pageTitle, content, selection, onAccept }: { pageId: string; pageTitle: string; content: string; selection: EditorSelection; onAccept: (candidate: InlineAgentCandidate) => void }) {
  const { candidate, error, request, accept, reject } = useInlineAgentStore();
  const current = candidate?.pageId === pageId ? candidate : null;
  const selected = selection.to > selection.from;
  const context = (): InlineAgentContext => ({ pageId, title: pageTitle, content, from: selection.from, to: selection.to });
  const loading = current?.status === "requesting";
  return <div className="shrink-0 border-b border-[var(--qy-border)]">
    <div className="flex min-h-8 items-center gap-1 py-1">
      <button type="button" className="inline-flex h-7 items-center gap-1 rounded px-2 text-[12px] text-[var(--qy-muted)] hover:bg-[var(--qy-bg)]" disabled={loading} onClick={() => void request("generate", context())}><Sparkles size={14} />AI 生成</button>
      {selected && <><button type="button" className="inline-flex h-7 items-center gap-1 rounded px-2 text-[12px] text-[var(--qy-muted)] hover:bg-[var(--qy-bg)]" disabled={loading} onClick={() => void request("summarize", context())}><FileText size={14} />总结</button><button type="button" className="inline-flex h-7 items-center gap-1 rounded px-2 text-[12px] text-[var(--qy-muted)] hover:bg-[var(--qy-bg)]" disabled={loading} onClick={() => void request("rewrite", context())}><PenLine size={14} />改写</button></>}
      {loading && <span className="ml-1 inline-flex items-center gap-1 text-[11px] text-[var(--qy-faint)]"><LoaderCircle className="animate-spin" size={13} />正在生成候选</span>}
    </div>
    {current?.status === "ready" && <div className="border-t border-[var(--qy-border)] px-2 py-2 text-[12px]">
      <div className="grid max-h-44 grid-cols-2 gap-3 overflow-auto"><div><div className="mb-1 text-[10px] font-semibold text-red-600">原文</div><pre className="whitespace-pre-wrap font-[inherit] text-[var(--qy-muted)]">{current.original || "（插入）"}</pre></div><div><div className="mb-1 text-[10px] font-semibold text-emerald-700">建议</div><pre className="whitespace-pre-wrap font-[inherit] text-[var(--qy-ink)]">{current.proposed}</pre></div></div>
      <div className="mt-2 flex justify-end gap-1"><button type="button" className="inline-flex h-7 items-center gap-1 rounded px-2 text-[11px] text-[var(--qy-muted)] hover:bg-[var(--qy-bg)]" onClick={reject}><X size={13} />拒绝</button><button type="button" className="inline-flex h-7 items-center gap-1 rounded bg-[var(--qy-accent)] px-2 text-[11px] text-white" onClick={() => { const next = accept(); if (next) onAccept(next); }}><Check size={13} />接受</button></div>
    </div>}
    {error && <div className="border-t border-red-100 bg-red-50 px-2 py-1 text-[11px] text-red-700">{error}</div>}
  </div>;
}
