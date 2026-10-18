import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type CodexStatus =
  | { state: "stopped" }
  | { state: "starting" }
  | { state: "ready"; userAgent: string; codexHome: string }
  | { state: "failed"; code: string; message: string };

export type CodexError = { code: string; message: string };
export type AgentSettings = { baseUrl: string; model: string; hasApiKey: boolean; workspaceRoot: string };
export type AgentConnectionTest = { model: string; text: boolean; tools: boolean; images: boolean; details: string[] };
export const getAgentSettings = () => invoke<AgentSettings>("agent_settings_get");
export const saveAgentSettings = (settings: { baseUrl: string; model: string; apiKey?: string; clearApiKey?: boolean }) => invoke<AgentSettings>("agent_settings_save", settings);
export const testAgentConnection = (model?: string) => invoke<AgentConnectionTest>("agent_test_connection", { model });

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
