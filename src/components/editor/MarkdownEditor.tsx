import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { Copy, ImagePlus } from "lucide-react";
import { createEditorExtensions } from "./editorExtensions";
import { storeImageAttachment, supportedImageFiles } from "./imageAttachments";
import type { PageSummary } from "../../lib/tauri/pages";
import { ensureBlockIds, findBlock, findBlockAt, revealBlock } from "./blockIdentity";

export type MarkdownEditorProps = { pageId?: string; pages?: PageSummary[]; targetBlockId?: string | null; value: string; onChange: (value: string) => void; onOpenPage?: (id: string, blockId?: string) => void; onError?: (message: string) => void; readOnly?: boolean };

export function MarkdownEditor({ pageId, pages = [], targetBlockId, value, onChange, onOpenPage, onError, readOnly = false }: MarkdownEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const viewRef = useRef<EditorView>();
  const valueRef = useRef(value);
  const onChangeRef = useRef(onChange);
  const pagesRef = useRef(pages);
  const onOpenPageRef = useRef(onOpenPage);
  onChangeRef.current = onChange;
  pagesRef.current = pages;
  onOpenPageRef.current = onOpenPage;
  useEffect(() => {
    if (!hostRef.current) return;
    const prepared = pageId ? ensureBlockIds(value) : value;
    valueRef.current = value;
    const view = new EditorView({ state: EditorState.create({ doc: value, extensions: createEditorExtensions((next) => { valueRef.current = next; onChangeRef.current(next); }, readOnly, { getPages: () => pagesRef.current, onOpenPage: (id, blockId) => onOpenPageRef.current?.(id, blockId), onMissingPage: () => onError?.("页面不存在") }) }), parent: hostRef.current });
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
    const view = viewRef.current;
    if (!pageId || !view || files.length === 0) return;
    try {
      const markdown = (await Promise.all(files.map((file) => storeImageAttachment(pageId, file)))).join("\n");
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
  const copyBlockLink = async () => {
    const view = viewRef.current;
    if (!pageId || !view) return;
    normalizeBlocks();
    const block = findBlockAt(view.state.doc.toString(), view.state.selection.main.head);
    if (!block) { onError?.("当前光标不在可复制的区块中"); return; }
    try { await navigator.clipboard.writeText(`qingyu://page/${pageId}#${block.id}`); }
    catch (error) { onError?.(error instanceof Error ? error.message : String(error)); }
  };
  return <div className="flex h-full min-h-0 flex-col" onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) normalizeBlocks(); }} onPasteCapture={(event) => { const files = supportedImageFiles(event.clipboardData.files); if (files.length) { event.preventDefault(); void upload(files); } }} onDragOver={(event) => { if (supportedImageFiles(event.dataTransfer.files).length) event.preventDefault(); }} onDrop={(event) => { const files = supportedImageFiles(event.dataTransfer.files); if (files.length) { event.preventDefault(); void upload(files); } }}>
    {pageId && !readOnly && <div className="flex h-8 shrink-0 items-center gap-1 border-b border-[var(--qy-border)]"><button type="button" className="icon-button" title="插入图片" aria-label="插入图片" onClick={() => fileRef.current?.click()}><ImagePlus size={15} /></button><button type="button" className="icon-button" title="复制当前区块链接" aria-label="复制当前区块链接" onClick={() => void copyBlockLink()}><Copy size={14} /></button><input ref={fileRef} className="hidden" type="file" aria-label="选择图片" accept="image/png,image/jpeg,image/gif,image/webp" multiple onChange={(event) => { const files = supportedImageFiles(event.currentTarget.files ?? []); event.currentTarget.value = ""; void upload(files); }} /></div>}
    <div ref={hostRef} className="min-h-0 flex-1" aria-label="Markdown 编辑器" />
  </div>;
}
