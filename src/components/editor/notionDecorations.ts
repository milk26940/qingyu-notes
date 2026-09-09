import { syntaxTree } from "@codemirror/language";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { readPageAsset } from "../../lib/tauri/pages";

class ImageWidget extends WidgetType {
  private objectUrl: string | null = null;
  private disposed = false;

  constructor(private path: string, private alt: string) { super(); }

  eq(other: ImageWidget) { return this.path === other.path && this.alt === other.alt; }

  toDOM() {
    const container = document.createElement("span");
    container.className = "qy-image-widget";
    container.textContent = "图片加载中";
    void Promise.resolve().then(() => readPageAsset(this.path)).then(({ mime, bytes }) => {
      if (this.disposed) return;
      this.objectUrl = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: mime }));
      const image = document.createElement("img");
      image.src = this.objectUrl;
      image.alt = this.alt;
      container.replaceChildren(image);
    }).catch(() => {
      if (!this.disposed) { container.className = "qy-image-widget qy-image-missing"; container.textContent = `图片不可用：${this.alt}`; }
    });
    return container;
  }

  destroy() {
    this.disposed = true;
    if (this.objectUrl) URL.revokeObjectURL(this.objectUrl);
  }
}

class DividerWidget extends WidgetType {
  toDOM() { const element = document.createElement("hr"); element.className = "qy-divider-widget"; return element; }
}

function createDecorations(view: EditorView) {
  const ranges: ReturnType<Decoration["range"]>[] = [];
  const cursor = view.state.selection.main.head;
  const markdown = view.state.doc.toString();
  const imagePattern = /!\[([^\]]*)\]\((\/.noteapp\/assets\/[^)\s]+)\)/g;
  for (const match of markdown.matchAll(imagePattern)) {
    if (match.index == null) continue;
    ranges.push(Decoration.replace({ widget: new ImageWidget(match[2], match[1] || "图片") }).range(match.index, match.index + match[0].length));
  }
  for (let number = 1; number <= view.state.doc.lines; number += 1) {
    const line = view.state.doc.line(number);
    if (/^\s*---\s*$/.test(line.text) && !(cursor >= line.from && cursor <= line.to)) {
      ranges.push(Decoration.replace({ widget: new DividerWidget() }).range(line.from, line.to));
    }
    if (/^\s*<\/?aside[^>]*>\s*$/i.test(line.text)) {
      ranges.push(Decoration.line({ class: "qy-aside-line" }).range(line.from));
    }
  }
  syntaxTree(view.state).iterate({
    enter: (node) => {
      const name = node.name;
      const active = cursor >= node.from && cursor <= node.to;
      if (/^ATXHeading[1-6]$/.test(name)) {
        const level = name.slice(-1);
        ranges.push(Decoration.line({ class: `qy-heading qy-heading-${level}` }).range(view.state.doc.lineAt(node.from).from));
      } else if (name === "StrongEmphasis") {
        ranges.push(Decoration.mark({ class: "qy-strong" }).range(node.from, node.to));
      } else if (name === "Emphasis") {
        ranges.push(Decoration.mark({ class: "qy-emphasis" }).range(node.from, node.to));
      } else if (name === "ListItem") {
        ranges.push(Decoration.line({ class: "qy-list-item" }).range(view.state.doc.lineAt(node.from).from));
      } else if (name === "Blockquote") {
        ranges.push(Decoration.line({ class: "qy-blockquote" }).range(view.state.doc.lineAt(node.from).from));
      } else if (name === "InlineCode") {
        ranges.push(Decoration.mark({ class: "qy-inline-code" }).range(node.from, node.to));
      } else if (name === "FencedCode") {
        ranges.push(Decoration.mark({ class: "qy-code-block" }).range(node.from, node.to));
        if (/^```mermaid\b/i.test(view.state.doc.sliceString(node.from, node.to))) {
          ranges.push(Decoration.mark({ class: "qy-mermaid-widget" }).range(node.from, node.to));
        }
      } else if (name === "HeaderMark") {
        ranges.push(Decoration.replace({}).range(node.from, node.to));
      } else if ((name === "EmphasisMark" || name === "QuoteMark" || name === "ListMark") && !active) {
        const parent = node.node.parent;
        const parentActive = parent ? cursor >= parent.from && cursor <= parent.to : false;
        if (!parentActive) ranges.push(Decoration.replace({}).range(node.from, node.to));
      }
    },
  });
  return Decoration.set(ranges, true);
}

export const notionDecorations = ViewPlugin.fromClass(class {
  decorations: DecorationSet;
  constructor(view: EditorView) { this.decorations = createDecorations(view); }
  update(update: ViewUpdate) {
    if (update.docChanged || update.selectionSet || update.viewportChanged) this.decorations = createDecorations(update.view);
  }
}, { decorations: (value) => value.decorations });
