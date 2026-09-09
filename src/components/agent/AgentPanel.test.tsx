import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AgentPanel } from "./AgentPanel";

const openConversation = vi.hoisted(() => vi.fn());
const uploadAttachments = vi.hoisted(() => vi.fn());
const send = vi.hoisted(() => vi.fn());
const interrupt = vi.hoisted(() => vi.fn());
const readConversationAttachment = vi.hoisted(() => vi.fn());
const invoke = vi.hoisted(() => vi.fn());
const setSelectedModel = vi.hoisted(() => vi.fn());
const loadModels = vi.hoisted(() => vi.fn());
const openPage = vi.hoisted(() => vi.fn());
let mockSending = false;
let mockRecords: any[] = [];
let mockSelectedModel: string | null = null;
let mockModels: any[] = [];
let mockModelsLoading = false;
let mockModelsError: string | null = null;
const conversations = vi.hoisted(() => [
  { id: "conversation-1", threadId: "thread-1", title: "当前对话", scope: { kind: "vault" }, createdAt: "1787570000", updatedAt: "1787570000", activeTurnId: null },
  { id: "conversation-2", threadId: "thread-2", title: "项目规划", scope: { kind: "page", pageId: "page-1" }, createdAt: "1787570100", updatedAt: "1787570200", activeTurnId: null },
]);

