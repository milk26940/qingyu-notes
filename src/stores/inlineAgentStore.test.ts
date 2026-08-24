import { beforeEach, describe, expect, it, vi } from "vitest";
import { useInlineAgentStore } from "./inlineAgentStore";

const requestInline = vi.hoisted(() => vi.fn());
vi.mock("./agentStore", () => ({ useAgentStore: { getState: () => ({ requestInline }) } }));

const context = { pageId: "page-1", title: "规划", content: "前文\n需要处理的文字\n后文", from: 3, to: 10 };

describe("inlineAgentStore", () => {
  beforeEach(() => {
    requestInline.mockReset();
    useInlineAgentStore.setState({ candidate: null, error: null });
  });

  it("inserts generated text after the selection", async () => {
    requestInline.mockResolvedValue("生成结果");

    await useInlineAgentStore.getState().request("generate", context);

    expect(useInlineAgentStore.getState().candidate).toEqual(expect.objectContaining({ from: 10, to: 10, original: "", proposed: "生成结果", status: "ready" }));
  });

  it("replaces selected text for summarize and clears rejected candidates", async () => {
    requestInline.mockResolvedValue("摘要结果");

    await useInlineAgentStore.getState().request("summarize", context);

    expect(useInlineAgentStore.getState().candidate).toEqual(expect.objectContaining({ from: 3, to: 10, original: context.content.slice(3, 10), proposed: "摘要结果" }));
    useInlineAgentStore.getState().reject();
    expect(useInlineAgentStore.getState().candidate).toBeNull();
  });

  it("keeps the document untouched when the request fails", async () => {
    requestInline.mockRejectedValue(new Error("连接失败"));

    await useInlineAgentStore.getState().request("rewrite", context);

    expect(useInlineAgentStore.getState().candidate).toBeNull();
    expect(useInlineAgentStore.getState().error).toBe("连接失败");
  });
});
