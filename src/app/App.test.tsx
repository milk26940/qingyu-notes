import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { useAppearanceStore } from "../stores/appearanceStore";

vi.mock("../lib/tauri/pages", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/tauri/pages")>()),
  subscribeVaultChanges: vi.fn().mockResolvedValue(() => undefined),
}));

describe("应用入口", () => {
  afterEach(cleanup);

  beforeEach(() => {
    localStorage.setItem("qingyu.fontScale", "100");
    useAppearanceStore.setState({ fontScale: 100 });
  });

  it("renders the QingYu shell", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "开始构建你的知识库" })).toBeInTheDocument();
    expect(screen.getByLabelText("Agent 面板")).toBeInTheDocument();
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
});
