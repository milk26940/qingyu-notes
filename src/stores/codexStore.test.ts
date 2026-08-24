import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCodexStore } from "./codexStore";

const startCodex = vi.hoisted(() => vi.fn());

vi.mock("../lib/tauri/codex", () => ({
  getCodexStatus: vi.fn(),
  startCodex,
  stopCodex: vi.fn(),
  subscribeCodexStatus: vi.fn(),
}));

describe("useCodexStore", () => {
  beforeEach(() => {
    useCodexStore.setState({ status: { state: "stopped" } });
    startCodex.mockReset();
  });

  it("connects and stores the native ready status", async () => {
    startCodex.mockResolvedValue({
      state: "ready",
      userAgent: "Codex Desktop/0.146.0",
      codexHome: "C:\\Users\\26940\\.codex",
    });

    expect(useCodexStore.getState().status).toEqual({ state: "stopped" });
    await useCodexStore.getState().connect();
    expect(useCodexStore.getState().status).toEqual({
      state: "ready",
      userAgent: "Codex Desktop/0.146.0",
      codexHome: "C:\\Users\\26940\\.codex",
    });
  });
});
