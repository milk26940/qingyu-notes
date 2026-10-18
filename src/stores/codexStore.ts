import { create } from "zustand";
import {
  getCodexStatus,
  listCodexModels,
  startCodex,
  restartCodex,
  stopCodex,
  subscribeCodexConfigChanges,
  subscribeCodexStatus,
  type CodexError,
  type CodexModelOption,
  type CodexStatus,
} from "../lib/tauri/codex";

type CodexStore = {
  status: CodexStatus;
  restartRequired: boolean;
  models: CodexModelOption[];
  modelsLoading: boolean;
  modelsError: string | null;
  loadModels: () => Promise<boolean>;
  connect: () => Promise<void>;
  restart: () => Promise<void>;
  disconnect: () => Promise<void>;
  subscribe: () => Promise<() => void>;
};

const asError = (error: unknown): CodexError => {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    return { code: String(error.code), message: String(error.message) };
  }
  return { code: "codex_unknown_error", message: String(error) };
};

let modelLoadSequence = 0;

export const useCodexStore = create<CodexStore>((set, get) => ({
  status: { state: "stopped" },
  restartRequired: false,
  models: [],
  modelsLoading: false,
  modelsError: null,
  loadModels: async () => {
    const sequence = ++modelLoadSequence;
    set({ modelsLoading: true, modelsError: null });
    try {
      const models = await listCodexModels();
      if (sequence !== modelLoadSequence) return false;
      set({ models, modelsLoading: false, modelsError: null });
      return true;
    } catch (error) {
      if (sequence !== modelLoadSequence) return false;
      const failure = asError(error);
      set({ modelsLoading: false, modelsError: failure.message });
      return false;
    }
  },
  connect: async () => {
    set({ status: { state: "starting" } });
    try {
      const status = await startCodex();
      set({ status, restartRequired: false });
      if (status.state === "ready") {
        void get().loadModels();
      }
    } catch (error) {
      const failure = asError(error);
      set({ status: { state: "failed", ...failure } });
    }
  },
  disconnect: async () => {
    try {
      set({ status: await stopCodex() });
    } catch (error) {
      const failure = asError(error);
      set({ status: { state: "failed", ...failure } });
    }
  },
  restart: async () => {
    set({ status: { state: "starting" } });
    try {
      const status = await restartCodex();
      set({ status, restartRequired: false });
      if (status.state === "ready") {
        void get().loadModels();
      }
    } catch (error) {
      const failure = asError(error);
      set({ status: { state: "failed", ...failure } });
    }
  },
  subscribe: async () => {
    const unlistenStatus = await subscribeCodexStatus((status) => {
      const prev = get().status;
      set({ status });
      if (status.state === "ready" && prev.state !== "ready") {
        void get().loadModels();
      }
      if (status.state === "failed" && status.code === "codex_disconnected") {
        window.setTimeout(() => { if (get().status.state === "failed") void get().connect(); }, 1500);
      }
    });
    const unlistenConfig = await subscribeCodexConfigChanges(() => {
      // 内置运行时在下一次请求时读取新设置，无需重启外部进程。
      set({ restartRequired: false });
      void get().loadModels();
    });
    return () => { unlistenStatus(); unlistenConfig(); };
  },
}));

export async function loadInitialCodexStatus(): Promise<void> {
  try {
    const status = await getCodexStatus();
    useCodexStore.setState({ status });
    if (status.state === "ready") {
      void useCodexStore.getState().loadModels();
    }
  } catch (error) {
    const failure = asError(error);
    useCodexStore.setState({ status: { state: "failed", ...failure } });
  }
}
