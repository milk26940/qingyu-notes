import { create } from "zustand";
import * as api from "../lib/tauri/conversations";
import type { CodexItemSummary, CodexStreamEvent, ConversationAttachment, ConversationMetadata, ConversationRecord, ConversationScope, PendingApproval } from "../lib/tauri/conversations";

export type SendMessageOptions = {
  requestText: string;
  displayText?: string;
  selectedText?: string;
  attachments?: ConversationAttachment[];
  scope?: ConversationScope;
};

type AgentStore = {
  conversations: ConversationMetadata[];
  activeId: string | null;
  selectedModel: string | null;
  conversationModels: Record<string, string | null>;
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
  setSelectedModel: (model: string | null) => void;
  send: (
    input: string | SendMessageOptions,
    defaultScope?: ConversationScope,
    attachments?: ConversationAttachment[],
  ) => Promise<void>;
  uploadAttachments: (files: File[], defaultScope: ConversationScope) => Promise<ConversationAttachment[]>;
  uploadAttachmentPaths: (paths: string[], defaultScope: ConversationScope) => Promise<ConversationAttachment[]>;
  requestInline: (pageId: string, pageTitle: string, prompt: string) => Promise<string>;
  interrupt: () => Promise<void>;
  resolveApproval: (id: string, approved: boolean) => Promise<void>;
};

const CONVERSATION_MODELS_STORAGE_KEY = "qingyu-conversation-models";
const DRAFT_MODEL_STORAGE_KEY = "qingyu-draft-model";
const AGENT_INACTIVITY_TIMEOUT_MS = 300_000;
const AGENT_TIMEOUT_MESSAGE = "Agent 连续 300 秒没有返回新内容，已停止本轮请求";

let activeTurnTimer: ReturnType<typeof setTimeout> | null = null;
let activeTurnStopReason: "timeout" | "user" | null = null;

function clearActiveTurnTimer(): void {
  if (activeTurnTimer !== null) {
    clearTimeout(activeTurnTimer);
    activeTurnTimer = null;
  }
}

function turnIdOf(event: CodexStreamEvent): string | null {
  if ("turnId" in event) return event.turnId;
  if (event.kind === "turnStarted" || event.kind === "turnCompleted") {
    return typeof event.turn.id === "string" ? event.turn.id : null;
  }
  return null;
}

function turnErrorMessage(turn: Record<string, unknown>): string {
  const error = turn.error;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message?: unknown }).message ?? "Agent 请求失败");
  }
  return "Agent 请求未正常完成";
}

function armActiveTurnTimer(threadId: string, turnId: string): void {
  clearActiveTurnTimer();
  activeTurnTimer = setTimeout(() => {
    const state = useAgentStore.getState();
    const active = state.conversations.find((item) => item.id === state.activeId);
    if (!state.sending || active?.activeTurnId !== turnId) return;
    activeTurnStopReason = "timeout";
    useAgentStore.setState((current) => ({
      sending: false,
      error: AGENT_TIMEOUT_MESSAGE,
      conversations: current.conversations.map((item) =>
        item.id === current.activeId ? { ...item, activeTurnId: null } : item
      ),
    }));
    void api.interruptConversation(threadId, turnId).catch(() => undefined);
  }, AGENT_INACTIVITY_TIMEOUT_MS);
}

function loadStoredConversationModels(): Record<string, string | null> {
  try {
    const raw = localStorage.getItem(CONVERSATION_MODELS_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      const result: Record<string, string | null> = {};
      for (const [key, value] of Object.entries(parsed)) {
        if (typeof value === "string" || value === null) {
          result[key] = value;
        }
      }
      return result;
    }
  } catch {
    // 容错损坏或旧数据
  }
  return {};
}

function persistConversationModels(models: Record<string, string | null>): void {
  try {
    localStorage.setItem(CONVERSATION_MODELS_STORAGE_KEY, JSON.stringify(models));
  } catch {
    // 忽略存储失败
  }
}

