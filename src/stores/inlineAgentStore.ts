import { create } from "zustand";
import { useAgentStore } from "./agentStore";

export type InlineAgentAction = "generate" | "summarize" | "rewrite";
export type InlineAgentContext = { pageId: string; title: string; content: string; from: number; to: number };
export type InlineAgentCandidate = {
  pageId: string;
  action: InlineAgentAction;
  from: number;
  to: number;
  original: string;
  proposed: string;
  status: "requesting" | "ready";
};

const actionInstruction: Record<InlineAgentAction, string> = {
  generate: "根据选区与上下文续写正文",
  summarize: "把选区总结为简洁、准确的正文",
  rewrite: "改写选区，提升清晰度与表达质量，不改变事实",
};

function visibleMarkdown(value: string) {
  return value.replace(/^<!-- qy:block:[0-9a-fA-F-]{36} -->\s*$/gm, "").trim();
}

export function inlineAgentPrompt(action: InlineAgentAction, context: InlineAgentContext) {
  const before = visibleMarkdown(context.content.slice(Math.max(0, context.from - 2000), context.from));
  const selected = visibleMarkdown(context.content.slice(context.from, context.to));
  const after = visibleMarkdown(context.content.slice(context.to, context.to + 2000));
  return [
    `任务：${actionInstruction[action]}。`,
    "不要调用任何工具，不要读取其他文件或页面。只返回可直接写入 Markdown 的候选正文，不要解释，不要使用代码围栏。",
    `页面标题：${context.title}`,
    `前文：\n${before || "（无）"}`,
    `选区：\n${selected || "（无选区）"}`,
    `后文：\n${after || "（无）"}`,
  ].join("\n\n");
}

function cleanCandidate(value: string) {
  const trimmed = value.trim();
  if (!trimmed.startsWith("```") || !trimmed.endsWith("```")) return trimmed;
  return trimmed.split("\n").slice(1, -1).join("\n").trim();
}

type InlineAgentStore = {
  candidate: InlineAgentCandidate | null;
  error: string | null;
  request: (action: InlineAgentAction, context: InlineAgentContext) => Promise<void>;
  accept: () => InlineAgentCandidate | null;
  reject: () => void;
};

export const useInlineAgentStore = create<InlineAgentStore>((set, get) => ({
  candidate: null,
  error: null,
  request: async (action, context) => {
    const inserting = action === "generate";
    const from = inserting ? context.to : context.from;
    const to = inserting ? context.to : context.to;
    const original = inserting ? "" : context.content.slice(from, to);
    set({ candidate: { pageId: context.pageId, action, from, to, original, proposed: "", status: "requesting" }, error: null });
    try {
      const proposed = cleanCandidate(await useAgentStore.getState().requestInline(context.pageId, context.title, inlineAgentPrompt(action, context)));
      if (!proposed) throw new Error("Agent 未返回候选正文");
      set((state) => state.candidate?.pageId === context.pageId ? { candidate: { ...state.candidate, proposed, status: "ready" }, error: null } : state);
    } catch (error) {
      set({ candidate: null, error: error instanceof Error ? error.message : String(error) });
    }
  },
  accept: () => { const candidate = get().candidate?.status === "ready" ? get().candidate : null; set({ candidate: null, error: null }); return candidate; },
  reject: () => set({ candidate: null, error: null }),
}));
