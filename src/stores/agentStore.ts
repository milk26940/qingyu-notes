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
  interrupt: () => Promise<void>;
  resolveApproval: (id: string, approved: boolean) => Promise<void>;
};

const errorMessage = (error: unknown) => typeof error === "object" && error && "message" in error ? String(error.message) : String(error);
const record = (kind: string, data: Record<string, unknown>): ConversationRecord => ({ kind, data, timestamp: String(Date.now()) });

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
  open: async (id) => { try { const records = await api.getConversationHistory(id); set({ activeId: id, records, streamingText: "", plan: null, diff: "", tools: [], error: null }); } catch (error) { set({ error: errorMessage(error) }); } },
  send: async (text, defaultScope) => { let id = get().activeId; if (!id) id = (await get().create(defaultScope, text.slice(0, 24) || "新对话"))?.id ?? null; if (!id) return; set((state) => ({ sending: true, streamingText: "", records: [...state.records, record("userMessage", { text })], error: null })); try { const turn = await api.sendConversationMessage(id, text); set((state) => ({ conversations: state.conversations.map((item) => item.id === id ? { ...item, threadId: turn.threadId, activeTurnId: turn.turnId } : item) })); } catch (error) { set({ sending: false, error: errorMessage(error) }); } },
  interrupt: async () => { const active = get().conversations.find((item) => item.id === get().activeId); if (!active?.threadId || !active.activeTurnId) return; try { await api.interruptConversation(active.threadId, active.activeTurnId); set({ sending: false }); } catch (error) { set({ error: errorMessage(error) }); } },
  resolveApproval: async (id, approved) => { try { await api.resolveApproval(id, approved); set((state) => ({ approvals: state.approvals.filter((item) => item.id !== id) })); } catch (error) { set({ error: errorMessage(error) }); } },
}));
