import { create } from "zustand";

type WorkspaceStore = {
  sidebarOpen: boolean;
  agentPanelOpen: boolean;
  sidebarWidth: number;
  agentPanelWidth: number;
  toggleSidebar: () => void;
  toggleAgentPanel: () => void;
};

export const useWorkspaceStore = create<WorkspaceStore>((set) => ({
  sidebarOpen: true,
  agentPanelOpen: true,
  sidebarWidth: 248,
  agentPanelWidth: 360,
  toggleSidebar: () => set((state) => ({ sidebarOpen: !state.sidebarOpen })),
  toggleAgentPanel: () => set((state) => ({ agentPanelOpen: !state.agentPanelOpen })),
}));
