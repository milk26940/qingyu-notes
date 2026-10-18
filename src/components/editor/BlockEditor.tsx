import { Extension, generateJSON } from "@tiptap/core";
import { Fragment, Slice, type Node as ProseMirrorNode } from "@tiptap/pm/model";
import Image from "@tiptap/extension-image";
import Link from "@tiptap/extension-link";
import Placeholder from "@tiptap/extension-placeholder";
import { Table } from "@tiptap/extension-table";
import TableCell from "@tiptap/extension-table-cell";
import TableHeader from "@tiptap/extension-table-header";
import TableRow from "@tiptap/extension-table-row";
import TaskItem from "@tiptap/extension-task-item";
import TaskList from "@tiptap/extension-task-list";
import Underline from "@tiptap/extension-underline";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Braces, CheckSquare, Code2, Database, FilePlus2, FileUp, Heading1, Heading2, Heading3, Heading4, ImagePlus, Link2, List, ListOrdered, MessageSquareQuote, Minus, Plus, Table2, Text, TextQuote } from "lucide-react";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { type ContextReference } from "../../lib/tauri/conversations";
import { type Page, type PageDocument, type PageKind, type PageSummary } from "../../lib/tauri/pages";
import { parseQingyuPageLink } from "../../lib/internalLinks";
import { clipboardContainsRichText, freshPastedBlocks, documentToEditorContent, hydrateEditorContent, isLikelyMarkdownPaste, markdownToHtml, rawHtmlSourceFromClipboard, sanitizePastedHtml, tiptapToDocument } from "./blockDocument";
import { QingyuCodeBlock } from "./CodeBlockNode";
import { storeEditorAttachment } from "./imageAttachments";
import { createPortal } from "react-dom";
import { useBlockHover } from "./useBlockHover";

import { useSessionStore } from "../../stores/sessionStore";
import { usePagesStore } from "../../stores/pagesStore";

function emptyBlock(node: ProseMirrorNode) {
  if (node.isAtom || node.type.name === "table" || node.textContent.trim()) return false;
  let empty = true;
  node.descendants((child) => { if (child.isAtom && !child.isText && child.type.name !== "hardBreak") empty = false; });
  return empty;
}

const BlockIdentity = Extension.create({
  name: "blockIdentity",
  addGlobalAttributes() {
    return [{
      types: ["paragraph", "heading", "bulletList", "orderedList", "taskList", "blockquote", "codeBlock", "horizontalRule", "table", "image"],
      attributes: { blockId: { default: null, parseHTML: (element) => element.getAttribute("data-block-id"), renderHTML: (attributes) => attributes.blockId ? { "data-block-id": attributes.blockId } : {} } },
    }];
  },
});

type Props = {
  document: PageDocument;
  pages: PageSummary[];
  targetBlockId?: string | null;
  readOnly?: boolean;
  onChange: (document: PageDocument) => void;
  onOpenPage?: (id: string, blockId?: string | null) => void;
  onCreateChild?: (kind: "page" | "database") => Promise<Page | null>;
  onSelectionAction?: (reference: ContextReference) => void;
  pageKind?: PageKind;
};

