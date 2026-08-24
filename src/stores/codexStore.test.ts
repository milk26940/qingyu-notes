import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCodexStore } from "./codexStore";

const mocks = vi.hoisted(() => ({ startCodex: vi.fn(), subscribeCodexStatus: vi.fn() }));

vi.mock("../lib/tauri/codex", () => ({
  getCodexStatus: vi.fn(),
  startCodex: mocks.startCodex,
  stopCodex: vi.fn(),
  subscribeCodexStatus: mocks.subscribeCodexStatus,
}));

describe("useCodexStore", () => {
  beforeEach(() => {
    useCodexStore.setState({ status: { state: "stopped" } });
    mocks.startCodex.mockReset();
    mocks.subscribeCodexStatus.mockReset();
  });

  it("connects and stores the native ready status", async () => {
    mocks.startCodex.mockResolvedValue({
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

  it("automatically reconnects after the owned App Server disconnects", async () => {
    vi.useFakeTimers();
    let statusHandler: (status: { state: "failed"; code: string; message: string }) => void = () => undefined;
    mocks.subscribeCodexStatus.mockImplementation(async (handler) => { statusHandler = handler; return () => undefined; });
    mocks.startCodex.mockResolvedValue({
      state: "ready",
      userAgent: "Codex Desktop/0.146.0",
      codexHome: "C:\\Users\\26940\\.codex",
    });
    await useCodexStore.getState().subscribe();
    statusHandler({ state: "failed", code: "codex_disconnected", message: "已断开" });
    await vi.advanceTimersByTimeAsync(1500);
    expect(mocks.startCodex).toHaveBeenCalledOnce();
    expect(useCodexStore.getState().status.state).toBe("ready");
    vi.useRealTimers();
  });
});
