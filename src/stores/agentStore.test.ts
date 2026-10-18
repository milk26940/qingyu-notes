import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
  afterEach(() => {
    vi.useRealTimers();
  });

  beforeEach(() => {
    useAgentStore.setState({
      conversations: [], activeId: null, records: [], streamingText: "", plan: null,
      diff: "", tools: [], approvals: [], sending: false, error: null, queue: {}, queuePaused: {}, selectedModel: null, conversationModels: {},
    });
    Object.values(api).forEach((mock) => mock.mockReset());
    api.listConversations.mockResolvedValue([]);
    api.listApprovals.mockResolvedValue([]);
    api.getConversationHistory.mockResolvedValue([]);
    api.subscribeConversationEvents.mockResolvedValue(() => undefined);
    api.subscribeApprovals.mockResolvedValue(() => undefined);
    api.subscribePersistenceErrors.mockResolvedValue(() => undefined);
    api.interruptConversation.mockResolvedValue(undefined);
    api.resolveApproval.mockResolvedValue(undefined);
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
    expect(state.records.find((item) => item.kind === "agentMessageDelta")?.data.delta).toBe("你好");
    expect(state.records[state.records.length - 1]?.kind).toBe("turnCompleted");
    expect(state.plan).toEqual([{ step: "读取" }]);
    expect(state.diff).toBe("+ 新内容");
    expect(state.tools).toEqual([{ id: "tool-1", tool: "read", namespace: "page", status: "completed" }]);
    expect(state.sending).toBe(false);
  });

  it("上一轮请求应答落后于完成事件时仍串行发送，失败后暂停队列", async () => {
    let eventHandler: (event: any) => void = () => undefined;
    api.subscribeConversationEvents.mockImplementation(async (handler) => { eventHandler = handler; return () => undefined; });
    const conversation = { id: "queue", threadId: "thread", title: "队列", scope: { kind: "vault" as const }, createdAt: "0", updatedAt: "0", activeTurnId: null };
    api.listConversations.mockResolvedValue([conversation]);
    useAgentStore.setState({ conversations: [conversation], activeId: "queue" });
    await useAgentStore.getState().initialize();
    let resolveFirst!: (value: { threadId: string; turnId: string }) => void;
    api.sendConversationMessage.mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve; })).mockResolvedValue({ threadId: "thread", turnId: "second" });
    const pending = useAgentStore.getState().send("第一轮");
    useAgentStore.getState().enqueue({ requestText: "第二轮" });
    eventHandler({ kind: "turnStarted", threadId: "thread", turn: { id: "first" } });
    eventHandler({ kind: "turnCompleted", threadId: "thread", turn: { id: "first", status: "completed" } });
    await Promise.resolve();
    expect(api.sendConversationMessage).toHaveBeenCalledTimes(1);
    resolveFirst({ threadId: "thread", turnId: "first" });
    await pending;
    await vi.waitFor(() => expect(api.sendConversationMessage).toHaveBeenCalledTimes(2));
    expect(useAgentStore.getState().conversations[0].activeTurnId).toBe("second");
    useAgentStore.getState().enqueue({ requestText: "第三轮" });
    eventHandler({ kind: "turnCompleted", threadId: "thread", turn: { id: "second", status: "failed", error: { message: "连接失败" } } });
    await Promise.resolve();
    expect(useAgentStore.getState().queuePaused.queue).toBe(true);
    expect(useAgentStore.getState().queue.queue).toHaveLength(1);
  });

  it("队列准备失败保留待发项目并恢复历史，重试接受后才出队", async () => {
    useAgentStore.setState({ activeId: "queue", records: [] });
    api.sendConversationMessage.mockRejectedValueOnce(new Error("密钥未配置"));
    useAgentStore.getState().enqueue({ requestText: "保留这条消息" });
    await useAgentStore.getState().sendNext();
    expect(useAgentStore.getState().queue.queue).toHaveLength(1);
    expect(useAgentStore.getState().records).toEqual([]);
    api.sendConversationMessage.mockResolvedValue({ threadId: "thread", turnId: "retry" });
    useAgentStore.setState({ queuePaused: {} });
    await useAgentStore.getState().sendNext();
    expect(useAgentStore.getState().queue.queue).toHaveLength(0);
    expect(useAgentStore.getState().records.filter((item) => item.kind === "userMessage")).toHaveLength(1);
  });

  it("区分上游失败和主动中止", async () => {
    let eventHandler: (event: any) => void = () => undefined;
    api.subscribeConversationEvents.mockImplementation(async (handler) => { eventHandler = handler; return () => undefined; });
    const conversation = { id: "conversation-1", threadId: "thread-1", title: "测试", scope: { kind: "vault" as const }, createdAt: "0", updatedAt: "0", activeTurnId: "turn-1" };
    api.listConversations.mockResolvedValue([conversation]);
    useAgentStore.setState({ conversations: [conversation], activeId: "conversation-1", sending: true, error: null });
    await useAgentStore.getState().initialize();

    eventHandler({ kind: "turnCompleted", threadId: "thread-1", turn: { id: "turn-1", status: "failed", error: { message: "上游连接中断" } } });
    expect(useAgentStore.getState().error).toBe("上游连接中断");

    useAgentStore.setState({ sending: true, error: null, conversations: [{ ...conversation, activeTurnId: "turn-2" }] });
    eventHandler({ kind: "turnCompleted", threadId: "thread-1", turn: { id: "turn-2", status: "interrupted", error: null } });
    expect(useAgentStore.getState().error).toBeNull();
  });

  it("等待审批时暂停无响应计时器", async () => {
    vi.useFakeTimers();
    let eventHandler: (event: any) => void = () => undefined;
    let approvalHandler: (approval: any) => void = () => undefined;
    api.subscribeConversationEvents.mockImplementation(async (handler) => { eventHandler = handler; return () => undefined; });
    api.subscribeApprovals.mockImplementation(async (handler) => { approvalHandler = handler; return () => undefined; });
    const conversation = { id: "conversation-1", threadId: "thread-1", title: "测试", scope: { kind: "vault" as const }, createdAt: "0", updatedAt: "0", activeTurnId: "turn-1" };
    api.listConversations.mockResolvedValue([conversation]);
    useAgentStore.setState({ conversations: [conversation], activeId: "conversation-1", sending: true, error: null });
    await useAgentStore.getState().initialize();

    eventHandler({ kind: "itemStarted", threadId: "thread-1", turnId: "turn-1", item: { id: "tool-1", tool: "update", namespace: "page" } });
    approvalHandler({ id: "approval-1", requestId: 1, threadId: "thread-1", turnId: "turn-1", namespace: "page", tool: "update", arguments: {}, diff: "+ 内容" });
    await vi.advanceTimersByTimeAsync(300_000);

    expect(api.interruptConversation).not.toHaveBeenCalled();
    expect(useAgentStore.getState().sending).toBe(true);
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
    await vi.waitFor(() =>
      expect(api.sendConversationMessage).toHaveBeenCalledWith(
        "conversation-1",
        "只返回候选正文",
        [],
        undefined,
        undefined,
        null,
      ),
    );
    eventHandler({ kind: "agentMessageDelta", threadId: "thread-1", turnId: "turn-2", itemId: "item-1", delta: "候选" });
    eventHandler({ kind: "agentMessageDelta", threadId: "thread-1", turnId: "turn-2", itemId: "item-1", delta: "正文" });
    eventHandler({ kind: "turnCompleted", threadId: "thread-1", turn: { id: "turn-2", status: "completed" } });

    await expect(pending).resolves.toBe("候选正文");
  });

  it("send 优先使用用户输入文字作为标题，没有文字时使用附件名，并保存 displayText 与 requestText", async () => {
    api.createConversation.mockResolvedValue({
      id: "conv-created",
      threadId: null,
      title: "附件.pdf",
      scope: { kind: "vault" },
      createdAt: "1",
      updatedAt: "1",
      activeTurnId: null,
    });
    api.sendConversationMessage.mockResolvedValue({ threadId: "th-1", turnId: "turn-1" });

    await useAgentStore.getState().send({
      requestText: "上下文内容",
      displayText: "",
      attachments: [{ id: "att-1", name: "附件.pdf", mime: "application/pdf", size: 100, relativePath: "attachments/1/att-1.pdf" }],
      scope: { kind: "vault" },
    });

    expect(api.createConversation).toHaveBeenCalledWith("附件.pdf", { kind: "vault" }, null);
    expect(api.sendConversationMessage).toHaveBeenCalledWith(
      "conv-created",
      "",
      expect.any(Array),
      "",
      undefined,
      null,
      [],
      undefined,
    );
    const record = useAgentStore.getState().records[0];
    expect(record.data.displayText).toBe("");
    expect(record.data.text).toBe("上下文内容");
  });

  it("支持为草稿和独立对话保存模型选择并在发送与新建时传递", async () => {
    // 尚未创建对话时设置模型
    useAgentStore.getState().setSelectedModel("gpt-4o");
    expect(useAgentStore.getState().selectedModel).toBe("gpt-4o");

    api.createConversation.mockResolvedValue({
      id: "conv-with-model",
      threadId: null,
      title: "新模型对话",
      scope: { kind: "vault" },
      createdAt: "1",
      updatedAt: "1",
      activeTurnId: null,
    });
    api.sendConversationMessage.mockResolvedValue({ threadId: "th-2", turnId: "turn-2" });

    // 新建对话关联该模型
    await useAgentStore.getState().create({ kind: "vault" }, "新模型对话");
    expect(api.createConversation).toHaveBeenCalledWith("新模型对话", { kind: "vault" }, "gpt-4o");
    expect(useAgentStore.getState().conversationModels["conv-with-model"]).toBe("gpt-4o");

    // 发送消息时传递该模型
    await useAgentStore.getState().send("你好");
    expect(api.sendConversationMessage).toHaveBeenCalledWith(
      "conv-with-model",
      "你好",
      [],
      "你好",
      undefined,
      "gpt-4o",
      [],
      undefined,
    );

    // 切换到另一个已有对话，模型选择同步切换
    api.getConversationHistory.mockResolvedValue([]);
    useAgentStore.setState({
      sending: false, conversationModels: { "conv-with-model": "gpt-4o", "conv-other": "claude-3-5-sonnet" },
    });
    await useAgentStore.getState().open("conv-other");
    expect(useAgentStore.getState().selectedModel).toBe("claude-3-5-sonnet");

    // 切换对话后随时修改模型，下一轮生效
    useAgentStore.getState().setSelectedModel("o3-mini");
    expect(useAgentStore.getState().conversationModels["conv-other"]).toBe("o3-mini");
  });
});
