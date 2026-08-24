import { create } from "zustand";
import * as api from "../lib/tauri/conversations";
import type { CodexItemSummary, CodexStreamEvent, ConversationMetadata, ConversationRecord, ConversationScope, PendingApproval } from "../lib/tauri/conversations";

type AgentStore = {
  conversations: ConversationMetadata[];
  activeId: string | null;
  records: ConversationRecord[];
  streamingText: string;
  plan: unknown;
  diff: string;
  tools: CodexItemSummary[];
  approvals: PendingApproval[];
  sending: boolean;
  error: string | null;
  initialize: () => Promise<() => void>;
  create: (scope: ConversationScope, title?: string) => Promise<ConversationMetadata | null>;
  open: (id: string) => Promise<void>;
  send: (text: string, defaultScope: ConversationScope) => Promise<void>;
  requestInline: (pageId: string, pageTitle: string, prompt: string) => Promise<string>;
  interrupt: () => Promise<void>;
  resolveApproval: (id: string, approved: boolean) => Promise<void>;
};

const errorMessage = (error: unknown) => typeof error === "object" && error && "message" in error ? String(error.message) : String(error);
const record = (kind: string, data: Record<string, unknown>): ConversationRecord => ({ kind, data, timestamp: String(Date.now()) });

export function projectConversationHistory(records: ConversationRecord[]) {
  let plan: unknown = null;
  let diff = "";
  const tools: CodexItemSummary[] = [];
  for (const item of records) {
    if (item.kind === "planUpdated") plan = item.data.plan ?? null;
    if (item.kind === "diffUpdated" || item.kind === "changeSummary" || item.kind === "approvalRequested") diff = String(item.data.diff ?? "");
    if (item.kind === "itemStarted" || item.kind === "itemCompleted") {
      const summary = item.data.item as CodexItemSummary | undefined;
      const existing = summary?.id ? tools.findIndex((entry) => entry.id === summary.id) : -1;
      if (existing >= 0) tools.splice(existing, 1);
      if (summary) tools.push(summary);
    }
  }
  return { plan, diff, tools };
}

