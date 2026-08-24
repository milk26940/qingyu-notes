import { StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, MatchDecorator, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";

const BLOCK_LINE = /^<!-- qy:block:([0-9a-fA-F-]{36}) -->$/;
const BLOCK_MARKER = /^<!-- qy:block:([0-9a-fA-F-]{36}) -->$/gm;

export type MarkdownBlock = { id: string; from: number; to: number; markerFrom: number; markerTo: number; text: string };

function newBlockId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (character) => {
    const random = Math.floor(Math.random() * 16);
    return (character === "x" ? random : (random & 3) | 8).toString(16);
  });
}

function structuralLine(line: string) {
  return /^(?:#{1,6}\s|[-*+]\s|\d+\.\s|>\s|---\s*$)/.test(line);
}

export function ensureBlockIds(markdown: string, createId: () => string = newBlockId) {
  if (!markdown) return markdown;
  const lines = markdown.split("\n");
  const output: string[] = [];
  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (!line.trim() || BLOCK_LINE.test(line.trim())) {
      output.push(line);
      index += 1;
      continue;
    }
    const block: string[] = [line];
    if (/^\s*(```|~~~)/.test(line)) {
      const fence = line.trimStart().slice(0, 3);
      index += 1;
      while (index < lines.length) {
        block.push(lines[index]);
        const closesFence = lines[index].trimStart().startsWith(fence);
        index += 1;
        if (closesFence) break;
      }
    } else {
      index += 1;
      while (index < lines.length && lines[index].trim() && !BLOCK_LINE.test(lines[index].trim()) && !structuralLine(line) && !structuralLine(lines[index]) && !/^\s*(```|~~~)/.test(lines[index])) {
        block.push(lines[index]);
        index += 1;
      }
    }
    output.push(...block);
    if (index >= lines.length || !BLOCK_LINE.test(lines[index].trim())) output.push(`<!-- qy:block:${createId()} -->`);
  }
  return output.join("\n");
}

export function blocks(markdown: string) {
  const result: MarkdownBlock[] = [];
  let previousMarkerTo = 0;
  for (const match of markdown.matchAll(BLOCK_MARKER)) {
    const markerFrom = match.index;
    const markerTo = markerFrom + match[0].length;
    const raw = markdown.slice(previousMarkerTo, markerFrom);
    const text = raw.trim();
    const leading = raw.indexOf(text);
    if (text) result.push({ id: match[1], from: previousMarkerTo + Math.max(0, leading), to: markerFrom - (raw.length - raw.trimEnd().length), markerFrom, markerTo, text });
    previousMarkerTo = markerTo;
  }
  return result;
}

export function findBlock(markdown: string, id: string) {
  return blocks(markdown).find((block) => block.id === id) ?? null;
}

export function findBlockAt(markdown: string, position: number) {
  return blocks(markdown).find((block) => position >= block.from && position <= block.markerTo) ?? null;
}

const setBlockHighlight = StateEffect.define<{ from: number; to: number } | null>();
const blockHighlightField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update: (decorations, transaction) => {
    let next = decorations.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (effect.is(setBlockHighlight)) next = effect.value ? Decoration.set([Decoration.mark({ class: "qy-block-highlight" }).range(effect.value.from, effect.value.to)]) : Decoration.none;
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

const markerMatcher = new MatchDecorator({ regexp: /^<!-- qy:block:[0-9a-fA-F-]{36} -->$/gm, decoration: (match, view, position) => {
  const cursor = view.state.selection.main.head;
  return cursor >= position && cursor <= position + match[0].length ? null : Decoration.replace({});
} });

const hiddenMarkers = ViewPlugin.fromClass(class {
  decorations: DecorationSet;
  constructor(view: EditorView) { this.decorations = markerMatcher.createDeco(view); }
  update(update: ViewUpdate) { this.decorations = markerMatcher.updateDeco(update, this.decorations); }
}, { decorations: (value) => value.decorations });

export const blockIdentityExtensions = [blockHighlightField, hiddenMarkers];

export function revealBlock(view: EditorView, block: MarkdownBlock) {
  view.dispatch({ selection: { anchor: block.from }, effects: [setBlockHighlight.of({ from: block.from, to: block.to }), EditorView.scrollIntoView(block.from, { y: "center" })] });
  window.setTimeout(() => {
    if (view.dom.isConnected) view.dispatch({ effects: setBlockHighlight.of(null) });
  }, 1600);
}
