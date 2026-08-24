import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AgentPanel } from "./AgentPanel";

const openConversation = vi.hoisted(() => vi.fn());
const conversations = vi.hoisted(() => [
  { id: "conversation-1", threadId: "thread-1", title: "当前对话", scope: { kind: "vault" }, createdAt: "1787570000", updatedAt: "1787570000", activeTurnId: null },
  { id: "conversation-2", threadId: "thread-2", title: "项目规划", scope: { kind: "page", pageId: "page-1" }, createdAt: "1787570100", updatedAt: "1787570200", activeTurnId: null },
]);

vi.mock("../../stores/agentStore", () => ({
  useAgentStore: () => ({
    conversations,
    activeId: "conversation-1",
    records: [],
    plan: null,
    diff: "",
    tools: [],
    approvals: [],
    sending: false,
    error: null,
    initialize: vi.fn().mockResolvedValue(() => undefined),
    create: vi.fn(),
    open: openConversation,
    send: vi.fn(),
    interrupt: vi.fn(),
    resolveApproval: vi.fn(),
  }),
}));
vi.mock("../../stores/codexStore", () => ({
  useCodexStore: (selector: (state: unknown) => unknown) => selector({ status: { state: "ready" }, connect: vi.fn(), subscribe: vi.fn().mockResolvedValue(() => undefined) }),
}));
vi.mock("../../stores/pagesStore", () => ({
  usePagesStore: (selector: (state: unknown) => unknown) => selector({ activePage: { id: "page-1", title: "页面", kind: "page" }, vaultPath: "C:\\notes" }),
}));
vi.mock("../../stores/workspaceStore", () => ({
  useWorkspaceStore: (selector: (state: unknown) => unknown) => selector({ toggleAgentPanel: vi.fn() }),
}));

describe("AgentPanel", () => {
  afterEach(() => {
    cleanup();
    openConversation.mockReset();
  });

  it("opens the conversation history list and switches conversations", () => {
    render(<AgentPanel />);
    fireEvent.click(screen.getByRole("button", { name: "历史对话" }));
    expect(screen.getByText("项目规划")).toBeVisible();
    expect(screen.getByText("当前页面")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /打开 项目规划/ }));
    expect(openConversation).toHaveBeenCalledWith("conversation-2");
  });
});
