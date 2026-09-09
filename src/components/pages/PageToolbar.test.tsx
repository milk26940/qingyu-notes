import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PageToolbar } from "./PageToolbar";

const writeText = vi.hoisted(() => vi.fn());
const trashPageRecursive = vi.hoisted(() => vi.fn());
const state = vi.hoisted(() => ({
  activePage: { id: "page-1", title: "项目规划", kind: "page" as const },
  pages: [],
  saveState: "saved",
  links: [],
  backlinks: [],
  history: [],
  searchQuery: "",
  save: vi.fn(),
  create: vi.fn(),
  trashPage: vi.fn(),
  trashPageRecursive,
  search: vi.fn(),
  reportError: vi.fn(),
}));

vi.mock("../../stores/pagesStore", () => ({
  usePagesStore: (selector: (value: typeof state) => unknown) => selector(state),
}));

describe("PageToolbar", () => {
  afterEach(cleanup);

  it("copies the active page link", async () => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    writeText.mockResolvedValue(undefined);
    render(<PageToolbar />);

    fireEvent.click(screen.getByRole("button", { name: "复制页面链接" }));

    await waitFor(() => expect(writeText).toHaveBeenCalledWith("qingyu://page/page-1"));
  });

  it("点击删除页面打开确认弹窗，确认后调用递归删除", () => {
    trashPageRecursive.mockReset();
    render(<PageToolbar />);

    fireEvent.click(screen.getByRole("button", { name: "删除页面" }));

    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("删除页面")).toBeInTheDocument();
    expect(screen.getByText(/确定删除页面“项目规划”？/)).toBeInTheDocument();
    expect(screen.getByText("删除后，该页面将不再显示。")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(trashPageRecursive).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "删除页面" }));
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));

    expect(trashPageRecursive).toHaveBeenCalledWith("page-1");
  });
});
