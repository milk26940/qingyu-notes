import { describe, expect, it } from "vitest";
import { conversationTimeline } from "./conversationTimeline";
import type { ConversationRecord } from "../../lib/tauri/conversations";
const record = (kind: string, data: Record<string, unknown>): ConversationRecord => ({ kind, data, timestamp: "100" });
describe("对话轮次", () => {
  it("保留旧回复，并将阶段和真实修改归属各自轮次", () => {
    const turns = conversationTimeline([
      record("userMessage", { displayText: "旧请求" }),
      record("agentMessageDelta", { itemId: "old-1", delta: "旧过程" }),
      record("agentMessageDelta", { itemId: "old-2", delta: "旧答复" }),
      record("userMessage", { displayText: "新请求" }),
      record("turnStarted", { id: "new" }),
      record("itemStarted", { turnId: "new", item: { id: "progress", itemType: "agentMessage", phase: "commentary" } }),
      record("agentMessageDelta", { turnId: "new", itemId: "progress", delta: "正在修改" }),
      record("toolActivity", { turnId: "new", callId: "call", tool: "update", stage: "completed", success: true, change: { before: { pageId: "p", title: "页面", content: "前" }, after: { pageId: "p", title: "页面", content: "后" } } }),
      record("itemCompleted", { turnId: "new", item: { id: "answer", itemType: "agentMessage", phase: "final_answer", text: "修改完成" } }),
      record("turnCompleted", { id: "new", status: "completed" }),
    ], null, false);
    expect(turns[0].messages.map((message) => message.text)).toEqual(["旧过程", "旧答复"]);
    expect(turns[1]).toMatchObject({ status: "completed", messages: [{ phase: "commentary" }, { phase: "final_answer", text: "修改完成" }], changes: [{ confirmed: true, after: { content: "后" } }] });
  });
});
