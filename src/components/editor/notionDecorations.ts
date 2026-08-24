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
    void readPageAsset(this.path).then(({ mime, bytes }) => {
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

function createDecorations(view: EditorView) {
  const ranges: ReturnType<Decoration["range"]>[] = [];
  const cursor = view.state.selection.main.head;
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
      } else if (name === "Image" && !active) {
        const source = view.state.doc.sliceString(node.from, node.to);
        const match = /^!\[([^\]]*)\]\((\/\.noteapp\/assets\/[^)]+)\)$/.exec(source);
        if (match) ranges.push(Decoration.replace({ widget: new ImageWidget(match[2], match[1] || "图片") }).range(node.from, node.to));
      } else if ((name === "HeaderMark" || name === "EmphasisMark" || name === "QuoteMark" || name === "ListMark") && !active) {
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