vi.mock("../../lib/tauri/conversations", async (importOriginal) => ({ ...(await importOriginal<object>()), readConversationAttachment }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

vi.mock("../../stores/agentStore", () => ({
  useAgentStore: (selector?: (state: unknown) => unknown) => {
    const state = {
      conversations,
      activeId: "conversation-1",
      selectedModel: mockSelectedModel,
      setSelectedModel,
      records: mockRecords,
      plan: null,
      diff: "",
      tools: [],
      approvals: [],
      sending: mockSending,
      error: null,
      initialize: vi.fn().mockResolvedValue(() => undefined),
      create: vi.fn(),
      open: openConversation,
      send,
      uploadAttachments,
      interrupt,
      resolveApproval: vi.fn(),
    };
    return selector ? selector(state) : state;
  },
}));
vi.mock("../../stores/codexStore", () => ({
  useCodexStore: (selector: (state: unknown) => unknown) =>
    selector({
      status: { state: "ready" },
      connect: vi.fn(),
      subscribe: vi.fn().mockResolvedValue(() => undefined),
      models: mockModels,
      modelsLoading: mockModelsLoading,
      modelsError: mockModelsError,
      loadModels,
    }),
}));
vi.mock("../../stores/pagesStore", () => ({
  usePagesStore: (selector: (state: unknown) => unknown) => selector({ activePage: { id: "page-1", title: "页面", kind: "page" }, vaultPath: "C:\\notes", open: openPage }),
}));
vi.mock("../../stores/workspaceStore", () => ({
  useWorkspaceStore: (selector: (state: unknown) => unknown) => selector({ toggleAgentPanel: vi.fn() }),
}));

describe("AgentPanel", () => {
  afterEach(() => {
    cleanup();
    mockSending = false;
    mockRecords = [];
    mockSelectedModel = null;
    mockModels = [];
    mockModelsLoading = false;
    mockModelsError = null;
    openConversation.mockReset();
    uploadAttachments.mockReset();
    send.mockReset();
    interrupt.mockReset();
    readConversationAttachment.mockReset();
    invoke.mockReset();
    setSelectedModel.mockReset();
    loadModels.mockReset();
    openPage.mockReset();
  });

  it("opens the conversation history list and switches conversations", () => {
    render(<AgentPanel />);
    fireEvent.click(screen.getByRole("button", { name: "历史对话" }));
    expect(screen.getByText("项目规划")).toBeVisible();
    expect(within(screen.getByLabelText("历史对话列表")).getByText(/当前页面/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /打开 项目规划/ }));
    expect(openConversation).toHaveBeenCalledWith("conversation-2");
  });

  it("输入区文字不会被裁切且发送按钮使用蓝色圆形样式", () => {
    render(<AgentPanel />);
    expect(screen.getByPlaceholderText("随心输入")).toHaveClass("agent-prompt");
    expect(screen.getByRole("button", { name: "发送消息" })).toHaveClass("agent-send-button");
  });

  it("聚焦输入框时刷新 WebView2 的输入法候选窗坐标", () => {
    invoke.mockResolvedValue(undefined);
    render(<AgentPanel />);
    fireEvent.focus(screen.getByPlaceholderText("随心输入"));
    expect(invoke).toHaveBeenCalledWith("window_refresh_ime_position");
  });

  it("选择普通文件后显示附件并随消息发送且不附带全文", async () => {
    uploadAttachments.mockResolvedValue([{ id: "file-1", name: "说明.md", mime: "text/markdown", size: 4, relativePath: "attachments/conversation-1/file-1.md" }]);
    render(<AgentPanel />);
    const file = new File(["内容"], "说明.md", { type: "text/markdown" });
    fireEvent.change(screen.getByLabelText("选择 Agent 附件"), { target: { files: [file] } });
    await waitFor(() => expect(screen.getByText("说明.md")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "发送消息" }));
    expect(send).toHaveBeenCalledWith(
      expect.objectContaining({
        requestText: expect.stringContaining("当前页面：页面\n页面 ID：page-1\n需要正文时调用 page.read/page.read_document 按需读取"),
        attachments: [expect.objectContaining({ id: "file-1" })],
      }),
    );
    const callArg = send.mock.calls[0][0];
    expect(callArg.requestText).not.toContain("页面正文：");
  });

  it("请求顺序为输入框文字最前、选中文本保留行号其次、最后为页面按需读取指引", () => {
    render(<AgentPanel selectedText={"第 1-2 行\n选中的文字"} />);
    fireEvent.change(screen.getByPlaceholderText("随心输入"), { target: { value: "请重写" } });
    fireEvent.click(screen.getByRole("button", { name: "发送消息" }));

    expect(send).toHaveBeenCalledWith(
      expect.objectContaining({
        displayText: "请重写",
        selectedText: "第 1-2 行\n选中的文字",
        requestText: "请重写\n\n选中文本：\n> 第 1-2 行\n> 选中的文字\n\n当前页面：页面\n页面 ID：page-1\n需要正文时调用 page.read/page.read_document 按需读取",
      }),
    );
  });

  it("仅有选中文本时也可发送", () => {
    render(<AgentPanel selectedText={"第 3 行\n单独引用"} />);
    const sendButton = screen.getByRole("button", { name: "发送消息" });
    expect(sendButton).not.toBeDisabled();
    fireEvent.click(sendButton);
    expect(send).toHaveBeenCalledWith(
      expect.objectContaining({
        displayText: "",
        selectedText: "第 3 行\n单独引用",
        requestText: "选中文本：\n> 第 3 行\n> 单独引用\n\n当前页面：页面\n页面 ID：page-1\n需要正文时调用 page.read/page.read_document 按需读取",
      }),
    );
  });

  it("工作状态下同一按钮变为停止并可中断，Enter 不会重复发送", () => {
    mockSending = true;
    render(<AgentPanel />);
    expect(screen.queryByLabelText("停止 Agent")).not.toBeInTheDocument();
    const stopButton = screen.getByRole("button", { name: "停止" });
    expect(stopButton).toBeInTheDocument();
    expect(stopButton).not.toBeDisabled();
    fireEvent.click(stopButton);
    expect(interrupt).toHaveBeenCalled();

    fireEvent.keyDown(screen.getByPlaceholderText("随心输入"), { key: "Enter" });
    expect(send).not.toHaveBeenCalled();
  });

  it("粘贴 QQ 截图后显示图片缩略图", async () => {
    uploadAttachments.mockResolvedValue([{ id: "image-1", name: "粘贴图片.png", mime: "image/png", size: 4, relativePath: "attachments/conversation-1/image-1.png" }]);
    readConversationAttachment.mockResolvedValue({ mime: "image/png", bytes: [137, 80, 78, 71] });
    Object.defineProperty(URL, "createObjectURL", { configurable: true, value: vi.fn(() => "blob:agent-image") });
    Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: vi.fn() });
    render(<AgentPanel />);
    const image = new File([new Uint8Array([137, 80, 78, 71])], "", { type: "image/png" });
    fireEvent.paste(screen.getByPlaceholderText("随心输入"), { clipboardData: { files: [image], items: [] } });
    await waitFor(() => expect(screen.getByRole("img", { name: "粘贴图片.png" })).toHaveAttribute("src", "blob:agent-image"));
  });

  it("默认展示跟随 Codex 配置并在有默认模型时辅助显示名称", () => {
    mockModels = [
      { id: "gpt-4o", model: "gpt-4o", displayName: "GPT-4o", description: "默认主力", isDefault: true, inputModalities: [] },
      { id: "claude", model: "claude", displayName: "Claude", description: "备选", isDefault: false, inputModalities: [] },
    ];
    render(<AgentPanel />);
    const selectorButton = screen.getByRole("button", { name: "选择模型" });
    expect(selectorButton).toHaveTextContent("跟随 Codex (GPT-4o)");
  });

  it("点击弹出模型选择菜单并支持选择具体模型与切回默认", () => {
    mockModels = [
      { id: "gpt-4o", model: "gpt-4o", displayName: "GPT-4o", description: "默认主力", isDefault: true, inputModalities: [] },
      { id: "claude", model: "claude", displayName: "Claude", description: "备选", isDefault: false, inputModalities: [] },
    ];
    render(<AgentPanel />);
    const selectorButton = screen.getByRole("button", { name: "选择模型" });
    fireEvent.click(selectorButton);

    const listbox = screen.getByRole("listbox", { name: "模型列表" });
    expect(listbox).toBeInTheDocument();
    expect(within(listbox).getByText("Claude")).toBeInTheDocument();

    fireEvent.click(within(listbox).getByText("Claude"));
    expect(setSelectedModel).toHaveBeenCalledWith("claude");
  });

  it("发送过程中禁用模型切换避免进行中轮次混乱", () => {
    mockSending = true;
    render(<AgentPanel />);
    const selectorButton = screen.getByRole("button", { name: "选择模型" });
    expect(selectorButton).toBeDisabled();
  });

  it("模型加载失败显示提示和重试入口且不影响聊天", () => {
    mockModelsError = "网络超时";
    render(<AgentPanel />);
    expect(screen.getByText("模型刷新受限")).toBeInTheDocument();
    const retryButton = screen.getByRole("button", { name: "重试加载模型" });
    fireEvent.click(retryButton);
    expect(loadModels).toHaveBeenCalled();
  });

  it("Agent 回复中的轻羽来源链接可点击打开页面", () => {
    mockRecords = [
      { kind: "userMessage", data: { displayText: "检索驱动关系" } },
      { kind: "agentMessageDelta", data: { itemId: "reply-1", delta: "来源：[应用与驱动层开发](qingyu://page/source-page)" } },
    ];

    render(<AgentPanel />);
    fireEvent.click(screen.getByRole("link", { name: "应用与驱动层开发" }));

    expect(openPage).toHaveBeenCalledWith("source-page", null);
  });

  it("知识检索结果由界面展示真实拆词和可点击来源", () => {
    mockRecords = [
      { kind: "userMessage", data: { displayText: "设备树和 Platform 总线有什么关系" } },
      { kind: "knowledgeSearchResult", data: {
        turnId: "turn-1",
        result: {
          queryTerms: ["设备树", "Platform 总线"],
          keyword: [{ page: { id: "source-page", title: "应用与驱动层开发" }, matchedTerm: "设备树", link: "qingyu://page/source-page" }],
          semantic: [],
          wiki: [{ pageId: "wiki-page", title: "概念：设备树", link: "qingyu://page/wiki-page", sources: [{ pageId: "source-page", title: "应用与驱动层开发", link: "qingyu://page/source-page" }] }],
          graph: { relations: [{ pageId: "source-page", pageTitle: "应用与驱动层开发", source: "设备树", target: "platform_device", relation: "创建", evidence: "设备树节点创建 platform_device", link: "qingyu://page/source-page" }] },
        },
      } },
      { kind: "agentMessageDelta", data: { turnId: "turn-1", itemId: "reply-1", delta: "设备树用于描述硬件。" } },
    ];

    render(<AgentPanel />);

    expect(screen.getByText("实际拆词：设备树、Platform 总线")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "打开来源 应用与驱动层开发" }));
    expect(openPage).toHaveBeenCalledWith("source-page", null);
    expect(screen.getByText("设备树节点创建 platform_device")).toBeInTheDocument();
  });
});
