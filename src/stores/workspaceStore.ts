import { create } from "zustand";

type WorkspaceStore = {
  sidebarOpen: boolean;
  agentPanelOpen: boolean;
  sidebarWidth: number;
  agentPanelWidth: number;
  setSidebarWidth: (width: number) => void;
  setAgentPanelWidth: (width: number) => void;
  toggleSidebar: () => void;
  toggleAgentPanel: () => void;
};

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, Math.round(value)));

const storedWidth = (key: string, fallback: number, min: number, max: number) => {
  try {
    const value = Number(localStorage.getItem(key));
    return Number.isFinite(value) && value > 0 ? clamp(value, min, max) : fallback;
  } catch {
    return fallback;
  }
};

const persistWidth = (key: string, value: number) => {
  try { localStorage.setItem(key, String(value)); } catch { /* 本地设置不可用时仍保留当前会话宽度。 */ }
};

export const useWorkspaceStore = create<WorkspaceStore>((set) => ({
  sidebarOpen: true,
  agentPanelOpen: true,
  sidebarWidth: storedWidth("qingyu.sidebarWidth", 248, 220, 420),
  agentPanelWidth: storedWidth("qingyu.agentPanelWidth", 360, 300, 560),
  setSidebarWidth: (width) => set(() => { const next = clamp(width, 220, 420); persistWidth("qingyu.sidebarWidth", next); return { sidebarWidth: next }; }),
  setAgentPanelWidth: (width) => set(() => { const next = clamp(width, 300, 560); persistWidth("qingyu.agentPanelWidth", next); return { agentPanelWidth: next }; }),
  toggleSidebar: () => set((state) => ({ sidebarOpen: !state.sidebarOpen, ...(window.innerWidth < 850 && !state.sidebarOpen ? { agentPanelOpen: false } : {}) })),
  toggleAgentPanel: () => set((state) => ({ agentPanelOpen: !state.agentPanelOpen, ...(window.innerWidth < 850 && !state.agentPanelOpen ? { sidebarOpen: false } : {}) })),
}));
