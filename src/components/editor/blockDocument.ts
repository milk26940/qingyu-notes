import type { JSONContent } from "@tiptap/core";
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

export function documentToTiptap(document: PageDocument): JSONContent {
  return {
    type: "doc",
    content: document.blocks.length
      ? document.blocks.flatMap(blockToNodes)
      : [{ type: "paragraph", attrs: { blockId: id() } }],
  };
}

export function documentToEditorContent(document: PageDocument): JSONContent | string {
  if (document.blocks.length === 1 && document.blocks[0].type === "markdown") {
    const source = document.blocks[0].text
      .replace(/<!--\s*qingyu-block:([A-Za-z0-9_-]+)\s*-->/g, '<p data-block-id="$1" class="qy-block-anchor"></p>\n\n')
      .replace(/<aside[^>]*>/gi, "> ")
      .replace(/<\/aside>/gi, "")
      .replace(/^---\s*---$/gm, "---");
    return DOMPurify.sanitize(marked.parse(source, { gfm: true, breaks: false }) as string, { ALLOW_UNKNOWN_PROTOCOLS: true });
  }
  return documentToTiptap(document);
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

function blockToNodes(block: BlockNode): JSONContent[] {
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
