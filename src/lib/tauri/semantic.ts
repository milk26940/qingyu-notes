import { invoke } from "@tauri-apps/api/core";

export type SemanticConfig = { baseUrl: string; model: string; dimension: number; enabled: boolean; hasApiKey: boolean };
export type SemanticRebuildStats = { scanned: number; reused: number; updated: number; removed: number; failed: number; errors?: string[]; cancelled?: boolean };
export const getSemanticConfig = () => invoke<SemanticConfig>("semantic_get_config");
export const saveSemanticConfig = (config: { baseUrl: string; model: string; dimension: number; enabled: boolean; apiKey?: string }) => invoke<SemanticConfig>("semantic_save_config", { baseUrl: config.baseUrl, model: config.model, dimension: config.dimension, enabled: config.enabled, apiKey: config.apiKey || null });
export const testSemanticConnection = (config: { baseUrl: string; model: string; dimension: number; apiKey: string }) => invoke<number>("semantic_test_connection", { baseUrl: config.baseUrl, model: config.model, dimension: config.dimension, apiKey: config.apiKey });
export const rebuildSemanticIndex = (full = false) => invoke<SemanticRebuildStats>("semantic_rebuild_index", { full });
export type SemanticSearchResult = { pageId: string; blockId: string; pageTitle: string; snippet: string; score: number };
export const semanticSearch = (query: string) => invoke<SemanticSearchResult[]>("semantic_search", { query });
