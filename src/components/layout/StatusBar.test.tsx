import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useBackupStore } from "../../stores/backupStore";
import { usePagesStore } from "../../stores/pagesStore";
import { StatusBar } from "./StatusBar";
import { useCodexStore } from "../../stores/codexStore";

describe("状态栏", () => {
  afterEach(cleanup);
  beforeEach(() => {
    usePagesStore.setState({ saveState: "saved", projectionStatus: { state: "ready" } });
    useBackupStore.setState({ loading: true, error: null });
    useCodexStore.setState({ status: { state: "ready", userAgent: "test", codexHome: "" } });
  });

  it("显示真实保存和备份状态", () => {
    render(<StatusBar />);

    expect(screen.getByText("已保存")).toBeInTheDocument();
    expect(screen.getByText("备份进行中")).toBeInTheDocument();
    expect(screen.getByText("搜索就绪")).toBeInTheDocument();
  });

  it("连接成功时显示连接图标而不是断开斜线", () => {
    render(<StatusBar />);

    const icon = document.querySelector("svg.lucide-link-2");
    expect(icon?.getAttribute("class")).toContain("text-emerald-600");
  });

  it("状态信息贴合窗口底部而不是悬浮卡片", () => {
    render(<StatusBar />);

    expect(screen.getByRole("contentinfo")).toHaveClass("border-t");
    expect(screen.getByRole("contentinfo")).not.toHaveClass("rounded-xl");
    expect(screen.getByRole("contentinfo")).not.toHaveClass("shadow-sm");
  });
});