function loadStoredDraftModel(): string | null {
  try {
    const val = localStorage.getItem(DRAFT_MODEL_STORAGE_KEY);
    return typeof val === "string" && val.trim() ? val : null;
  } catch {
    return null;
  }
}

function persistDraftModel(model: string | null): void {
  try {
    if (model) {
      localStorage.setItem(DRAFT_MODEL_STORAGE_KEY, model);
    } else {
      localStorage.removeItem(DRAFT_MODEL_STORAGE_KEY);
    }
  } catch {
    // 忽略存储失败
  }
}

const errorMessage = (error: unknown) => typeof error === "object" && error && "message" in error ? String(error.message) : String(error);
const record = (kind: string, data: Record<string, unknown>): ConversationRecord => ({ kind, data, timestamp: String(Date.now()) });
const progressItem = (kind: "itemStarted" | "itemCompleted", item?: CodexItemSummary) => {
  if (!item || (!item.tool && !item.namespace && !item.itemType && !item.id)) return null;
  return { ...item, status: item.status ?? (kind === "itemCompleted" ? "completed" : "inProgress") };
};

export function projectConversationHistory(records: ConversationRecord[]) {
  let plan: unknown = null;
  let diff = "";
  const tools: CodexItemSummary[] = [];
  for (const item of records) {
    if (item.kind === "planUpdated") plan = item.data.plan ?? null;
    if (item.kind === "diffUpdated" || item.kind === "changeSummary" || item.kind === "approvalRequested") diff = String(item.data.diff ?? "");
    if (item.kind === "itemStarted" || item.kind === "itemCompleted") {
      const summary = progressItem(item.kind, item.data.item as CodexItemSummary | undefined);
      const existing = summary?.id ? tools.findIndex((entry) => entry.id === summary.id) : -1;
      if (existing >= 0) tools.splice(existing, 1);
      if (summary) tools.push(summary);
    }
  }
  return { plan, diff, tools };
}

const initialConversationModels = loadStoredConversationModels();
const initialDraftModel = loadStoredDraftModel();

