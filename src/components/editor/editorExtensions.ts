import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { defaultHighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { pageMentionExtensions, type PageMentionConfig } from "./pageMentions";
import { blockIdentityExtensions } from "./blockIdentity";
import { notionDecorations } from "./notionDecorations";
import type { EditorSelection } from "./InlineAgentMenu";

export const editorTheme = EditorView.theme({
  "&": { height: "100%", fontSize: "calc(15px * var(--qy-font-scale))", color: "var(--qy-ink)" },
  ".cm-scroller": { fontFamily: '"Inter Variable", Inter, ui-sans-serif, system-ui, sans-serif', overflow: "auto" },
  ".cm-content": { padding: "28px 32px", minHeight: "100%" },
  ".cm-gutters": { display: "none" },
  ".cm-focused": { outline: "none" },
  ".qy-page-mention": { color: "var(--qy-accent)", backgroundColor: "#eff6ff", borderRadius: "3px", cursor: "pointer" },
  ".qy-page-mention-missing": { color: "#b42318", backgroundColor: "#fef3f2", textDecoration: "line-through" },
  ".qy-block-highlight": { backgroundColor: "#fef3c7", borderRadius: "3px" },
  ".qy-heading": { fontWeight: "700", color: "var(--qy-ink)" },
  ".qy-heading-1": { fontSize: "calc(28px * var(--qy-font-scale))", lineHeight: "1.3", paddingTop: "12px", paddingBottom: "4px" },
  ".qy-heading-2": { fontSize: "calc(22px * var(--qy-font-scale))", lineHeight: "1.35", paddingTop: "10px", paddingBottom: "3px" },
  ".qy-heading-3": { fontSize: "calc(18px * var(--qy-font-scale))", lineHeight: "1.4", paddingTop: "8px" },
  ".qy-strong": { fontWeight: "700" },
  ".qy-emphasis": { fontStyle: "italic" },
  ".qy-list-item": { paddingLeft: "4px" },
  ".qy-blockquote": { borderLeft: "3px solid #d0d5dd", paddingLeft: "12px", color: "var(--qy-muted)" },
  ".qy-inline-code": { fontFamily: "ui-monospace, SFMono-Regular, Consolas, monospace", backgroundColor: "#f2f4f7", borderRadius: "3px", padding: "1px 3px" },
  ".qy-code-block": { fontFamily: "ui-monospace, SFMono-Regular, Consolas, monospace", backgroundColor: "#f8fafc" },
  ".qy-image-widget": { display: "block", minHeight: "80px", maxWidth: "100%", padding: "8px 0", color: "var(--qy-faint)", fontSize: "calc(12px * var(--qy-font-scale))" },
  ".qy-image-widget img": { display: "block", maxWidth: "100%", maxHeight: "520px", borderRadius: "4px", objectFit: "contain" },
  ".qy-image-missing": { display: "grid", placeItems: "center", border: "1px dashed var(--qy-border)", backgroundColor: "var(--qy-bg)" },
});

export function createEditorExtensions(onChange: (value: string) => void, readOnly = false, mentions?: PageMentionConfig, onSelectionChange?: (selection: EditorSelection) => void) {
  return [markdown(), history(), keymap.of([...defaultKeymap, ...historyKeymap]), EditorView.lineWrapping, syntaxHighlighting(defaultHighlightStyle), editorTheme, blockIdentityExtensions, notionDecorations, mentions ? pageMentionExtensions(mentions) : [], EditorState.readOnly.of(readOnly), EditorView.updateListener.of((update) => { if (update.docChanged) onChange(update.state.doc.toString()); if (update.docChanged || update.selectionSet) { const selection = update.state.selection.main; onSelectionChange?.({ from: selection.from, to: selection.to }); } })];
}
