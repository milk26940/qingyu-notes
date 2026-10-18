import { generateJSON, type Extensions, type JSONContent } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import DOMPurify from "dompurify";
import { marked } from "marked";
import { readPageAsset, type BlockNode, type PageDocument } from "../../lib/tauri/pages";

const id = () => crypto.randomUUID();

export function bytesToDataUrl(mime: string, bytes: number[] | Uint8Array): string {
  const data = bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
  let binary = "";
  const len = data.length;
  const chunkSize = 8192;
  for (let i = 0; i < len; i += chunkSize) {
    const chunk = data.subarray(i, Math.min(i + chunkSize, len));
    binary += String.fromCharCode(...chunk);
  }
  const base64 = btoa(binary);
  return `data:${mime};base64,${base64}`;
}

export function documentToTiptap(document: PageDocument, extensions: Extensions = [StarterKit]): JSONContent {
  return {
    type: "doc",
    content: document.blocks.length
      ? document.blocks.flatMap((block) => blockToNodes(block, extensions))
      : [{ type: "paragraph", attrs: { blockId: id() } }],
  };
}

const escapeHtml = (text: string) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const localImageSource = (source: string) => !/^(?:https?:|data:|\/\.noteapp\/assets\/)/i.test(source);

export function clipboardContainsRichText(text: string, html: string): boolean {
  if (!html) return false;
  const body = new DOMParser().parseFromString(html, "text/html").body;
  const semantic = body.querySelector("h1,h2,h3,h4,h5,h6,b,strong,i,em,u,s,del,ul,ol,li,table,blockquote,img,a,hr");
  if (semantic) return true;
  return body.textContent?.trim() !== text.trim();
}

export function freshPastedBlocks(node: JSONContent): JSONContent {
  return { ...node, attrs: node.attrs ? { ...node.attrs, blockId: id() } : undefined, content: node.content?.map(freshPastedBlocks) };
}

export function sanitizePastedHtml(html: string): string {
  const original = new DOMParser().parseFromString(html, "text/html");
  const supportedTags = new Set("P DIV SPAN H1 H2 H3 H4 H5 H6 BR STRONG B EM I U S DEL STRIKE CODE PRE UL OL LI BLOCKQUOTE TABLE THEAD TBODY TFOOT TR TD TH IMG A HR ASIDE MAIN ARTICLE SECTION".split(" "));
  for (const element of Array.from(original.body.querySelectorAll("*"))) {
    if (!supportedTags.has(element.tagName)) element.replaceWith(original.createTextNode(element.outerHTML));
  }
  const sanitized = DOMPurify.sanitize(original.body.innerHTML, {
    ALLOW_UNKNOWN_PROTOCOLS: true,
    FORBID_TAGS: ["script", "style", "iframe", "object", "embed"],
  });
  const parser = new DOMParser();
  const document = parser.parseFromString(sanitized, "text/html");
  for (const image of document.querySelectorAll<HTMLImageElement>("img")) {
    const source = image.getAttribute("src")?.trim() ?? "";
    if (localImageSource(source)) image.replaceWith(document.createTextNode(image.outerHTML));
  }
  return document.body.innerHTML;
}

export function markdownToHtml(text: string): string {
  // 编辑器尚不支持的扩展语法保留原文，避免解析时吞掉定义或标记。
  if (/^\s*\[\^[^\]]+\]:|\[\^[^\]]+\]|^\s*\$\$|(?<!\\)\$[^$\n]+\$|^\s*:\s+\S/m.test(text)) return "<p>" + escapeHtml(text).replace(/\n/g, "<br>") + "</p>";
  const source = text
    .replace(/<!--\s*qingyu-block:([A-Za-z0-9_-]+)\s*-->/g, '<p data-block-id="$1" class="qy-block-anchor"></p>\n\n')
    .replace(/<aside[^>]*>/gi, "> ")
    .replace(/<\/aside>/gi, "")
    .replace(/^---\s*---$/gm, "---");
  const renderer = new marked.Renderer();
  const image = renderer.image.bind(renderer);
  const list = renderer.list.bind(renderer);
  renderer.checkbox = () => "";
  renderer.list = (token) => {
    if (!token.items.some((item) => item.task)) return list(token);
    // 将连续的待办项与普通列表分组，避免混合列表丢失原有语义。
    const groups: typeof token.items[] = [];
    for (const item of token.items) {
      const current = groups[groups.length - 1];
      if (current && current[0].task === item.task) current.push(item);
      else groups.push([item]);
    }
    return groups.map((items) => items[0].task
      ? '<ul data-type="taskList">' + items.map((item) => '<li data-type="taskItem" data-checked="' + Boolean(item.checked) + '"><div>' + renderer.parser.parse(item.tokens) + "</div></li>").join("") + "</ul>"
      : list({ ...token, items })).join("\n");
  };
  renderer.image = (token) => localImageSource(token.href) ? escapeHtml(token.raw) : image(token);
  renderer.html = (token) => /^<p data-block-id="[A-Za-z0-9_-]+" class="qy-block-anchor"><\/p>/.test(token.text) ? token.text : escapeHtml(token.text);
  return sanitizePastedHtml(marked.parse(source, { gfm: true, breaks: false, renderer }) as string);
}