export const useAgentStore = create<AgentStore>((set, get) => ({
  conversations: [],
  activeId: null,
  selectedModel: initialDraftModel,
  conversationModels: initialConversationModels,
  records: [],
  streamingText: "",
  plan: null,
  diff: "",
  tools: [],
  approvals: [],
  sending: false,
  error: null,
  initialize: async () => {
    const conversations = await api.listConversations();
    const approvals = await api.listApprovals();
    set({ conversations, approvals });
    if (!get().activeId && conversations[0]) await get().open(conversations[0].id);
    let deltaTimer: ReturnType<typeof setTimeout> | null = null;
    let pending: Extract<CodexStreamEvent, { kind: "agentMessageDelta" }>[] = [];
    const flushDeltas = () => {
      if (deltaTimer !== null) clearTimeout(deltaTimer);
      deltaTimer = null;
      const deltas = pending; pending = [];
      const active = get().conversations.find((item) => item.id === get().activeId);
      const accepted = deltas.filter((event) => event.threadId === active?.threadId);
      if (!accepted.length) return;
      set((state) => {
        const records = [...state.records];
        for (const event of accepted) {
          const last = records[records.length - 1];
          if (last?.kind === "agentMessageDelta" && last.data.itemId === event.itemId && last.data.turnId === event.turnId) records[records.length - 1] = { ...last, data: { ...last.data, delta: String(last.data.delta ?? "") + event.delta } };
          else records.push(record("agentMessageDelta", { delta: event.delta, turnId: event.turnId, itemId: event.itemId }));
        }
        return { records, streamingText: state.streamingText + accepted.map((event) => event.delta).join("") };
      });
    };
    const handle = (event: CodexStreamEvent) => {
      const active = get().conversations.find((item) => item.id === get().activeId);
      if ("threadId" in event && active?.threadId !== event.threadId) return;
      const eventTurnId = turnIdOf(event);
      if ("threadId" in event && eventTurnId && get().sending) {
        armActiveTurnTimer(event.threadId, eventTurnId);
      }
      if (event.kind === "agentMessageDelta") {
        pending.push(event);
        if (deltaTimer === null) deltaTimer = setTimeout(flushDeltas, 40);
        return;
      }
      flushDeltas();
      if (event.kind === "turnPlanUpdated") set({ plan: event.plan });
      if (event.kind === "turnDiffUpdated") set({ diff: event.diff });
      if (event.kind === "itemStarted" || event.kind === "itemCompleted") {
        const item = progressItem(event.kind, event.item);
        if (item) set((state) => ({ tools: [...state.tools.filter((entry) => entry.id !== item.id), item] }));
      }
      if (event.kind === "turnCompleted") {
        clearActiveTurnTimer();
        const status = String(event.turn.status ?? "completed");
        const nextError = status === "failed" || status === "error"
          ? turnErrorMessage(event.turn)
          : status === "interrupted" && activeTurnStopReason === "timeout"
            ? get().error ?? AGENT_TIMEOUT_MESSAGE
            : null;
        activeTurnStopReason = null;
        set((state) => ({
          sending: false,
          error: nextError,
          conversations: state.conversations.map((item) =>
            item.id === state.activeId ? { ...item, activeTurnId: null } : item
          ),
        }));
      }
    };
    const unlistenEvents = await api.subscribeConversationEvents(handle);
    const unlistenApprovals = await api.subscribeApprovals((approval) => {
      const active = get().conversations.find((item) => item.id === get().activeId);
      if (active?.threadId === approval.threadId && active.activeTurnId === approval.turnId) {
        clearActiveTurnTimer();
      }
      set((state) => ({ approvals: [...state.approvals.filter((item) => item.id !== approval.id), approval] }));
    });
    const unlistenPersistenceErrors = await api.subscribePersistenceErrors((failure) => set({ error: `对话历史写入失败：${failure.message}` }));
    return () => { flushDeltas(); unlistenEvents(); unlistenApprovals(); unlistenPersistenceErrors(); };
  },
  setSelectedModel: (model) => {
    const activeId = get().activeId;
    if (activeId) {
      const conversationModels = { ...get().conversationModels, [activeId]: model };
      set({ selectedModel: model, conversationModels });
      persistConversationModels(conversationModels);
    } else {
      set({ selectedModel: model });
      persistDraftModel(model);
    }
  },
  create: async (scope, title = "新对话") => {
    try {
      const model = get().selectedModel;
      const conversation = await api.createConversation(title, scope, model);
      const conversationModels = { ...get().conversationModels, [conversation.id]: model };
      set((state) => ({ conversations: [conversation, ...state.conversations], activeId: conversation.id, selectedModel: model, conversationModels, records: [], streamingText: "", plan: null, diff: "", tools: [], error: null }));
      persistConversationModels(conversationModels);
      return conversation;
    } catch (error) { set({ error: errorMessage(error) }); return null; }
  },  open: async (id) => {
    try {
      const records = await api.getConversationHistory(id);
      const projection = projectConversationHistory(records);
      const model = get().conversationModels[id] ?? null;
      set({ activeId: id, selectedModel: model, records, streamingText: "", ...projection, error: null });
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },
  send: async (input, defaultScope, attachments = []) => {
    activeTurnStopReason = null;
    const isObject = typeof input === "object" && input !== null;
    const requestText = (isObject ? input.requestText : input) || "";
    const displayText = isObject ? (input.displayText ?? "") : input;
    const selectedText = isObject ? (input.selectedText ?? "") : "";
    const outgoingAttachments = (isObject ? input.attachments : attachments) ?? [];
    const scope = (isObject ? input.scope : defaultScope) ?? defaultScope ?? { kind: "vault" as const };

    let id = get().activeId;
    if (!id) {
      const title =
        displayText.trim().slice(0, 24) ||
        outgoingAttachments[0]?.name ||
        (selectedText.trim() ? "选中文本" : "新对话");
      id = (await get().create(scope, title))?.id ?? null;
    }
    if (!id) return;

    const model = get().conversationModels[id] ?? get().selectedModel ?? null;

    set((state) => ({
      sending: true,
      streamingText: "",
      records: [
        ...state.records,
        record("userMessage", {
          text: requestText,
          displayText,
          selectedText,
          attachments: outgoingAttachments,
        }),
      ],
      error: null,
    }));

    try {
      const turn = await api.sendConversationMessage(
        id,
        requestText,
        outgoingAttachments,
        displayText,
        selectedText || undefined,
        model,
      );
      const stillSending = get().sending;
      set((state) => ({
        conversations: state.conversations.map((item) =>
          item.id === id ? { ...item, threadId: turn.threadId, activeTurnId: stillSending ? turn.turnId : null } : item
        ),
      }));
      if (stillSending) armActiveTurnTimer(turn.threadId, turn.turnId);
    } catch (error) {
      clearActiveTurnTimer();
      set({ sending: false, error: errorMessage(error) });
    }
  },
  uploadAttachments: async (files, defaultScope) => {
    let id = get().activeId;
    if (!id) id = (await get().create(defaultScope, files[0]?.name || "附件对话"))?.id ?? null;
    if (!id) return [];
    try {
      const result = [] as ConversationAttachment[];
      for (const file of files) {
        if (file.size > 50 * 1024 * 1024) throw new Error(`${file.name} 超过 50 MB`);
        const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
        result.push(await api.storeConversationAttachment(id, file.name, file.type || "application/octet-stream", bytes));
      }
      set({ error: null });
      return result;
    } catch (error) { set({ error: errorMessage(error) }); return []; }
  },
  uploadAttachmentPaths: async (paths, defaultScope) => {
    let id = get().activeId;
    if (!id) id = (await get().create(defaultScope, paths[0]?.split(/[\\/]/).pop() || "附件对话"))?.id ?? null;
    if (!id) return [];
    try { const result = [] as ConversationAttachment[]; for (const path of paths) result.push(await api.storeConversationAttachmentPath(id, path)); set({ error: null }); return result; }
    catch (error) { set({ error: errorMessage(error) }); return []; }
  },
  requestInline: async (pageId, pageTitle, prompt) => {
    let conversation = [...get().conversations]
      .filter((item) => item.scope.kind === "page" && item.scope.pageId === pageId)
      .sort((left, right) => Number(right.updatedAt) - Number(left.updatedAt))[0];
    let model: string | null = null;
    if (!conversation) {
      model = get().selectedModel ?? null;
      conversation = await api.createConversation(`写作 · ${pageTitle}`, { kind: "page", pageId }, model);
      const conversationModels = { ...get().conversationModels, [conversation.id]: model };
      set((state) => ({
        conversations: [conversation!, ...state.conversations],
        conversationModels,
      }));
      persistConversationModels(conversationModels);
    } else {
      model = get().conversationModels[conversation.id] ?? null;
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
      expected = await api.sendConversationMessage(
        conversation.id,
        prompt,
        [],
        undefined,
        undefined,
        model,
      );
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
  interrupt: async () => { const active = get().conversations.find((item) => item.id === get().activeId); if (!active?.threadId || !active.activeTurnId) return; try { activeTurnStopReason = "user"; await api.interruptConversation(active.threadId, active.activeTurnId); clearActiveTurnTimer(); set({ sending: false, error: null }); } catch (error) { activeTurnStopReason = null; set({ error: errorMessage(error) }); } },
  resolveApproval: async (id, approved) => {
    const approval = get().approvals.find((item) => item.id === id);
    try {
      await api.resolveApproval(id, approved);
      set((state) => ({ approvals: state.approvals.filter((item) => item.id !== id) }));
      const active = get().conversations.find((item) => item.id === get().activeId);
      if (approval && get().sending && active?.threadId === approval.threadId && active.activeTurnId === approval.turnId) {
        armActiveTurnTimer(approval.threadId, approval.turnId);
      }
    } catch (error) { set({ error: errorMessage(error) }); }
  },
}));
