import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { defaultHighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { pageMentionExtensions, type PageMentionConfig } from "./pageMentions";
import { blockIdentityExtensions } from "./blockIdentity";

export const editorTheme = EditorView.theme({
  "&": { height: "100%", fontSize: "calc(15px * var(--qy-font-scale))", color: "var(--qy-ink)" },
  ".cm-scroller": { fontFamily: '"Inter Variable", Inter, ui-sans-serif, system-ui, sans-serif', overflow: "auto" },
  ".cm-content": { padding: "28px 32px", minHeight: "100%" },
  ".cm-gutters": { display: "none" },
  ".cm-focused": { outline: "none" },
  ".qy-page-mention": { color: "var(--qy-accent)", backgroundColor: "#eff6ff", borderRadius: "3px", cursor: "pointer" },
  ".qy-page-mention-missing": { color: "#b42318", backgroundColor: "#fef3f2", textDecoration: "line-through" },
  ".qy-block-highlight": { backgroundColor: "#fef3c7", borderRadius: "3px" },
});

export function createEditorExtensions(onChange: (value: string) => void, readOnly = false, mentions?: PageMentionConfig) {
  return [markdown(), history(), keymap.of([...defaultKeymap, ...historyKeymap]), EditorView.lineWrapping, syntaxHighlighting(defaultHighlightStyle), editorTheme, blockIdentityExtensions, mentions ? pageMentionExtensions(mentions) : [], EditorState.readOnly.of(readOnly), EditorView.updateListener.of((update) => { if (update.docChanged) onChange(update.state.doc.toString()); })];
}
