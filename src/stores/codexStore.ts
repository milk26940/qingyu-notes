import { create } from "zustand";
import {
  getCodexStatus,
  startCodex,
  stopCodex,
  subscribeCodexStatus,
  type CodexError,
  type CodexStatus,
} from "../lib/tauri/codex";

type CodexStore = {
  status: CodexStatus;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  subscribe: () => Promise<() => void>;
};

const asError = (error: unknown): CodexError => {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    return { code: String(error.code), message: String(error.message) };
  }
  return { code: "codex_unknown_error", message: String(error) };
};

export const useCodexStore = create<CodexStore>((set) => ({
  status: { state: "stopped" },
  connect: async () => {
    set({ status: { state: "starting" } });
    try {
      set({ status: await startCodex() });
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
  subscribe: async () => {
    const unlisten = await subscribeCodexStatus((status) => set({ status }));
    return unlisten;
  },
}));

export async function loadInitialCodexStatus(): Promise<void> {
  try {
    useCodexStore.setState({ status: await getCodexStatus() });
  } catch (error) {
    const failure = asError(error);
    useCodexStore.setState({ status: { state: "failed", ...failure } });
  }
}
