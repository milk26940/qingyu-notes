import { autocompletion, type CompletionContext, type CompletionSource } from "@codemirror/autocomplete";
import { Decoration, EditorView, MatchDecorator, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import type { PageSummary } from "../../lib/tauri/pages";

export type PageMentionConfig = {
  getPages: () => PageSummary[];
  onOpenPage: (id: string) => void;
  onMissingPage: () => void;
};

function mentionMarkdown(page: PageSummary) {
  const label = page.title.replace(/[\[\]]/g, "").trim() || "未命名页面";
  return `[${label}](qingyu://page/${page.id})`;
}

export function pageMentionOptions(query: string, pages: PageSummary[]) {
  const needle = query.trim().toLocaleLowerCase("zh-CN");
  return pages
    .filter((page) => !needle || page.title.toLocaleLowerCase("zh-CN").includes(needle) || page.path.toLocaleLowerCase("zh-CN").includes(needle))
    .slice(0, 40)
    .map((page) => ({ label: page.title, detail: page.path.replace(/\.md$/i, ""), type: "text", apply: mentionMarkdown(page) }));
}

function mentionCompletionSource(getPages: () => PageSummary[]): CompletionSource {
  return (context: CompletionContext) => {
    const match = context.matchBefore(/@[^\s@]*/);
    if (!match) return null;
    return { from: match.from, options: pageMentionOptions(match.text.slice(1), getPages()), validFor: /^@[^\s@]*$/ };
  };
}

export function pageMentionExtensions(config: PageMentionConfig) {
  const matcher = new MatchDecorator({
    regexp: /\[[^\]\n]+\]\(qingyu:\/\/page\/([^)#\s]+)(?:#[^)]+)?\)/g,
    decoration: (match) => {
      const id = match[1];
      const exists = config.getPages().some((page) => page.id === id);
      return Decoration.mark({ class: exists ? "qy-page-mention" : "qy-page-mention qy-page-mention-missing", attributes: { "data-page-id": id } });
    },
  });
  const mentionPlugin = ViewPlugin.fromClass(class {
    decorations: DecorationSet;
    constructor(view: EditorView) { this.decorations = matcher.createDeco(view); }
    update(update: ViewUpdate) { this.decorations = matcher.updateDeco(update, this.decorations); }
  }, { decorations: (value) => value.decorations });

  return [
    autocompletion({ override: [mentionCompletionSource(config.getPages)] }),
    mentionPlugin,
    EditorView.domEventHandlers({
      click: (event) => {
        const element = event.target instanceof Element ? event.target.closest<HTMLElement>(".qy-page-mention") : null;
        const id = element?.dataset.pageId;
        if (!id) return false;
        event.preventDefault();
        if (config.getPages().some((page) => page.id === id)) config.onOpenPage(id);
        else config.onMissingPage();
        return true;
      },
    }),
  ];
}
