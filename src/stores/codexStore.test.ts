import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCodexStore } from "./codexStore";

const mocks = vi.hoisted(() => ({
  startCodex: vi.fn(),
  restartCodex: vi.fn(),
  subscribeCodexStatus: vi.fn(),
  subscribeCodexConfigChanges: vi.fn(),
  listCodexModels: vi.fn(),
}));

vi.mock("../lib/tauri/codex", () => ({
  getCodexStatus: vi.fn(),
  startCodex: mocks.startCodex,
  restartCodex: mocks.restartCodex,
  stopCodex: vi.fn(),
  listCodexModels: mocks.listCodexModels,
  subscribeCodexStatus: mocks.subscribeCodexStatus,
  subscribeCodexConfigChanges: mocks.subscribeCodexConfigChanges,
}));

describe("useCodexStore", () => {
  beforeEach(() => {
    useCodexStore.setState({
      status: { state: "stopped" },
      restartRequired: false,
      models: [],
      modelsLoading: false,
      modelsError: null,
    });
    mocks.startCodex.mockReset();
    mocks.restartCodex.mockReset();
    mocks.listCodexModels.mockReset();
    mocks.subscribeCodexStatus.mockReset();
    mocks.subscribeCodexConfigChanges.mockReset();
    mocks.subscribeCodexConfigChanges.mockResolvedValue(() => undefined);
  });

  it("配置变化后等待用户确认重启", async () => {
    let configHandler = () => undefined;
    mocks.subscribeCodexStatus.mockResolvedValue(() => undefined);
    mocks.subscribeCodexConfigChanges.mockImplementation(async (handler) => { configHandler = handler; return () => undefined; });
    mocks.restartCodex.mockResolvedValue({ state: "ready", userAgent: "Codex", codexHome: "C:\\Users\\26940\\.codex" });

    await useCodexStore.getState().subscribe();
    configHandler();
    expect(useCodexStore.getState().restartRequired).toBe(true);

    await useCodexStore.getState().restart();
    expect(mocks.restartCodex).toHaveBeenCalledOnce();
    expect(useCodexStore.getState().restartRequired).toBe(false);
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

  it("Codex 处于 ready 后自动加载可用模型列表", async () => {
    const fakeModels = [
      {
        id: "gpt-4o",
        model: "gpt-4o",
        displayName: "GPT-4o",
        description: "OpenAI 旗舰模型",
        isDefault: true,
        inputModalities: ["text", "image"],
      },
      {
        id: "claude-3-5-sonnet",
        model: "claude-3-5-sonnet",
        displayName: "Claude 3.5 Sonnet",
        description: "Anthropic 智能模型",
        isDefault: false,
        inputModalities: ["text", "image"],
      },
    ];
    mocks.listCodexModels.mockResolvedValue(fakeModels);

    await useCodexStore.getState().loadModels();
    expect(mocks.listCodexModels).toHaveBeenCalledOnce();
    expect(useCodexStore.getState().models).toEqual(fakeModels);
    expect(useCodexStore.getState().modelsLoading).toBe(false);
    expect(useCodexStore.getState().modelsError).toBeNull();
  });

  it("模型加载失败记录错误且不破坏状态", async () => {
    mocks.listCodexModels.mockRejectedValue({ code: "rpc_error", message: "Codex 暂时不可用" });

    await useCodexStore.getState().loadModels();
    expect(useCodexStore.getState().modelsLoading).toBe(false);
    expect(useCodexStore.getState().modelsError).toBe("Codex 暂时不可用");
    expect(useCodexStore.getState().models).toEqual([]);
  });
});
