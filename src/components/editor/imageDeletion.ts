import type { EditorView } from "@codemirror/view";

const IMAGE_MARKDOWN = /!\[[^\]\r\n]*\]\([^\r\n)]+\)/g;
const BLOCK_MARKER_ONLY = /^\s*<!--\s*qy:block:[0-9a-fA-F-]{36}\s*-->\s*$/;
const BLOCK_MARKERS = /<!--\s*qy:block:[0-9a-fA-F-]{36}\s*-->/g;

export type ImageDeletionRange = { from: number; to: number };

function lineStart(markdown: string, position: number) {
  return markdown.lastIndexOf("\n", Math.max(0, position - 1)) + 1;
}

function lineEnd(markdown: string, position: number) {
  const newline = markdown.indexOf("\n", position);
  return newline < 0 ? markdown.length : newline + 1;
}

function previousLine(markdown: string, from: number) {
  if (from === 0) return null;
  const to = from;
  const start = lineStart(markdown, Math.max(0, from - 1));
  return { from: start, to, text: markdown.slice(start, to).replace(/\r?\n$/, "") };
}

function nextLine(markdown: string, to: number) {
  if (to >= markdown.length) return null;
  const end = lineEnd(markdown, to);
  return { from: to, to: end, text: markdown.slice(to, end).replace(/\r?\n$/, "") };
}

export function findImageDeletionRange(markdown: string, position: number): ImageDeletionRange | null {
  IMAGE_MARKDOWN.lastIndex = 0;
  for (const match of markdown.matchAll(IMAGE_MARKDOWN)) {
    const imageFrom = match.index;
    const imageTo = imageFrom + match[0].length;
    if (position < imageFrom || position > imageTo) continue;

    const from = lineStart(markdown, imageFrom);
    const to = lineEnd(markdown, imageTo);
    const remainder = markdown.slice(from, imageFrom) + markdown.slice(imageTo, to).replace(/\r?\n$/, "");
    if (remainder.replace(BLOCK_MARKERS, "").trim()) return { from: imageFrom, to: imageTo };

    let blockFrom = from;
    let blockTo = to;
    let adjacent = previousLine(markdown, blockFrom);
    while (adjacent && BLOCK_MARKER_ONLY.test(adjacent.text)) {
      blockFrom = adjacent.from;
      adjacent = previousLine(markdown, blockFrom);
    }
    adjacent = nextLine(markdown, blockTo);
    while (adjacent && BLOCK_MARKER_ONLY.test(adjacent.text)) {
      blockTo = adjacent.to;
      adjacent = nextLine(markdown, blockTo);
    }

    // 图片独占一行时同时收掉一个相邻空行，避免删除后留下额外空白。
    const following = nextLine(markdown, blockTo);
    if (following && !following.text.trim()) blockTo = following.to;
    return { from: blockFrom, to: blockTo };
  }
  return null;
}

export function deleteImageAtCursor(view: EditorView, direction: "backward" | "forward") {
  const selection = view.state.selection.main;
  const markdown = view.state.doc.toString();
  let range: ImageDeletionRange | null = null;

  if (selection.from !== selection.to) {
    IMAGE_MARKDOWN.lastIndex = 0;
    for (const match of markdown.matchAll(IMAGE_MARKDOWN)) {
      const imageTo = match.index + match[0].length;
      if (selection.from < imageTo && selection.to > match.index) {
        range = findImageDeletionRange(markdown, match.index);
        break;
      }
    }
  } else {
    const cursor = selection.from;
    range = findImageDeletionRange(markdown, cursor)
      ?? findImageDeletionRange(markdown, direction === "backward" ? cursor - 1 : cursor + 1);
  }

  if (!range) return false;
  view.dispatch({ changes: { from: range.from, to: range.to }, selection: { anchor: range.from } });
  return true;
}