export const useAgentStore = create<AgentStore>((set, get) => ({
  conversations: [], activeId: null, records: [], streamingText: "", plan: null, diff: "", tools: [], approvals: [], sending: false, error: null,
  initialize: async () => {
    const conversations = await api.listConversations();
    const approvals = await api.listApprovals();
    set({ conversations, approvals });
    if (!get().activeId && conversations[0]) await get().open(conversations[0].id);
    const handle = (event: CodexStreamEvent) => {
      const active = get().conversations.find((item) => item.id === get().activeId);
      if ("threadId" in event && active?.threadId !== event.threadId) return;
      if (event.kind === "agentMessageDelta") set((state) => ({ streamingText: state.streamingText + event.delta, records: [...state.records, record("agentMessageDelta", { delta: event.delta, turnId: event.turnId })] }));
      if (event.kind === "turnPlanUpdated") set({ plan: event.plan });
      if (event.kind === "turnDiffUpdated") set({ diff: event.diff });
      if (event.kind === "itemStarted" || event.kind === "itemCompleted") set((state) => ({ tools: [...state.tools.filter((item) => item.id !== event.item.id), event.item] }));
      if (event.kind === "turnCompleted") { set({ sending: false }); const id = get().activeId; if (id) void get().open(id); }
    };
    const unlistenEvents = await api.subscribeConversationEvents(handle);
    const unlistenApprovals = await api.subscribeApprovals((approval) => set((state) => ({ approvals: [...state.approvals.filter((item) => item.id !== approval.id), approval] })));
    const unlistenPersistenceErrors = await api.subscribePersistenceErrors((failure) => set({ error: `对话历史写入失败：${failure.message}` }));
    return () => { unlistenEvents(); unlistenApprovals(); unlistenPersistenceErrors(); };
  },
  create: async (scope, title = "新对话") => { try { const conversation = await api.createConversation(title, scope); set((state) => ({ conversations: [conversation, ...state.conversations], activeId: conversation.id, records: [], streamingText: "", plan: null, diff: "", tools: [], error: null })); return conversation; } catch (error) { set({ error: errorMessage(error) }); return null; } },
  open: async (id) => { try { const records = await api.getConversationHistory(id); const projection = projectConversationHistory(records); set({ activeId: id, records, streamingText: "", ...projection, error: null }); } catch (error) { set({ error: errorMessage(error) }); } },
  send: async (text, defaultScope) => { let id = get().activeId; if (!id) id = (await get().create(defaultScope, text.slice(0, 24) || "新对话"))?.id ?? null; if (!id) return; set((state) => ({ sending: true, streamingText: "", records: [...state.records, record("userMessage", { text })], error: null })); try { const turn = await api.sendConversationMessage(id, text); set((state) => ({ conversations: state.conversations.map((item) => item.id === id ? { ...item, threadId: turn.threadId, activeTurnId: turn.turnId } : item) })); } catch (error) { set({ sending: false, error: errorMessage(error) }); } },
  requestInline: async (pageId, pageTitle, prompt) => {
    let conversation = [...get().conversations]
      .filter((item) => item.scope.kind === "page" && item.scope.pageId === pageId)
      .sort((left, right) => Number(right.updatedAt) - Number(left.updatedAt))[0];
    if (!conversation) {
      conversation = await api.createConversation(`写作 · ${pageTitle}`, { kind: "page", pageId });
      set((state) => ({ conversations: [conversation!, ...state.conversations] }));
    }

    let expected: { threadId: string; turnId: string } | null = null;
    let output = "";
    const buffered: CodexStreamEvent[] = [];
    let resolveResult: (value: string) => void = () => undefined;
    let rejectResult: (error: Error) => void = () => undefined;
    let timeout = 0;
    const result = new Promise<string>((resolve, reject) => {
      resolveResult = resolve;
      rejectResult = reject;
      timeout = window.setTimeout(() => reject(new Error("Agent 生成超时")), 120_000);
    });
    const process = (event: CodexStreamEvent) => {
      if (!expected || !("threadId" in event) || event.threadId !== expected.threadId) return;
      if (event.kind === "agentMessageDelta" && event.turnId === expected.turnId) output += event.delta;
      if (event.kind === "turnCompleted" && String(event.turn.id ?? "") === expected.turnId) {
        window.clearTimeout(timeout);
        if (String(event.turn.status ?? "completed") === "completed") resolveResult(output);
        else rejectResult(new Error(String(event.turn.error ?? "Agent 生成失败")));
      }
    };
    let unlisten: () => void = () => undefined;
    try {
      unlisten = await api.subscribeConversationEvents((event) => {
        if (!expected) buffered.push(event);
        else process(event);
      });
      expected = await api.sendConversationMessage(conversation.id, prompt);
      set((state) => ({ conversations: state.conversations.map((item) => item.id === conversation!.id ? { ...item, threadId: expected!.threadId, activeTurnId: expected!.turnId, updatedAt: String(Math.floor(Date.now() / 1000)) } : item) }));
      buffered.forEach(process);
      return await result;
    } finally {
      window.clearTimeout(timeout);
      unlisten();
      const conversationId = conversation.id;
      set((state) => ({ conversations: state.conversations.map((item) => item.id === conversationId ? { ...item, activeTurnId: null } : item) }));
    }
  },
  interrupt: async () => { const active = get().conversations.find((item) => item.id === get().activeId); if (!active?.threadId || !active.activeTurnId) return; try { await api.interruptConversation(active.threadId, active.activeTurnId); set({ sending: false }); } catch (error) { set({ error: errorMessage(error) }); } },
  resolveApproval: async (id, approved) => { try { await api.resolveApproval(id, approved); set((state) => ({ approvals: state.approvals.filter((item) => item.id !== id) })); } catch (error) { set({ error: errorMessage(error) }); } },
}));
