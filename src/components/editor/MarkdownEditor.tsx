import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { createEditorExtensions } from "./editorExtensions";

export type MarkdownEditorProps = { value: string; onChange: (value: string) => void; readOnly?: boolean };

export function MarkdownEditor({ value, onChange, readOnly = false }: MarkdownEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
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
  return <div ref={hostRef} className="h-full min-h-0" aria-label="Markdown 编辑器" />;
}
