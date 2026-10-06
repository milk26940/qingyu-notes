import { List, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { PageDocument } from "../../lib/tauri/pages";
import { headingsFromDocument } from "../editor/blockDocument";

export type OutlineHeading = {
  id: string;
  text: string;
  level: number;
  line: number;
  position: number;
};

export function parseMarkdownOutline(markdown: string): OutlineHeading[] {
  const seen = new Map<string, number>();
  let position = 0;
  return markdown.split(/\r?\n/).flatMap((line, lineIndex) => {
    const match = /^(#{1,6})\s+(.+?)\s*$/.exec(line);
    if (!match) return [];
    const text = match[2].replace(/\s+#+\s*$/, "").trim();
    const base =
      text
        .toLowerCase()
        .replace(/[^\p{Letter}\p{Number}]+/gu, "-")
        .replace(/^-|-$/g, "") || `heading-${lineIndex + 1}`;
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    return [
      {
        id: count ? `${base}-${count + 1}` : base,
        text,
        level: match[1].length,
        line: lineIndex + 1,
        position: position++,
      },
    ];
  });
}

export function DocumentOutline({
  markdown,
  document,
  onSelect,
}: {
  markdown: string;
  document?: PageDocument | null;
  onSelect: (heading: OutlineHeading) => void;
}) {
  const headings = useMemo(() => {
    if (document) {
      const structured = headingsFromDocument(document);
      if (structured.length) return structured.map((heading, position) => ({ id: heading.id, text: heading.text, level: heading.level, line: heading.index + 1, position }));
      const legacyMarkdown = document.blocks.filter((block) => block.type === "markdown").map((block) => block.text).join("\n");
      if (legacyMarkdown) return parseMarkdownOutline(legacyMarkdown);
    }
    return parseMarkdownOutline(markdown);
  }, [document, markdown]);
  const [open, setOpen] = useState(false);
  const [activeId, setActiveId] = useState<string | null>(null);
  const panel = useRef<HTMLDivElement>(null);
  const headingIds = headings.map((heading) => heading.id).join("|");
  useEffect(() => {
    setOpen(false); setActiveId(null);
  }, [document?.pageId]);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => { if (!panel.current?.contains(event.target as Node)) setOpen(false); };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    window.addEventListener("pointerdown", close); window.addEventListener("keydown", escape);
    return () => { window.removeEventListener("pointerdown", close); window.removeEventListener("keydown", escape); };
  }, [open]);
  useEffect(() => {
    const scroller = window.document.querySelector<HTMLElement>(".qy-page-scroll");
    if (!scroller) return;
    const observer = new IntersectionObserver((entries) => {
      const visible = entries.filter((entry) => entry.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
      if (visible[0]) setActiveId(visible[0].target.getAttribute("data-block-id"));
    }, { root: scroller, rootMargin: "-5% 0px -75% 0px", threshold: 0 });
    const frame = requestAnimationFrame(() => scroller.querySelectorAll("h1[data-block-id], h2[data-block-id], h3[data-block-id], h4[data-block-id], h5[data-block-id], h6[data-block-id]").forEach((element) => observer.observe(element)));
    return () => { cancelAnimationFrame(frame); observer.disconnect(); };
  }, [headingIds, document?.pageId]);
  return (
    <div
      ref={panel}
      className="pointer-events-none absolute inset-0 z-30"
      aria-label="文档目录"
    >
      <button
        type="button"
        className="pointer-events-auto absolute right-3 top-3 grid h-9 w-9 place-items-center rounded-xl border border-[var(--qy-border)] bg-white text-[var(--qy-muted)] shadow-md hover:text-[var(--qy-ink)]"
        title={open ? "收起目录" : "打开目录"}
        aria-label={open ? "收起目录" : "打开目录"}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        {open ? <X size={15} /> : <List size={15} />}
      </button>
      {open && (
        <section className="pointer-events-auto absolute right-3 top-14 flex max-h-[calc(100%-68px)] w-[min(280px,calc(100%-24px))] flex-col overflow-hidden rounded-[18px] border border-[var(--qy-border)] bg-white shadow-[var(--qy-float-shadow)]">
          <div className="flex h-9 shrink-0 items-center border-b border-[var(--qy-border)] px-3 text-[12px] font-semibold text-[var(--qy-ink)]">
            文档目录
          </div>
          <div className="min-h-0 overflow-y-auto p-2">
            {headings.length ? (
              headings.map((heading) => (
                <button
                  type="button"
                  key={heading.id}
                  aria-current={activeId === heading.id ? "location" : undefined}
                  title={heading.text}
                  onClick={() => { setActiveId(heading.id); onSelect(heading); setOpen(false); }}
                  className={`block w-full truncate rounded px-2 py-1.5 text-left text-[12px] text-[var(--qy-muted)] hover:bg-[var(--qy-bg)] hover:text-[var(--qy-ink)] ${activeId === heading.id ? "bg-[var(--qy-selected)] font-semibold" : ""}`}
                  style={{ paddingLeft: `${8 + (heading.level - 1) * 12}px` }}
                >
                  {heading.text}
                </button>
              ))
            ) : (
              <div className="px-2 py-3 text-[12px] text-[var(--qy-faint)]">
                暂无目录
              </div>
            )}
          </div>
        </section>
      )}
    </div>
  );
}