export function isLikelyMarkdownPaste(text: string): boolean {
  const value = text.replace(/\r\n?/g, "\n").trim();
  if (!value) return false;
  if (/^\x60\x60\x60[^\n]*\n[\s\S]*\n\x60\x60\x60\s*$/m.test(value) || /^~~~[^\n]*\n[\s\S]*\n~~~\s*$/m.test(value)) return true;
  if (/^#{1,6}\s+\S/m.test(value)) return true;
  if (/^(?:\s{0,3}[-+*]\s+\S|\s{0,3}\d+[.)]\s+\S|\s{0,3}>\s+\S)/m.test(value) && value.includes("\n")) return true;
  if (/^\s{0,3}[-*+]\s+\[[ xX]\]\s+\S/m.test(value)) return true;
  if (/^\s*\|?.+\|.+\n\s*\|?\s*:?-{3,}:?\s*\|/m.test(value)) return true;
  if (/!\[[^\]]*\]\([^)]+\)|\[[^\]]+\]\((?:https?:\/\/|mailto:|qingyu:)[^)]+\)/i.test(value)) return true;
  const inlineSignals = [/(^|\s)\*\*[^*\n]+\*\*(?=\s|[.,!?，。！？]|$)/, /(^|\s)~~[^~\n]+~~(?=\s|[.,!?，。！？]|$)/, /\x60[^\x60\n]+\x60/];
  return inlineSignals.some((pattern) => pattern.test(value)) || /(^|\s)(?:\*[^*\s][^*\n]*\*|_[^_\s][^_\n]*_)(?=\s|[.,!?，。！？]|$)/.test(value);
}

export function rawHtmlSourceFromClipboard(text: string, clipboardHtml: string): string | null {
  const source = text.trim();
  if (!/^<(?:!doctype\s+html|html|body|main|article|section|div|p|h[1-6]|ul|ol|table|blockquote|pre|aside)\b/i.test(source)) return null;
  if (!/<\/[a-z][^>]*>\s*$/i.test(source)) return null;
  if (!clipboardHtml) return source;
  const parser = new DOMParser();
  const wrapped = parser.parseFromString(clipboardHtml, "text/html").body.textContent?.trim() ?? "";
  return wrapped === source ? source : null;
}

export function documentToEditorContent(document: PageDocument, extensions: Extensions = [StarterKit]): JSONContent | string {
  if (document.blocks.length === 1 && document.blocks[0].type === "markdown") {
    return markdownToHtml(document.blocks[0].text);
  }
  return documentToTiptap(document, extensions);
}

export async function hydrateHtmlContent(
  html: string,
  readAsset: (path: string) => Promise<{ mime: string; bytes: number[] }> = readPageAsset,
): Promise<string> {
  if (!html.includes("/.noteapp/assets/")) {
    return html;
  }
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, "text/html");
  const images = Array.from(doc.querySelectorAll<HTMLImageElement>('img[src^="/.noteapp/assets/"]'));
  if (images.length === 0) {
    return html;
  }

  await Promise.all(
    images.map(async (image) => {
      const rawSrc = image.getAttribute("src") ?? "";
      try {
        const asset = await readAsset(rawSrc);
        const dataUrl = bytesToDataUrl(asset.mime, asset.bytes);
        image.setAttribute("src", dataUrl);
        image.setAttribute("title", rawSrc);
      } catch {
        image.setAttribute("title", rawSrc);
        const currentAlt = image.getAttribute("alt") || "图片";
        if (!currentAlt.includes("（加载失败）")) {
          image.setAttribute("alt", `${currentAlt}（加载失败）`);
        }
      }
    }),
  );

  return doc.body.innerHTML;
}

