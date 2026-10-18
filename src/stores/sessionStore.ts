import { create } from "zustand";

export type EditorPosition = { from: number; to: number; scrollTop: number };
export type WorkspaceSession = {
  tabs: { pageId: string; title: string }[]; activePageId: string | null;
  collapsedIds: string[]; treeScroll: number; positions: Record<string, EditorPosition>;
  sidebarOpen: boolean; sidebarWidth: number; agentPanelOpen: boolean; agentPanelWidth: number;
  fontScale: number; window?: { width: number; height: number; maximized: boolean };
};
const key = (vault: string) => `qingyu.session.v1:${vault.replace(/\\/g, "/").toLowerCase()}`;
export function readSession(vault: string): Partial<WorkspaceSession> {
  try { return JSON.parse(localStorage.getItem(key(vault)) || "{}"); } catch { return {}; }
}
export function saveSession(vault: string, update: Partial<WorkspaceSession>) {
  try { localStorage.setItem(key(vault), JSON.stringify({ ...readSession(vault), ...update })); } catch { /* 存储不可用时保留当前工作区。 */ }
}
type SessionStore = {
  vault: string | null; collapsedIds: string[]; treeScroll: number; positions: Record<string, EditorPosition>;
  bind: (vault: string) => void; toggleNode: (id: string) => void;
  setTreeScroll: (value: number) => void; rememberPosition: (id: string, position: EditorPosition) => void;
};
export const useSessionStore = create<SessionStore>((set, get) => ({
  vault: null, collapsedIds: [], treeScroll: 0, positions: {},
  bind: (vault) => { const saved = readSession(vault); set({ vault, collapsedIds: saved.collapsedIds ?? [], treeScroll: saved.treeScroll ?? 0, positions: saved.positions ?? {} }); },
  toggleNode: (id) => { const state = get(); const collapsedIds = state.collapsedIds.includes(id) ? state.collapsedIds.filter((value) => value !== id) : [...state.collapsedIds, id]; set({ collapsedIds }); if (state.vault) saveSession(state.vault, { collapsedIds }); },
  setTreeScroll: (treeScroll) => { set({ treeScroll }); const vault = get().vault; if (vault) saveSession(vault, { treeScroll }); },
  rememberPosition: (id, position) => {
    const state = get(); const positions = { ...state.positions }; delete positions[id]; positions[id] = position;
    const entries = Object.entries(positions); if (entries.length > 100) delete positions[entries[0][0]];
    set({ positions }); if (state.vault) saveSession(state.vault, { positions });
  },
}));
