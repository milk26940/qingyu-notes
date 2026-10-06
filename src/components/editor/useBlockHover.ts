import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useRef, useState, type MouseEvent, type RefObject } from "react";

type Hover = { index: number; top: number; type: string; visible: boolean };
type Bounds = { element: HTMLElement; top: number; bottom: number };

export function useBlockHover(shell: RefObject<HTMLDivElement>, editor: Editor | null, locked: boolean) {
  const [hover, setHover] = useState<Hover>({ index: 0, top: 0, type: "paragraph", visible: false });
  const frame = useRef(0);
  const point = useRef<number | null>(null);
  const geometry = useRef<{ dirty: boolean; bounds: Bounds[]; shellTop: number; lines: Map<HTMLElement, DOMRect[]> }>({ dirty: true, bounds: [], shellTop: 0, lines: new Map() });
  const measure = useCallback(() => {
    frame.current = 0;
    if (locked || point.current === null || !shell.current || !editor) return;
    const cached = geometry.current;
    if (cached.dirty) {
      cached.shellTop = shell.current.getBoundingClientRect().top;
      cached.bounds = Array.from(editor.view.dom.children).map((element) => {
        const rect = element.getBoundingClientRect();
        return { element: element as HTMLElement, top: rect.top, bottom: rect.bottom };
      });
      cached.lines.clear();
      cached.dirty = false;
    }
    const y = point.current;
    const blocks = cached.bounds;
    if (!blocks.length) return;
    let low = 0, high = blocks.length - 1;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if (blocks[mid].bottom < y) low = mid + 1;
      else high = mid;
    }
    let index = low;
    if (index > 0 && y < blocks[index].top && y - blocks[index - 1].bottom < blocks[index].top - y) index--;
    const block = blocks[index];
    let lines = cached.lines.get(block.element);
    if (!lines) {
      lines = [];
      const walker = window.document.createTreeWalker(block.element, NodeFilter.SHOW_TEXT);
      while (walker.nextNode()) {
        const range = window.document.createRange();
        range.selectNodeContents(walker.currentNode);
        lines.push(...Array.from(range.getClientRects()).filter((rect) => rect.width && rect.height));
      }
      cached.lines.set(block.element, lines);
    }
    let top = block.top, distance = Infinity;
    for (const line of lines) {
      const nextDistance = y >= line.top && y <= line.bottom ? 0 : Math.min(Math.abs(y - line.top), Math.abs(y - line.bottom));
      if (nextDistance < distance) { top = line.top; distance = nextDistance; }
      if (distance === 0) break;
    }
    const type = /^H[1-6]$/.test(block.element.tagName) ? "heading" : block.element.tagName.toLowerCase();
    const offset = Math.round(top - cached.shellTop);
    setHover((current) => current.visible && current.index === index && current.top === offset && current.type === type ? current : { index, top: offset, type, visible: true });
  }, [editor, locked, shell]);
  const queue = useCallback(() => { if (!frame.current) frame.current = requestAnimationFrame(measure); }, [measure]);
  useEffect(() => {
    if (!editor || !shell.current) return;
    const invalidate = () => { geometry.current.dirty = true; if (point.current !== null) queue(); };
    const resize = new ResizeObserver(invalidate);
    resize.observe(editor.view.dom);
    const mutation = new MutationObserver(invalidate);
    mutation.observe(editor.view.dom, { subtree: true, childList: true, characterData: true });
    window.addEventListener("scroll", invalidate, true);
    window.addEventListener("resize", invalidate);
    editor.on("update", invalidate);
    invalidate();
    return () => {
      resize.disconnect(); mutation.disconnect(); editor.off("update", invalidate);
      window.removeEventListener("scroll", invalidate, true); window.removeEventListener("resize", invalidate);
      cancelAnimationFrame(frame.current); frame.current = 0;
    };
  }, [editor, queue, shell]);
  const onMouseMove = (event: MouseEvent<HTMLDivElement>) => {
    if (locked || (event.target as HTMLElement).closest(".qy-block-add, [data-block-menu]")) return;
    point.current = event.clientY;
    queue();
  };
  const onMouseLeave = () => { point.current = null; cancelAnimationFrame(frame.current); frame.current = 0; if (!locked) setHover((current) => current.visible ? { ...current, visible: false } : current); };
  return { ...hover, onMouseMove, onMouseLeave };
}