export async function hydrateJsonContent(
  node: JSONContent,
  readAsset: (path: string) => Promise<{ mime: string; bytes: number[] }> = readPageAsset,
): Promise<JSONContent> {
  const children = node.content
    ? await Promise.all(node.content.map((child) => hydrateJsonContent(child, readAsset)))
    : undefined;

  const rawSrc =
    typeof node.attrs?.src === "string" && node.attrs.src.startsWith("/.noteapp/assets/")
      ? node.attrs.src
      : typeof node.attrs?.title === "string" && node.attrs.title.startsWith("/.noteapp/assets/")
      ? node.attrs.title
      : null;

  if (node.type === "image" && rawSrc) {
    try {
      const asset = await readAsset(rawSrc);
      const dataUrl = bytesToDataUrl(asset.mime, asset.bytes);
      return {
        ...node,
        attrs: {
          ...(node.attrs ?? {}),
          title: rawSrc,
          src: dataUrl,
        },
        content: children,
      };
    } catch {
      const currentAlt = typeof node.attrs?.alt === "string" && node.attrs.alt ? node.attrs.alt : "图片";
      const alt = currentAlt.includes("（加载失败）") ? currentAlt : `${currentAlt}（加载失败）`;
      return {
        ...node,
        attrs: {
          ...(node.attrs ?? {}),
          title: rawSrc,
          alt,
          src: node.attrs?.src ?? rawSrc,
        },
        content: children,
      };
    }
  }

  return { ...node, content: children };
}

export async function hydrateEditorContent(
  content: JSONContent | string,
  readAsset: (path: string) => Promise<{ mime: string; bytes: number[] }> = readPageAsset,
): Promise<JSONContent | string> {
  if (typeof content === "string") {
    return hydrateHtmlContent(content, readAsset);
  }
  return hydrateJsonContent(content, readAsset);
}

function blockToNodes(block: BlockNode, extensions: Extensions): JSONContent[] {
  // Agent 可在富文本或 Markdown 页面中追加 Markdown，必须逐块解析后再交给编辑器。
  if (block.type === "markdown") {
    const parsed: JSONContent = generateJSON(markdownToHtml(block.text), extensions);
    return (parsed.content ?? []).map((node, index) => ({
      ...node,
      attrs: { ...(node.attrs ?? {}), blockId: node.attrs?.blockId ?? (index === 0 ? block.id : `${block.id}-${index}`) },
    }));
  }
  const stored = block.attrs.node;
  if (stored && typeof stored === "object") {
    const node = stored as JSONContent;
    if (node.type === "doc") {
      return (node.content ?? []).map((child) => ({
        ...child,
        attrs: { ...(child.attrs ?? {}), blockId: child.attrs?.blockId ?? block.id },
      }));
    }
    return [{ ...node, attrs: { ...(node.attrs ?? {}), blockId: block.id } }];
  }
  const type = block.type === "heading" ? "heading" : block.type || "paragraph";
  return [{
    type,
    attrs: { ...block.attrs, blockId: block.id },
    content: block.text ? [{ type: "text", text: block.text }] : undefined,
  }];
}

export function restoreImageNodes(node: JSONContent): JSONContent {
  let current = node;
  if (current.type === "image" && typeof current.attrs?.title === "string" && current.attrs.title.startsWith("/.noteapp/assets/")) {
    current = {
      ...current,
      attrs: {
        ...(current.attrs ?? {}),
        src: current.attrs.title,
      },
    };
  }
  if (current.content && Array.isArray(current.content)) {
    return {
      ...current,
      content: current.content.map(restoreImageNodes),
    };
  }
  return current;
}

export function tiptapToDocument(value: JSONContent, previous: PageDocument): PageDocument {
  const blocks = (value.content ?? []).map((node, position) => nodeToBlock(node, position));
  return { ...previous, blocks };
}

function nodeToBlock(node: JSONContent, position: number): BlockNode {
  const blockId = typeof node.attrs?.blockId === "string" ? node.attrs.blockId : id();
  const cleanedNode = restoreImageNodes(node);
  return {
    id: blockId,
    type: cleanedNode.type ?? "paragraph",
    text: textContent(cleanedNode),
    attrs: { position, node: { ...cleanedNode, attrs: { ...(cleanedNode.attrs ?? {}), blockId: undefined } } },
    children: [],
  };
}

function textContent(node: JSONContent): string {
  if (typeof node.text === "string") return node.text;
  return (node.content ?? []).map(textContent).join(node.type === "doc" ? "\n" : "");
}

export function headingsFromDocument(document: PageDocument) {
  return document.blocks.flatMap((block, index) => {
    const node = block.attrs.node as JSONContent | undefined;
    if (block.type !== "heading" && node?.type !== "heading") return [];
    return [{ id: block.id, text: block.text, level: Number(node?.attrs?.level ?? block.attrs.level ?? 1), index }];
  });
}

export function selectionWithLines(before: string, selected: string) {
  const start = before ? before.split("\n").length : 1;
  const end = start + Math.max(0, selected.split("\n").length - 1);
  return `${start === end ? `第 ${start} 行` : `第 ${start}-${end} 行`}\n${selected}`;
}
