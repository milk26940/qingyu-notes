import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import { Database, FilePlus2, MessageSquare } from "lucide-react";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { createEditorExtensions } from "./editorExtensions";
import { clipboardImageFiles, storeImageAttachment, supportedImageFiles } from "./imageAttachments";
import type { PageSummary } from "../../lib/tauri/pages";
import { ensureBlockIds, findBlock, revealBlock } from "./blockIdentity";
import type { EditorSelection } from "./InlineAgentMenu";

export type EditorHandle = { scrollToHeading: (heading: { text: string; line: number }) => void; getSelection: () => string };
export type MarkdownEditorProps = { pageId?: string; pageTitle?: string; pages?: PageSummary[]; targetBlockId?: string | null; value: string; onChange: (value: string) => void; onOpenPage?: (id: string, blockId?: string) => void; onError?: (message: string) => void; onSelectionChange?: (text: string) => void; onSelectionAction?: (action: "add", text: string) => void; onCreateChild?: (kind: "page" | "database", position: number) => void | Promise<void>; readOnly?: boolean };

export const MarkdownEditor = forwardRef<EditorHandle, MarkdownEditorProps>(function MarkdownEditor({ pageId, pages = [], targetBlockId, value, onChange, onOpenPage, onError, onSelectionChange, onSelectionAction, onCreateChild, readOnly = false }, ref) {
  const hostRef = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const viewRef = useRef<EditorView>();
  const valueRef = useRef(value);
  const onChangeRef = useRef(onChange);
  const pagesRef = useRef(pages);
  const onOpenPageRef = useRef(onOpenPage);
  const [selection, setSelection] = useState<EditorSelection>({ from: 0, to: 0 });
  const [selectionPosition, setSelectionPosition] = useState({ left: 12, top: 4 });
  const [createMenu, setCreateMenu] = useState<{ left: number; top: number; position: number } | null>(null);
  onChangeRef.current = onChange;
  pagesRef.current = pages;
  onOpenPageRef.current = onOpenPage;
  useImperativeHandle(ref, () => ({ scrollToHeading: ({ line }) => { const view = viewRef.current; if (!view) return; const lineInfo = view.state.doc.line(Math.min(line, view.state.doc.lines)); view.dispatch({ selection: { anchor: lineInfo.from }, effects: EditorView.scrollIntoView(lineInfo.from, { y: "center" }) }); view.focus(); }, getSelection: () => { const view = viewRef.current; return view ? view.state.doc.sliceString(view.state.selection.main.from, view.state.selection.main.to) : ""; } }), []);
  useEffect(() => { onSelectionChange?.(value.slice(selection.from, selection.to)); }, [onSelectionChange, selection.from, selection.to, value]);
  useEffect(() => {
    if (!hostRef.current) return;
    const prepared = pageId ? ensureBlockIds(value) : value;
    valueRef.current = value;
    const handleSelection = (next: EditorSelection) => { setSelection(next); try { const coords = viewRef.current?.coordsAtPos(next.from); const host = hostRef.current?.getBoundingClientRect(); if (coords && host) setSelectionPosition({ left: Math.max(8, coords.left - host.left), top: Math.max(4, coords.top - host.top - 42) }); } catch { setSelectionPosition({ left: 12, top: 4 }); } };
    const view = new EditorView({ state: EditorState.create({ doc: value, extensions: createEditorExtensions((next) => { valueRef.current = next; onChangeRef.current(next); }, readOnly, { getPages: () => pagesRef.current, onOpenPage: (id, blockId) => onOpenPageRef.current?.(id, blockId), onMissingPage: () => onError?.("页面不存在") }, handleSelection) }), parent: hostRef.current });
    viewRef.current = view;
    if (prepared !== value) view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: prepared } });
    return () => { view.destroy(); viewRef.current = undefined; };
  }, [pageId, readOnly]);
  useEffect(() => {
    const view = viewRef.current;
    if (!view || value === valueRef.current || value === view.state.doc.toString()) return;
    const prepared = pageId ? ensureBlockIds(value) : value;
    valueRef.current = prepared;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: prepared } });
  }, [pageId, value]);
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !targetBlockId) return;
    const block = findBlock(view.state.doc.toString(), targetBlockId);
    if (block) revealBlock(view, block);
    else onError?.("未找到目标区块");
  }, [onError, targetBlockId]);
  const upload = async (files: File[]) => {
    if (!pageId || !viewRef.current || files.length === 0) return;
    try {
      const markdown = (await Promise.all(files.map((file) => storeImageAttachment(pageId, file)))).join("\n");
      const view = viewRef.current;
      if (!view) return;
      const selection = view.state.selection.main;
      view.dispatch({ changes: { from: selection.from, to: selection.to, insert: markdown }, selection: { anchor: selection.from + markdown.length } });
      view.focus();
    } catch (error) {
      onError?.(error instanceof Error ? error.message : String(error));
    }
  };
  const normalizeBlocks = () => {
    const view = viewRef.current;
    if (!pageId || !view) return;
    const current = view.state.doc.toString();
    const prepared = ensureBlockIds(current);
    if (prepared !== current) view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: prepared } });
  };
  const selected = selection.to > selection.from ? value.slice(selection.from, selection.to).trim() : "";
  const runSelectionAction = () => { if (selected) onSelectionAction?.("add", selected); };
  return <div className="relative min-h-full" onPointerDown={(event) => { if (!(event.target as HTMLElement).closest("[data-editor-create-menu]")) setCreateMenu(null); }} onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) normalizeBlocks(); }} onPasteCapture={(event) => { const files = clipboardImageFiles(event.clipboardData); if (files.length) { event.preventDefault(); void upload(files); } }} onDragOver={(event) => { if (supportedImageFiles(event.dataTransfer.files).length) event.preventDefault(); }} onDrop={(event) => { const files = supportedImageFiles(event.dataTransfer.files); if (files.length) { event.preventDefault(); void upload(files); } }}>
    {pageId && !readOnly && <input ref={fileRef} className="hidden" type="file" aria-label="选择图片" accept="image/png,image/jpeg,image/gif,image/webp" multiple onChange={(event) => { const files = supportedImageFiles(event.currentTarget.files ?? []); event.currentTarget.value = ""; void upload(files); }} />}
    {selected && !readOnly && <div className="absolute z-20 flex items-center rounded-xl border border-[var(--qy-border)] bg-white p-1 shadow-lg" style={{ left: selectionPosition.left, top: selectionPosition.top }}><button type="button" className="inline-flex h-7 items-center gap-1 rounded-lg px-2 text-[12px] text-[var(--qy-ink)] hover:bg-[var(--qy-bg)]" onClick={runSelectionAction}><MessageSquare size={14}/>添加到对话</button></div>}
    {createMenu && <div data-editor-create-menu className="absolute z-30 min-w-40 rounded-md border border-[var(--qy-border)] bg-white p-1 text-[12px] shadow-lg" style={{ left: createMenu.left, top: createMenu.top }}><button type="button" className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]" onClick={() => { const menu = createMenu; setCreateMenu(null); void onCreateChild?.("page", menu.position); }}><FilePlus2 size={14}/>新建子页面</button><button type="button" className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]" onClick={() => { const menu = createMenu; setCreateMenu(null); void onCreateChild?.("database", menu.position); }}><Database size={14}/>新建数据库</button></div>}
    <div ref={hostRef} className="min-h-full" aria-label="Markdown 编辑器" onContextMenu={(event) => { if (!pageId || readOnly || !onCreateChild) return; const view = viewRef.current; const lineElement = (event.target as HTMLElement).closest(".cm-line"); if (!view || !lineElement) return; try { const position = view.posAtDOM(lineElement, 0); const line = view.state.doc.lineAt(position); if (line.text.trim()) return; event.preventDefault(); const host = hostRef.current?.getBoundingClientRect(); setCreateMenu({ left: event.clientX - (host?.left ?? 0), top: event.clientY - (host?.top ?? 0), position: line.from }); } catch { return; } }} />
  </div>;
});
