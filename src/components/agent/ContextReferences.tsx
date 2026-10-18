import { MessageSquareQuote, X } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import type { ContextReference } from "../../lib/tauri/conversations";
import { useAgentDraftStore } from "../../stores/agentDraftStore";
import { usePagesStore } from "../../stores/pagesStore";

export function ContextReferences({ references, onRemove, onClear }: { references: ContextReference[]; onRemove?: (id: string) => void; onClear?: () => void }) {
  const openPage = usePagesStore((state) => state.open);
  if (!references.length) return null;
  return <div className="qy-reference-list">
    {onClear && <button type="button" className="qy-reference-clear" onClick={onClear}>清空引用（{references.length}）</button>}
    {references.map((reference) => <div key={reference.id} className="qy-reference-card">
      <details><summary>{reference.source === "page" ? "笔记" : "对话"} · {reference.title}<small className="qy-reference-excerpt">{reference.text.slice(0, 100)}</small></summary><pre>{reference.text}</pre></details>
      <div className="qy-reference-actions">{reference.pageId && <button type="button" onClick={() => void openPage(reference.pageId!, reference.blockIds?.[0])}>打开笔记 ↗</button>}{onRemove && <button type="button" aria-label={"移除引用：" + reference.title} onClick={() => onRemove(reference.id)}><X size={12}/></button>}</div>
    </div>)}
  </div>;
}

export function QuotableMessage({ conversationId, messageId, title, children, className }: { conversationId?: string | null; messageId: string; title: string; children: ReactNode; className?: string }) {
  const root = useRef<HTMLDivElement>(null);
  const selectedRange = useRef<Range | null>(null);
  const [quote, setQuote] = useState<{ text: string; from: number; to: number; left: number; top: number } | null>(null);
  useEffect(() => {
    const position = () => {
      const range = selectedRange.current;
      if (!range || !root.current?.contains(range.commonAncestorContainer)) return;
      const rect = range.getBoundingClientRect();
      const bounds = root.current.getBoundingClientRect();
      const viewport = root.current.closest('[data-agent-viewport]')?.getBoundingClientRect();
      if (rect.bottom < Math.max(0, viewport?.top ?? 0) || rect.top > Math.min(window.innerHeight, viewport?.bottom ?? window.innerHeight) || bounds.width === 0) { setQuote((current) => current ? { ...current, top: -1000 } : null); return; }
      setQuote((current) => current ? { ...current, left: Math.max(8, Math.min(rect.left, window.innerWidth - 145)), top: Math.max(8, rect.top - 34) } : null);
    };
    const selectionChanged = () => {
      const selection = window.getSelection();
      if (!selection?.rangeCount || selection.isCollapsed || !root.current) { selectedRange.current = null; setQuote(null); return; }
      const range = selection.getRangeAt(0);
      if (!root.current.contains(range.startContainer) || !root.current.contains(range.endContainer)) { selectedRange.current = null; setQuote(null); return; }
      const text = selection.toString();
      if (!text.trim()) { setQuote(null); return; }
      const before = range.cloneRange(); before.selectNodeContents(root.current); before.setEnd(range.startContainer, range.startOffset);
      const from = before.toString().length;
      selectedRange.current = range.cloneRange();
      setQuote({ text, from, to: from + text.length, left: 0, top: -1000 }); position();
    };
    document.addEventListener("selectionchange", selectionChanged);
    window.addEventListener("scroll", position, true);
    window.addEventListener("resize", position);
    window.visualViewport?.addEventListener("resize", position);
    const observer = new ResizeObserver(position); if (root.current) observer.observe(root.current);
    return () => { document.removeEventListener("selectionchange", selectionChanged); window.removeEventListener("scroll", position, true); window.removeEventListener("resize", position); window.visualViewport?.removeEventListener("resize", position); observer.disconnect(); };
  }, [conversationId, messageId]);
  return <div ref={root} className={className}>{children}{quote && conversationId && createPortal(<button type="button" className="qy-message-quote" style={{ left: quote.left, top: quote.top }} onMouseDown={(event) => event.preventDefault()} onClick={() => useAgentDraftStore.getState().addReferences([{ id: crypto.randomUUID(), source: "conversation", conversationId, messageId, title, text: quote.text, from: quote.from, to: quote.to }])}><MessageSquareQuote size={13}/>添加到对话</button>, document.body)}</div>;
}
