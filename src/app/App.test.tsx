import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { useAppearanceStore } from "../stores/appearanceStore";
import { useCodexStore } from "../stores/codexStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import * as pageApi from "../lib/tauri/pages";

vi.mock("../lib/tauri/pages", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/tauri/pages")>()),
  subscribeVaultChanges: vi.fn().mockResolvedValue(() => undefined),
  subscribeProjectionStatus: vi.fn().mockResolvedValue(() => undefined),
  searchPages: vi.fn(),
}));

describe("应用入口", () => {
  afterEach(cleanup);

  beforeEach(() => {
    localStorage.setItem("qingyu.fontScale", "100");
    useAppearanceStore.setState({ fontScale: 100 });
    useWorkspaceStore.setState({ agentPanelOpen: true });
  });

  it("renders the QingYu shell", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "选择或新建一个页面" })).toBeInTheDocument();
    expect(screen.getByLabelText("轻羽羽毛标志")).toHaveClass("w-24", "rounded-[22px]");
    expect(screen.getByLabelText("Agent 面板")).toBeInTheDocument();
  });

  it("点击侧栏搜索后可以搜索笔记", async () => {
    vi.mocked(pageApi.searchPages).mockResolvedValue([
      {
        page: {
          id: "page-1",
          title: "雨停在凌晨四点",
          parentId: null,
          kind: "page",
          sortOrder: 0,
          trashedAt: null,
          path: "雨停在凌晨四点.md",
        },
        snippet: "凌晨四点，雨终于停了。",
      },
    ]);
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "搜索" }));
    fireEvent.change(screen.getByRole("searchbox", { name: "搜索笔记" }), {
      target: { value: "凌晨" },
    });

    expect(await screen.findByRole("button", { name: /雨停在凌晨四点/ })).toBeInTheDocument();
    expect(pageApi.searchPages).toHaveBeenCalledWith("凌晨");
  });

  it("handles text scaling shortcuts without browser zoom", async () => {
    render(<App />);
    await waitFor(() => expect(document.documentElement.style.getPropertyValue("--qy-font-scale")).toBe("1"));

    const increase = new KeyboardEvent("keydown", { key: "=", ctrlKey: true, cancelable: true });
    window.dispatchEvent(increase);
    expect(increase.defaultPrevented).toBe(true);
    expect(useAppearanceStore.getState().fontScale).toBe(110);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "-", ctrlKey: true, cancelable: true }));
    expect(useAppearanceStore.getState().fontScale).toBe(100);

    useAppearanceStore.getState().increaseFont();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "0", ctrlKey: true, cancelable: true }));
    expect(useAppearanceStore.getState().fontScale).toBe(100);
    expect(localStorage.getItem("qingyu.fontScale")).toBe("100");
  });

  it("Agent 面板关闭时仍维持全局状态订阅", async () => {
    const connect = vi.fn().mockResolvedValue(undefined);
    const subscribe = vi.fn().mockResolvedValue(() => undefined);
    useWorkspaceStore.setState({ agentPanelOpen: false });
    useCodexStore.setState({ connect, subscribe, status: { state: "stopped" } });

    render(<App />);

    await waitFor(() => expect(connect).toHaveBeenCalledOnce());
    expect(subscribe).toHaveBeenCalledOnce();
    expect(screen.queryByLabelText("Agent 面板")).not.toBeInTheDocument();
  });
});