export const BlockEditor = memo(function BlockEditor({ document, pages, targetBlockId, readOnly = false, onChange, onOpenPage, onCreateChild, onSelectionAction, pageKind = "page" }: Props) {
  const documentRef = useRef(document);
  const fileRef = useRef<HTMLInputElement>(null);
  const plainPasteRef = useRef(false);
  const uploadRef = useRef<(files: File[]) => Promise<void>>(async () => undefined);
  const [menu, setMenu] = useState(false);
  const [linkMenu, setLinkMenu] = useState(false);
  const [menuPosition, setMenuPosition] = useState({ top: 0, left: 0, maxHeight: 420 });
  const fileInsertIndex = useRef<number | null>(null);
  const localBlocks = useRef<PageDocument["blocks"] | null>(null);
  const [menuBlockIndex, setMenuBlockIndex] = useState<number | null>(null);
  const [selectionText, setSelectionText] = useState("");
  const [selectionRange, setSelectionRange] = useState({ from: 0, to: 0, blockIds: [] as string[] });
  const [selectionPosition, setSelectionPosition] = useState({ top: 0, left: 0 });
  const shellRef = useRef<HTMLDivElement>(null);
  documentRef.current = document;
  const extensions = useMemo(() => [
    StarterKit.configure({ link: false, codeBlock: false }), QingyuCodeBlock, BlockIdentity, Underline,
    Link.configure({ openOnClick: false, autolink: true, protocols: ["http", "https", "mailto", "qingyu"] }),
    Image.configure({ inline: false, allowBase64: true, HTMLAttributes: { loading: "lazy", decoding: "async" } }),
    Table.configure({ resizable: true }), TableRow, TableHeader, TableCell, TaskList, TaskItem.configure({ nested: true }),
    Placeholder.configure({ placeholder: "输入内容，或点击左侧 + 添加区块" }),
  ], []);
  const editor = useEditor({
    extensions,
    shouldRerenderOnTransaction: false,
    content: documentToEditorContent(document, extensions),
    editable: !readOnly,
    editorProps: {
      attributes: { class: "qy-block-editor", spellcheck: "true" },
      handleKeyDown(_view, event) {
        plainPasteRef.current = (event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === "v";
        return false;
      },
      handleClick(_view, _pos, event) {
        const anchor = (event.target as HTMLElement).closest("a") as HTMLAnchorElement | null;
        if (!anchor) return false;
        event.preventDefault();
        const internal = parseQingyuPageLink(anchor.href);
        if (internal) onOpenPage?.(internal.pageId, internal.blockId);
        else if (/^(https?:|mailto:)/i.test(anchor.href)) void openUrl(anchor.href);
        return true;
      },
      transformPastedHTML(html) {
        return sanitizePastedHtml(html).replace(/\sdata-block-id=(?:"[^"]*"|'[^']*')/gi, () => ' data-block-id="' + crypto.randomUUID() + '"');
      },
      handlePaste(view, event) {
        const plainPaste = plainPasteRef.current;
        plainPasteRef.current = false;
        const text = event.clipboardData?.getData("text/plain") ?? "";
        if (view.state.selection.$from.parent.type.name === "codeBlock") {
          event.preventDefault();
          view.dispatch(view.state.tr.insertText(text.replace(/\r\n?/g, "\n")).scrollIntoView());
          return true;
        }
        const files = Array.from(event.clipboardData?.files ?? []);
        if (files.length) {
          event.preventDefault();
          void uploadRef.current(files);
          return true;
        }
        if (plainPaste) {
          event.preventDefault();
          const lines = text.replace(/\r\n?/g, "\n").split("\n");
          if (lines.length === 1) view.dispatch(view.state.tr.insertText(text).scrollIntoView());
          else {
            const nodes = lines.map((line) => view.state.schema.nodes.paragraph.create(null, line ? view.state.schema.text(line) : undefined));
            view.dispatch(view.state.tr.replaceSelection(Slice.maxOpen(Fragment.fromArray(nodes))).scrollIntoView());
          }
          return true;
        }
        const clipboardHtml = event.clipboardData?.getData("text/html") ?? "";
        if (clipboardContainsRichText(text, clipboardHtml)) return false;
        const rawHtml = rawHtmlSourceFromClipboard(text, clipboardHtml);
        if (!rawHtml && !isLikelyMarkdownPaste(text)) return false;
        event.preventDefault();
        const html = rawHtml ? sanitizePastedHtml(rawHtml) : markdownToHtml(text);
        const parsed = freshPastedBlocks(generateJSON(html, extensions));
        editor?.chain().focus(undefined, { scrollIntoView: false }).insertContent(parsed.content ?? []).scrollIntoView().run();
        return true;
      },
      handleDrop(_view, event) {
        const files = Array.from(event.dataTransfer?.files ?? []);
        if (!files.length) return false;
        event.preventDefault();
        void uploadRef.current(files);
        return true;
      },
    },
    onUpdate({ editor: current }) { const next = tiptapToDocument(current.getJSON(), documentRef.current); localBlocks.current = next.blocks; onChange(next); },
    onSelectionUpdate({ editor: current }) {
      const { from, to, empty } = current.state.selection;
      const text = empty ? "" : current.state.doc.textBetween(from, to, "\n", "\n");
      setSelectionText(text);
      const blockIds = new Set<string>();
      if (!empty) current.state.doc.nodesBetween(from, to, (node) => {
        const blockId = typeof node.attrs?.blockId === "string" ? node.attrs.blockId : "";
        if (blockId) blockIds.add(blockId);
      });
      setSelectionRange({ from, to, blockIds: [...blockIds] });
      if (!empty && shellRef.current) {
        const nativeSelection = window.getSelection();
        const range = nativeSelection?.rangeCount ? nativeSelection.getRangeAt(0) : null;
        const rect = range?.getBoundingClientRect();
        const shellRect = shellRef.current.getBoundingClientRect();
        if (rect && rect.width + rect.height > 0) {
          setSelectionPosition({
            top: Math.max(4, rect.top - shellRect.top - 44),
            left: Math.max(0, rect.left - shellRect.left),
          });
        }
      }
    },
  });
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    if (!selectionText) return;
    const position = () => {
      if (editor.isDestroyed || !shellRef.current) return;
      const start = editor.view.coordsAtPos(editor.state.selection.from);
      const shell = shellRef.current.getBoundingClientRect();
      setSelectionPosition({ top: Math.max(4, start.top - shell.top - 44), left: Math.max(0, Math.min(start.left - shell.left, shell.width - 140)) });
    };
    position();
    window.addEventListener("scroll", position, true);
    window.addEventListener("resize", position);
    const observer = new ResizeObserver(position); observer.observe(editor.view.dom);
    return () => { window.removeEventListener("scroll", position, true); window.removeEventListener("resize", position); observer.disconnect(); };
  }, [editor, selectionText, selectionRange.from, selectionRange.to]);
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    let disposed = false;
    const pageId = document.pageId;
    const scroller = shellRef.current?.closest<HTMLElement>(".qy-page-scroll");
    const sessionVault = useSessionStore.getState().vault;
    const savedPosition = useSessionStore.getState().positions[pageId];
    const content = documentToEditorContent(document, extensions);
    const restorePosition = () => {
      if (!savedPosition || targetBlockId) return;
      const size = editor.state.doc.content.size;
      editor.commands.setTextSelection({ from: Math.max(1, Math.min(savedPosition.from, size)), to: Math.max(1, Math.min(savedPosition.to, size)) });
      if (scroller) scroller.scrollTop = savedPosition.scrollTop;
    };
    // 本地保存只更新版本号，不替换仍在编辑的 DOM 和选区。
    const localSave = localBlocks.current !== null && JSON.stringify(localBlocks.current) === JSON.stringify(document.blocks);
    if (!localSave) editor.commands.setContent(content, { emitUpdate: false });
    if (!localSave) restorePosition();
    const baseline = editor.state.doc;
    if (!localSave) void hydrateEditorContent(content).then((hydrated) => {
      if (disposed || editor.isDestroyed || documentRef.current.pageId !== pageId || editor.state.doc !== baseline) return;
      editor.commands.setContent(hydrated, { emitUpdate: false });
      restorePosition();
    }).catch(() => undefined);
    return () => {
      disposed = true;
      if (!editor.isDestroyed && useSessionStore.getState().vault === sessionVault) {
        const { from, to } = editor.state.selection;
        useSessionStore.getState().rememberPosition(pageId, { from, to, scrollTop: scroller?.scrollTop ?? 0 });
      }
    };
  }, [document.pageId, document.revision, editor]);
  useEffect(() => {
    if (!editor) return;
    const scroller = shellRef.current?.closest<HTMLElement>(".qy-page-scroll");
    const sessionVault = useSessionStore.getState().vault;
    const remember = () => { if (!editor.isDestroyed && useSessionStore.getState().vault === sessionVault) { const { from, to } = editor.state.selection; useSessionStore.getState().rememberPosition(document.pageId, { from, to, scrollTop: scroller?.scrollTop ?? 0 }); } };
    let timer: ReturnType<typeof setTimeout> | undefined;
    const schedule = () => { clearTimeout(timer); timer = setTimeout(remember, 150); };
    editor.on("selectionUpdate", schedule); scroller?.addEventListener("scroll", schedule); window.addEventListener("qingyu:remember-position", remember);
    return () => { clearTimeout(timer); remember(); editor.off("selectionUpdate", schedule); scroller?.removeEventListener("scroll", schedule); window.removeEventListener("qingyu:remember-position", remember); };
  }, [editor, document.pageId]);
  useEffect(() => {
    if (!editor || !targetBlockId) return;
    const frame = window.requestAnimationFrame(() => {
      const root = editor.view.dom;
      const target = root.querySelector<HTMLElement>(`[data-block-id="${CSS.escape(targetBlockId)}"], #${CSS.escape(targetBlockId)}`);
      if (!target) return;
      target.scrollIntoView({ behavior: "smooth", block: "center" });
      target.classList.add("qy-target-block");
      window.setTimeout(() => target.classList.remove("qy-target-block"), 1200);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [document.pageId, editor, targetBlockId]);
  useEffect(() => {
    const close = (event: KeyboardEvent) => { if (event.key === "Escape") { setMenu(false); setLinkMenu(false); } };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, []);
  const hover = useBlockHover(shellRef, editor, menu || linkMenu);
  const activeBlockIndex = hover.index;
  const addTop = hover.top;
  const hoverVisible = hover.visible;
  useEffect(() => {
    const close = (event: PointerEvent) => { if (!(event.target as HTMLElement).closest("[data-block-menu], .qy-block-add")) { setMenu(false); setLinkMenu(false); } };
    const shortcut = (event: KeyboardEvent) => {
      if (!editor || readOnly || event.isComposing || !(event.ctrlKey || event.metaKey) || !event.shiftKey || event.key.toLowerCase() !== "p" || !shellRef.current?.contains(window.document.activeElement)) return;
      event.preventDefault();
      const index = editor.state.selection.$from.index(0);
      const element = editor.view.dom.children[index] as HTMLElement | undefined;
      if (element && shellRef.current) openBlockMenu(index, element.getBoundingClientRect().top - shellRef.current.getBoundingClientRect().top);
    };
    window.document.addEventListener("pointerdown", close);
    window.addEventListener("keydown", shortcut);
    return () => { window.document.removeEventListener("pointerdown", close); window.removeEventListener("keydown", shortcut); };
  }, [editor, readOnly]);
  if (!editor) return null;
  const upload = async (files: File[]) => {
    const targetIndex = fileInsertIndex.current;
    fileInsertIndex.current = null;
    const nodes: Record<string, unknown>[] = [];
    try {
      for (const file of files) {
        const attachment = await storeEditorAttachment(document.pageId, file);
        if (attachment.image) {
          const dataUrl = await new Promise<string>((resolve, reject) => {
            const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = () => reject(reader.error); reader.readAsDataURL(file);
          });
          nodes.push({ type: "image", attrs: { src: dataUrl, alt: attachment.name, title: attachment.path } });
        } else nodes.push({ type: "paragraph", content: [{ type: "text", text: attachment.name, marks: [{ type: "link", attrs: { href: attachment.path } }, { type: "bold" }] }] });
      }
      if (editor.isDestroyed || !nodes.length) return;
      const chain = editor.chain().focus(undefined, { scrollIntoView: false });
      if (targetIndex !== null && targetIndex < editor.state.doc.childCount) {
        let position = 0;
        for (let i = 0; i < targetIndex; i++) position += editor.state.doc.child(i).nodeSize;
        const target = editor.state.doc.child(targetIndex);
        const at = emptyBlock(target) ? { from: position, to: position + target.nodeSize } : position + target.nodeSize;
        chain.insertContentAt(at, nodes).scrollIntoView().run();
      } else chain.insertContent(nodes).scrollIntoView().run();
    } catch (error) { usePagesStore.getState().reportError(String(error)); }
  };
  uploadRef.current = upload;
  const openBlockMenu = (index: number, top: number) => {
    setMenuBlockIndex(index);
    const rect = shellRef.current?.getBoundingClientRect();
    if (rect) {
      const anchor = rect.top + top;
      const below = window.innerHeight - anchor - 38;
      const above = anchor - 12;
      const upward = below < 300 && above > below;
      const maxHeight = Math.min(420, Math.max(120, upward ? above : below));
      setMenuPosition({ top: upward ? Math.max(12, anchor - maxHeight) : Math.max(12, anchor + 30), left: Math.max(12, Math.min(rect.left + 34, window.innerWidth - 284)), maxHeight });
    }
    setMenu(true);
    setLinkMenu(false);
  };

  const insertAfter = (content: Record<string, unknown>) => {
    const parent = editor.state.doc;
    const current = menuBlockIndex ?? activeBlockIndex;
    const index = Math.max(0, Math.min(current + 1, parent.childCount));
    let position = 0;
    for (let i = 0; i < index; i++) position += parent.child(i).nodeSize;
    editor.chain().focus(undefined, { scrollIntoView: false }).insertContentAt(position, content).scrollIntoView().run();
    setMenu(false);
    setMenuBlockIndex(null);
  };
  const chooseNode = (content: Record<string, unknown>) => {
    const index = menuBlockIndex ?? activeBlockIndex;
    const node = editor.state.doc.child(index);
    if (emptyBlock(node)) {
      let position = 0;
      for (let i = 0; i < index; i++) position += editor.state.doc.child(i).nodeSize;
      editor.chain().focus(undefined, { scrollIntoView: false }).insertContentAt({ from: position, to: position + node.nodeSize }, content).scrollIntoView().run();
      setMenu(false); setLinkMenu(false); setMenuBlockIndex(null);
    } else insertAfter(content);
  };
  const createChild = async (kind: "page" | "database") => {
    setMenu(false);
    const child = await onCreateChild?.(kind);
    if (!child || editor.isDestroyed) return;
    chooseNode({ type: "paragraph", content: [{ type: "text", text: child.title, marks: [{ type: "link", attrs: { href: `qingyu://page/${child.id}` } }, { type: "bold" }] }] });
  };
  const paragraph = { type: "paragraph" };
  const items = [
    [Text, "正文", paragraph],
    [Heading1, "标题 1", { type: "heading", attrs: { level: 1 } }],
    [Heading2, "标题 2", { type: "heading", attrs: { level: 2 } }],
    [Heading3, "标题 3", { type: "heading", attrs: { level: 3 } }],
    [Heading4, "标题 4", { type: "heading", attrs: { level: 4 } }],
    [List, "项目符号列表", { type: "bulletList", content: [{ type: "listItem", content: [paragraph] }] }],
    [ListOrdered, "有序列表", { type: "orderedList", content: [{ type: "listItem", content: [paragraph] }] }],
    [CheckSquare, "待办事项", { type: "taskList", content: [{ type: "taskItem", attrs: { checked: false }, content: [paragraph] }] }],
    [TextQuote, "引用", { type: "blockquote", content: [paragraph] }],
    [Table2, "表格", { type: "table", content: [
      { type: "tableRow", content: Array.from({ length: 3 }, () => ({ type: "tableHeader", content: [paragraph] })) },
      { type: "tableRow", content: Array.from({ length: 3 }, () => ({ type: "tableCell", content: [paragraph] })) },
    ] }],
    [Minus, "分隔线", { type: "horizontalRule" }],
    [Code2, "代码", { type: "codeBlock", attrs: { language: null } }],
    [Braces, "Mermaid", { type: "codeBlock", attrs: { language: "mermaid" } }],
  ] as const;
  return <div ref={shellRef} className="qy-block-editor-shell" onMouseMove={hover.onMouseMove} onMouseLeave={hover.onMouseLeave} onContextMenu={(event) => { const target = event.target as HTMLElement; if (target.closest("p")?.textContent?.trim() === "") { event.preventDefault(); openBlockMenu(activeBlockIndex, addTop); } }} onPointerDown={(event) => { const target = event.target as HTMLElement; if (target.closest("[data-block-menu], .qy-block-add")) return; setMenu(false); setLinkMenu(false); }}>
    <input ref={fileRef} type="file" multiple className="hidden" aria-label="选择图片或附件" accept="image/*,.pdf,.doc,.docx,.ppt,.pptx,.xls,.xlsx,.md,.txt,.csv,.json,.zip,.7z,.rar" onChange={(event) => { const files = Array.from(event.currentTarget.files ?? []); event.currentTarget.value = ""; void upload(files); }}/>
    {!readOnly && (hoverVisible || menu) && <button type="button" className="qy-block-add" style={{ top: addTop }} aria-label="添加区块" onPointerDownCapture={(event) => { event.preventDefault(); event.stopPropagation(); openBlockMenu(activeBlockIndex, addTop); }} onClick={(event) => { event.preventDefault(); event.stopPropagation(); openBlockMenu(activeBlockIndex, addTop); }}><Plus size={17}/></button>}
    {selectionText && <button type="button" className="qy-selection-agent" style={{ top: selectionPosition.top, left: selectionPosition.left }} onMouseDown={(event) => event.preventDefault()} onClick={() => onSelectionAction?.({ id: crypto.randomUUID(), source: "page", text: selectionText, title: document.title, pageId: document.pageId, pageRevision: document.revision, blockIds: selectionRange.blockIds, from: selectionRange.from, to: selectionRange.to })}><MessageSquareQuote size={14}/>添加到对话</button>}
    {menu && !linkMenu && createPortal(<div data-block-menu className="qy-block-menu" role="dialog" aria-label="区块菜单" style={{ position: "fixed", ...menuPosition }} onMouseDown={(event) => event.preventDefault()}>
      <div className="qy-block-menu-title">选择区块类型</div>
      <div className="qy-block-menu-hint">空行直接转换，有内容时在下方插入</div>
      {items.map(([Icon, label, content]) => <button type="button" key={label} onClick={() => chooseNode(content as Record<string, unknown>)}><Icon size={18}/><span>{label}</span></button>)}
      <button type="button" onClick={() => { fileInsertIndex.current = menuBlockIndex ?? activeBlockIndex; setMenu(false); fileRef.current?.click(); }}><ImagePlus size={18}/><span>图片</span></button>
      <button type="button" onClick={() => { fileInsertIndex.current = menuBlockIndex ?? activeBlockIndex; setMenu(false); fileRef.current?.click(); }}><FileUp size={18}/><span>文件</span></button>
      <button type="button" onClick={() => void createChild("page")}><FilePlus2 size={18}/><span>页面</span></button>
      {pageKind !== "database" && <button type="button" onClick={() => void createChild("database")}><Database size={18}/><span>数据库</span></button>}
      <button type="button" onClick={() => setLinkMenu(true)}><Link2 size={18}/><span>链接到页面</span></button>
    </div>, window.document.body)}
    {linkMenu && createPortal(<div data-block-menu className="qy-page-link-menu" style={{ position: "fixed", ...menuPosition }} onMouseDown={(event) => event.preventDefault()}>{pages.map((page) => <button key={page.id} type="button" onClick={() => { chooseNode({ type: "paragraph", content: [{ type: "text", text: page.title, marks: [{ type: "link", attrs: { href: `qingyu://page/${page.id}` } }] }] }); setLinkMenu(false); }}>{page.title}</button>)}</div>, window.document.body)}
    <EditorContent editor={editor}/>
  </div>;
});
