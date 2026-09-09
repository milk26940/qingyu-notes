import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspaceStore } from "./workspaceStore";

describe("工作区栏宽", () => {
  beforeEach(() => {
    localStorage.clear();
    useWorkspaceStore.setState({ sidebarWidth: 248, agentPanelWidth: 360 });
  });

  it("限制并持久化左侧栏宽度", () => {
    useWorkspaceStore.getState().setSidebarWidth(900);
    expect(useWorkspaceStore.getState().sidebarWidth).toBe(420);
    expect(localStorage.getItem("qingyu.sidebarWidth")).toBe("420");

    useWorkspaceStore.getState().setSidebarWidth(100);
    expect(useWorkspaceStore.getState().sidebarWidth).toBe(220);
  });

  it("限制并持久化 Agent 栏宽度", () => {
    useWorkspaceStore.getState().setAgentPanelWidth(100);
    expect(useWorkspaceStore.getState().agentPanelWidth).toBe(300);
    expect(localStorage.getItem("qingyu.agentPanelWidth")).toBe("300");

    useWorkspaceStore.getState().setAgentPanelWidth(900);
    expect(useWorkspaceStore.getState().agentPanelWidth).toBe(560);
  });
});
