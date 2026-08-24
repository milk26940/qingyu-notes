import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { defaultHighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";

export const editorTheme = EditorView.theme({
  "&": { height: "100%", fontSize: "15px", color: "var(--qy-ink)" },
  ".cm-scroller": { fontFamily: "ui-monospace, SFMono-Regular, Consolas, monospace", overflow: "auto" },
  ".cm-content": { padding: "28px 32px", minHeight: "100%" },
  ".cm-gutters": { display: "none" },
  ".cm-focused": { outline: "none" },
});

export function createEditorExtensions(onChange: (value: string) => void, readOnly = false) {
  return [markdown(), history(), keymap.of([...defaultKeymap, ...historyKeymap]), EditorView.lineWrapping, syntaxHighlighting(defaultHighlightStyle), editorTheme, EditorState.readOnly.of(readOnly), EditorView.updateListener.of((update) => { if (update.docChanged) onChange(update.state.doc.toString()); })];
}
