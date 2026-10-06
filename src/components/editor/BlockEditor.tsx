import { Extension } from "@tiptap/core";
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
import { Braces, CheckSquare, ChevronDown, Code2, Database, FilePlus2, FileUp, Heading1, Heading2, Heading3, Heading4, ImagePlus, Link2, List, ListOrdered, MessageSquareQuote, Minus, Plus, Table2, Text, TextQuote } from "lucide-react";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { type PageDocument, type PageKind, type PageSummary } from "../../lib/tauri/pages";
import { parseQingyuPageLink } from "../../lib/internalLinks";
import { documentToEditorContent, hydrateEditorContent, selectionWithLines, tiptapToDocument } from "./blockDocument";
import { QingyuCodeBlock } from "./CodeBlockNode";
import { storeEditorAttachment } from "./imageAttachments";
import { createPortal } from "react-dom";
import { useBlockHover } from "./useBlockHover";

const editorPositions = new Map<string, { from: number; to: number; scrollTop: number }>();

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
  onCreateChild?: (kind: "page" | "database", position: number) => void | Promise<void>;
  onSelectionAction?: (text: string) => void;
  pageKind?: PageKind;
};

export const BlockEditor = memo(function BlockEditor({ document, pages, targetBlockId, readOnly = false, onChange, onOpenPage, onCreateChild, onSelectionAction, pageKind = "page" }: Props) {
  const documentRef = useRef(document);
  const fileRef = useRef<HTMLInputElement>(null);
  const uploadRef = useRef<(files: File[]) => Promise<void>>(async () => undefined);
  const [menu, setMenu] = useState(false);
  const [linkMenu, setLinkMenu] = useState(false);
  const [menuPosition, setMenuPosition] = useState({ top: 0, left: 0, maxHeight: 420 });
  const fileInsertIndex = useRef<number | null>(null);
  const localBlocks = useRef<PageDocument["blocks"] | null>(null);
  const [menuBlockIndex, setMenuBlockIndex] = useState<number | null>(null);
  const [selectionText, setSelectionText] = useState("");
  const [selectionLines, setSelectionLines] = useState({ start: 1, end: 1 });
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
    content: documentToEditorContent(document),
    editable: !readOnly,
    editorProps: {
      attributes: { class: "qy-block-editor", spellcheck: "true" },
      handleClick(_view, _pos, event) {
        const anchor = (event.target as HTMLElement).closest("a") as HTMLAnchorElement | null;
        if (!anchor) return false;
        event.preventDefault();
        const internal = parseQingyuPageLink(anchor.href);
        if (internal) onOpenPage?.(internal.pageId, internal.blockId);
        else if (/^(https?:|mailto:)/i.test(anchor.href)) void openUrl(anchor.href);
        return true;
      },
      handlePaste(_view, event) {
        const files = Array.from(event.clipboardData?.files ?? []);
        if (!files.length) return false;
        event.preventDefault();
        void uploadRef.current(files);
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
      const before = current.state.doc.textBetween(0, from, "\n", "\n");
      const start = before ? before.split("\n").length : 1;
      setSelectionText(text);
      setSelectionLines({ start, end: start + Math.max(0, text.split("\n").length - 1) });
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
    let disposed = false;
    const pageId = document.pageId;
    const scroller = shellRef.current?.closest<HTMLElement>(".qy-page-scroll");
    const savedPosition = editorPositions.get(pageId);
    const content = documentToEditorContent(document);
    const restorePosition = () => {
      if (!savedPosition || targetBlockId) return;
      const size = editor.state.doc.content.size;
      editor.commands.setTextSelection({ from: Math.min(savedPosition.from, size), to: Math.min(savedPosition.to, size) });
      if (scroller) scroller.scrollTop = savedPosition.scrollTop;
    };
    // 本地保存只更新版本号，不替换仍在编辑的 DOM 和选区。
    const localSave = localBlocks.current !== null && JSON.stringify(localBlocks.current) === JSON.stringify(document.blocks);
    if (!localSave) editor.commands.setContent(content, { emitUpdate: false });
    restorePosition();
    const baseline = editor.state.doc;
    if (!localSave) void hydrateEditorContent(content).then((hydrated) => {
      if (disposed || editor.isDestroyed || documentRef.current.pageId !== pageId || editor.state.doc !== baseline) return;
      editor.commands.setContent(hydrated, { emitUpdate: false });
      restorePosition();
    }).catch(() => undefined);
    return () => {
      disposed = true;
      if (!editor.isDestroyed) {
        const { from, to } = editor.state.selection;
        editorPositions.delete(pageId);
        editorPositions.set(pageId, { from, to, scrollTop: scroller?.scrollTop ?? 0 });
        if (editorPositions.size > 50) editorPositions.delete(editorPositions.keys().next().value!);
      }
    };
  }, [document.pageId, document.revision, editor]);
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
  const activeBlockType = hover.type;
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
    for (const file of files) {
      const attachment = await storeEditorAttachment(document.pageId, file);
      let position: number | null = null;
      if (fileInsertIndex.current !== null) {
        position = 0;
        for (let i = 0; i < Math.min(fileInsertIndex.current + 1, editor.state.doc.childCount); i++) position += editor.state.doc.child(i).nodeSize;
      }
      if (attachment.image) {
        const dataUrl = await new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(String(reader.result));
          reader.onerror = () => reject(reader.error);
          reader.readAsDataURL(file);
        });
        const node = { type: "image", attrs: { src: dataUrl, alt: attachment.name, title: attachment.path } };
        if (position !== null) editor.chain().focus(undefined, { scrollIntoView: false }).insertContentAt(position, node).scrollIntoView().run();
        else editor.chain().focus().setImage(node.attrs).run();
      } else {
        const link = { type: "text", text: attachment.name, marks: [{ type: "link", attrs: { href: attachment.path } }, { type: "bold" }] };
        if (position !== null) editor.chain().focus(undefined, { scrollIntoView: false }).insertContentAt(position, { type: "paragraph", content: [link] }).scrollIntoView().run();
        else editor.chain().focus().insertContent(link).run();
      }
      if (fileInsertIndex.current !== null) fileInsertIndex.current++;
    }
    fileInsertIndex.current = null;
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
  const choose = (action: (chain: ReturnType<typeof editor.chain>) => ReturnType<typeof editor.chain> | void) => {
    const target = menuBlockIndex ?? activeBlockIndex;
    const parent = editor.state.doc;
    if (target >= parent.childCount) {
      setMenu(false);
      setMenuBlockIndex(null);
      return;
    }
    let startPos = 0;
    for (let i = 0; i < target; i++) {
      startPos += parent.child(i).nodeSize;
    }
    const node = parent.child(target);
    const from = Math.min(startPos + 1, editor.state.doc.content.size);
    const to = Math.min(startPos + node.nodeSize - 1, editor.state.doc.content.size);

    const chain = editor.chain().focus(undefined, { scrollIntoView: false }).setTextSelection({ from, to });
    action(chain);
    chain.scrollIntoView().run();

    setMenu(false);
    setLinkMenu(false);
    setMenuBlockIndex(null);
  };
  const items = [
    [Text, "文本", (c: ReturnType<typeof editor.chain>) => c.setParagraph()],
    [Heading1, "标题 1", (c: ReturnType<typeof editor.chain>) => c.setHeading({ level: 1 })],
    [Heading2, "标题 2", (c: ReturnType<typeof editor.chain>) => c.setHeading({ level: 2 })],
    [Heading3, "标题 3", (c: ReturnType<typeof editor.chain>) => c.setHeading({ level: 3 })],
    [Heading4, "标题 4", (c: ReturnType<typeof editor.chain>) => c.setHeading({ level: 4 })],
    [List, "项目符号列表", (c: ReturnType<typeof editor.chain>) => c.toggleBulletList()],
    [ListOrdered, "有序列表", (c: ReturnType<typeof editor.chain>) => c.toggleOrderedList()],
    [CheckSquare, "待办事项", (c: ReturnType<typeof editor.chain>) => c.toggleTaskList()],
    [ChevronDown, "折叠列表", (c: ReturnType<typeof editor.chain>) => c.toggleBulletList()],
    [MessageSquareQuote, "标注", (c: ReturnType<typeof editor.chain>) => c.toggleBlockquote()],
    [TextQuote, "引用", (c: ReturnType<typeof editor.chain>) => c.toggleBlockquote()],
    [Table2, "表格", (c: ReturnType<typeof editor.chain>) => c.insertTable({ rows: 3, cols: 3, withHeaderRow: true })],
    [Minus, "分隔线", (c: ReturnType<typeof editor.chain>) => c.setHorizontalRule()],
    [Code2, "代码", (c: ReturnType<typeof editor.chain>) => c.toggleCodeBlock()],
    [Braces, "Mermaid", (c: ReturnType<typeof editor.chain>) => c.setCodeBlock().updateAttributes("codeBlock", { language: "mermaid" })],
    [ImagePlus, "图片", () => fileRef.current?.click()],
    [FileUp, "文件", () => fileRef.current?.click()],
  ] as const;
  return <div ref={shellRef} className="qy-block-editor-shell" onMouseMove={hover.onMouseMove} onMouseLeave={hover.onMouseLeave} onContextMenu={(event) => { const target = event.target as HTMLElement; if (target.closest("p")?.textContent?.trim() === "") { event.preventDefault(); openBlockMenu(activeBlockIndex, addTop); } }} onPointerDown={(event) => { const target = event.target as HTMLElement; if (target.closest("[data-block-menu], .qy-block-add")) return; setMenu(false); setLinkMenu(false); }}>
    <input ref={fileRef} type="file" multiple className="hidden" aria-label="选择图片或附件" accept="image/*,.pdf,.doc,.docx,.ppt,.pptx,.xls,.xlsx,.md,.txt,.csv,.json,.zip,.7z,.rar" onChange={(event) => { const files = Array.from(event.currentTarget.files ?? []); event.currentTarget.value = ""; void upload(files); }}/>
    {!readOnly && (hoverVisible || menu) && <button type="button" className="qy-block-add" style={{ top: addTop }} aria-label="添加区块" onPointerDownCapture={(event) => { event.preventDefault(); event.stopPropagation(); openBlockMenu(activeBlockIndex, addTop); }} onClick={(event) => { event.preventDefault(); event.stopPropagation(); openBlockMenu(activeBlockIndex, addTop); }}><Plus size={17}/></button>}
    {selectionText && <button type="button" className="qy-selection-agent" style={{ top: selectionPosition.top, left: selectionPosition.left }} onMouseDown={(event) => event.preventDefault()} onClick={() => onSelectionAction?.(selectionWithLines("\n".repeat(selectionLines.start - 1), selectionText))}><MessageSquareQuote size={14}/>添加到对话</button>}
    {menu && !linkMenu && createPortal(<div data-block-menu className="qy-block-menu" role="dialog" aria-label="区块菜单" style={{ position: "fixed", ...menuPosition }} onMouseDown={(event) => event.preventDefault()}>
      <div className="qy-block-menu-title">转换当前区块 · {activeBlockType === "heading" ? "标题" : "正文"}</div>
      {items.map(([Icon, label, run]) => <button type="button" key={label} onClick={() => choose(run)}><Icon size={18}/><span>{label}</span></button>)}
      <div className="qy-block-menu-title">在当前行下方插入</div>
      <button type="button" onClick={() => insertAfter({ type: "paragraph" })}><Text size={18}/><span>空白正文</span></button>
      <button type="button" onClick={() => insertAfter({ type: "heading", attrs: { level: 1 } })}><Heading1 size={18}/><span>标题 1</span></button>
      <button type="button" onClick={() => insertAfter({ type: "heading", attrs: { level: 2 } })}><Heading2 size={18}/><span>标题 2</span></button>
      <button type="button" onClick={() => insertAfter({ type: "heading", attrs: { level: 3 } })}><Heading3 size={18}/><span>标题 3</span></button>
      <button type="button" onClick={() => insertAfter({ type: "heading", attrs: { level: 4 } })}><Heading4 size={18}/><span>标题 4</span></button>
      <button type="button" onClick={() => insertAfter({ type: "bulletList", content: [{ type: "listItem", content: [{ type: "paragraph" }] }] })}><List size={18}/><span>项目符号列表</span></button>
      <button type="button" onClick={() => insertAfter({ type: "orderedList", content: [{ type: "listItem", content: [{ type: "paragraph" }] }] })}><ListOrdered size={18}/><span>有序列表</span></button>
      <button type="button" onClick={() => insertAfter({ type: "taskList", content: [{ type: "taskItem", attrs: { checked: false }, content: [{ type: "paragraph" }] }] })}><CheckSquare size={18}/><span>待办事项</span></button>
      <button type="button" onClick={() => insertAfter({ type: "blockquote", content: [{ type: "paragraph" }] })}><TextQuote size={18}/><span>引用</span></button>
      <button type="button" onClick={() => insertAfter({ type: "table", content: [{ type: "tableRow", content: [{ type: "tableHeader", content: [{ type: "paragraph" }] }, { type: "tableHeader", content: [{ type: "paragraph" }] }, { type: "tableHeader", content: [{ type: "paragraph" }] }] }, { type: "tableRow", content: [{ type: "tableCell", content: [{ type: "paragraph" }] }, { type: "tableCell", content: [{ type: "paragraph" }] }, { type: "tableCell", content: [{ type: "paragraph" }] }] }] })}><Table2 size={18}/><span>表格</span></button>
      <button type="button" onClick={() => insertAfter({ type: "horizontalRule" })}><Minus size={18}/><span>分隔线</span></button>
      <button type="button" onClick={() => insertAfter({ type: "codeBlock", attrs: { language: null } })}><Code2 size={18}/><span>代码</span></button>
      <button type="button" onClick={() => insertAfter({ type: "codeBlock", attrs: { language: "mermaid" } })}><Braces size={18}/><span>Mermaid</span></button>
      <button type="button" onClick={() => { fileInsertIndex.current = menuBlockIndex ?? activeBlockIndex; setMenu(false); fileRef.current?.click(); }}><ImagePlus size={18}/><span>图片</span></button>
      <button type="button" onClick={() => { fileInsertIndex.current = menuBlockIndex ?? activeBlockIndex; setMenu(false); fileRef.current?.click(); }}><FileUp size={18}/><span>文件</span></button>
      <button type="button" onClick={() => { setMenu(false); void onCreateChild?.("page", (menuBlockIndex ?? activeBlockIndex) + 1); }}><FilePlus2 size={18}/><span>页面</span></button>
      {pageKind !== "database" && <button type="button" onClick={() => { setMenu(false); void onCreateChild?.("database", (menuBlockIndex ?? activeBlockIndex) + 1); }}><Database size={18}/><span>数据库</span></button>}
      <button type="button" onClick={() => setLinkMenu(true)}><Link2 size={18}/><span>链接到页面</span></button>
    </div>, window.document.body)}
    {linkMenu && createPortal(<div data-block-menu className="qy-page-link-menu" style={{ position: "fixed", ...menuPosition }} onMouseDown={(event) => event.preventDefault()}>{pages.map((page) => <button key={page.id} type="button" onClick={() => choose((c) => c.insertContent({ type: "text", text: page.title, marks: [{ type: "link", attrs: { href: `qingyu://page/${page.id}` } }, { type: "bold" }] }))}>{page.title}</button>)}</div>, window.document.body)}
    <EditorContent editor={editor}/>
  </div>;
});
