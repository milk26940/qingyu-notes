import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { ImagePlus } from "lucide-react";
import { createEditorExtensions } from "./editorExtensions";
import { storeImageAttachment, supportedImageFiles } from "./imageAttachments";

export type MarkdownEditorProps = { pageId?: string; value: string; onChange: (value: string) => void; onError?: (message: string) => void; readOnly?: boolean };

export function MarkdownEditor({ pageId, value, onChange, onError, readOnly = false }: MarkdownEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const viewRef = useRef<EditorView>();
  const valueRef = useRef(value);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  useEffect(() => {
    if (!hostRef.current) return;
    const view = new EditorView({ state: EditorState.create({ doc: value, extensions: createEditorExtensions((next) => { valueRef.current = next; onChangeRef.current(next); }, readOnly) }), parent: hostRef.current });
    viewRef.current = view;
    return () => { view.destroy(); viewRef.current = undefined; };
  }, [readOnly]);
  useEffect(() => {
    const view = viewRef.current;
    if (!view || value === valueRef.current || value === view.state.doc.toString()) return;
    valueRef.current = value;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
  }, [value]);
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
  return <div className="flex h-full min-h-0 flex-col" onPasteCapture={(event) => { const files = supportedImageFiles(event.clipboardData.files); if (files.length) { event.preventDefault(); void upload(files); } }} onDragOver={(event) => { if (supportedImageFiles(event.dataTransfer.files).length) event.preventDefault(); }} onDrop={(event) => { const files = supportedImageFiles(event.dataTransfer.files); if (files.length) { event.preventDefault(); void upload(files); } }}>
    {pageId && !readOnly && <div className="flex h-8 shrink-0 items-center border-b border-[var(--qy-border)]"><button type="button" className="icon-button" title="插入图片" aria-label="插入图片" onClick={() => fileRef.current?.click()}><ImagePlus size={15} /></button><input ref={fileRef} className="hidden" type="file" aria-label="选择图片" accept="image/png,image/jpeg,image/gif,image/webp" multiple onChange={(event) => { const files = supportedImageFiles(event.currentTarget.files ?? []); event.currentTarget.value = ""; void upload(files); }} /></div>}
    <div ref={hostRef} className="min-h-0 flex-1" aria-label="Markdown 编辑器" />
  </div>;
}
