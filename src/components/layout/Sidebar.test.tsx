import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "./Sidebar";

describe("笔记库切换", () => {
  afterEach(cleanup);
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("便携版不允许切换到安装目录外的笔记库", () => {
    render(<Sidebar />);
    expect(screen.queryByRole("button", { name: "切换笔记库" })).not.toBeInTheDocument();
  });

  it("导航入口可以点击并显示当前选择", () => {
    render(<Sidebar />);

    const pageButtons = screen.getAllByRole("button", { name: "页面" });
    fireEvent.click(pageButtons[pageButtons.length - 1]);

    const selectedButtons = screen.getAllByRole("button", { name: "页面" });
    expect(selectedButtons[selectedButtons.length - 1]).toHaveAttribute("aria-current", "page");
  });

  it("固定导航区并只让页面树滚动", () => {
    render(<Sidebar />);

    expect(screen.getByLabelText("工作区导航")).toHaveClass("overflow-hidden");
    expect(screen.getByLabelText("页面树滚动区域")).toHaveClass("overflow-y-auto");
  });
});
