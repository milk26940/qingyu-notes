import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type CodexStatus =
  | { state: "stopped" }
  | { state: "starting" }
  | { state: "ready"; userAgent: string; codexHome: string }
  | { state: "failed"; code: string; message: string };

export type CodexError = { code: string; message: string };

export type CodexModelOption = {
  id: string;
  model: string;
  displayName: string;
  description: string;
  isDefault: boolean;
  inputModalities: string[];
};

export const getCodexStatus = () => invoke<CodexStatus>("codex_status");
export const startCodex = () => invoke<CodexStatus>("codex_start");
export const stopCodex = () => invoke<CodexStatus>("codex_stop");
export const restartCodex = () => invoke<CodexStatus>("codex_restart");
export const listCodexModels = () => invoke<CodexModelOption[]>("codex_models");
export const subscribeCodexStatus = (handler: (status: CodexStatus) => void): Promise<UnlistenFn> =>
  listen<CodexStatus>("codex://status", (event) => handler(event.payload));
export const subscribeCodexConfigChanges = (handler: () => void): Promise<UnlistenFn> =>
  listen("codex://config-changed", handler);
