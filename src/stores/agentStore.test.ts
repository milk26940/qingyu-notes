import { beforeEach, describe, expect, it, vi } from "vitest";
import { projectConversationHistory, useAgentStore } from "./agentStore";

const api = vi.hoisted(() => ({
  listConversations: vi.fn(),
  listApprovals: vi.fn(),
  getConversationHistory: vi.fn(),
  subscribeConversationEvents: vi.fn(),
  subscribeApprovals: vi.fn(),
  subscribePersistenceErrors: vi.fn(),
  createConversation: vi.fn(),
  sendConversationMessage: vi.fn(),
  interruptConversation: vi.fn(),
  resolveApproval: vi.fn(),
}));

vi.mock("../lib/tauri/conversations", () => ({
  ...api,
}));

describe("useAgentStore", () => {
  beforeEach(() => {
    useAgentStore.setState({
      conversations: [], activeId: null, records: [], streamingText: "", plan: null,
      diff: "", tools: [], approvals: [], sending: false, error: null,
    });
    Object.values(api).forEach((mock) => mock.mockReset());
    api.listConversations.mockResolvedValue([]);
    api.listApprovals.mockResolvedValue([]);
    api.getConversationHistory.mockResolvedValue([]);
    api.subscribeConversationEvents.mockResolvedValue(() => undefined);
    api.subscribeApprovals.mockResolvedValue(() => undefined);
    api.subscribePersistenceErrors.mockResolvedValue(() => undefined);
  });

  it("projects streaming, plan, diff, tool progress and completion", async () => {
    let eventHandler: (event: any) => void = () => undefined;
    api.subscribeConversationEvents.mockImplementation(async (handler) => { eventHandler = handler; return () => undefined; });
    const conversation = { id: "conversation-1", threadId: "thread-1", title: "测试", scope: { kind: "vault" as const }, createdAt: "0", updatedAt: "0", activeTurnId: "turn-1" };
    api.listConversations.mockResolvedValue([conversation]);
    useAgentStore.setState({
      conversations: [conversation],
      activeId: "conversation-1", sending: true,
    });
    await useAgentStore.getState().initialize();

    eventHandler({ kind: "agentMessageDelta", threadId: "thread-1", turnId: "turn-1", itemId: "item-1", delta: "你好" });
    eventHandler({ kind: "turnPlanUpdated", threadId: "thread-1", turnId: "turn-1", plan: [{ step: "读取" }] });
    eventHandler({ kind: "turnDiffUpdated", threadId: "thread-1", turnId: "turn-1", diff: "+ 新内容" });
    eventHandler({ kind: "itemStarted", threadId: "thread-1", turnId: "turn-1", item: { id: "tool-1", tool: "read", namespace: "page", status: "inProgress" } });
    eventHandler({ kind: "itemCompleted", threadId: "thread-1", turnId: "turn-1", item: { id: "tool-1", tool: "read", namespace: "page", status: "completed" } });
    eventHandler({ kind: "turnCompleted", threadId: "thread-1", turn: { id: "turn-1", status: "completed" } });

    const state = useAgentStore.getState();
    expect(state.records[state.records.length - 1]?.data.delta).toBe("你好");
    expect(state.plan).toEqual([{ step: "读取" }]);
    expect(state.diff).toBe("+ 新内容");
    expect(state.tools).toEqual([{ id: "tool-1", tool: "read", namespace: "page", status: "completed" }]);
    expect(state.sending).toBe(false);
  });

  it("restores the latest visible state from JSONL records", () => {
    expect(projectConversationHistory([
      { kind: "planUpdated", timestamp: "1", data: { plan: ["步骤"] } },
      { kind: "diffUpdated", timestamp: "2", data: { diff: "+ 修改" } },
      { kind: "itemCompleted", timestamp: "3", data: { item: { id: "tool-1", status: "completed" } } },
    ])).toEqual({ plan: ["步骤"], diff: "+ 修改", tools: [{ id: "tool-1", status: "completed" }] });
  });

  it("collects an inline writing response in the matching page conversation", async () => {
    let eventHandler: (event: any) => void = () => undefined;
    api.subscribeConversationEvents.mockImplementation(async (handler) => { eventHandler = handler; return () => undefined; });
    api.sendConversationMessage.mockResolvedValue({ threadId: "thread-1", turnId: "turn-2" });
    useAgentStore.setState({
      conversations: [{ id: "conversation-1", threadId: "thread-1", title: "页面写作", scope: { kind: "page", pageId: "page-1" }, createdAt: "1", updatedAt: "2", activeTurnId: null }],
    });

    const pending = useAgentStore.getState().requestInline("page-1", "规划", "只返回候选正文");
    await vi.waitFor(() => expect(api.sendConversationMessage).toHaveBeenCalledWith("conversation-1", "只返回候选正文"));
    eventHandler({ kind: "agentMessageDelta", threadId: "thread-1", turnId: "turn-2", itemId: "item-1", delta: "候选" });
    eventHandler({ kind: "agentMessageDelta", threadId: "thread-1", turnId: "turn-2", itemId: "item-1", delta: "正文" });
    eventHandler({ kind: "turnCompleted", threadId: "thread-1", turn: { id: "turn-2", status: "completed" } });

    await expect(pending).resolves.toBe("候选正文");
  });
});
