import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Topbar } from "./Topbar";

const trashPageRecursive = vi.hoisted(() => vi.fn());
const activePage = vi.hoisted(() => ({ id: "page-1", title: "项目主页", kind: "page" as const }));
const pages = vi.hoisted(() => [
  { id: "page-1", title: "项目主页", parentId: null, kind: "page" as const, sortOrder: 0, trashedAt: null, path: "项目主页.md" },
  { id: "page-2", title: "子任务", parentId: "page-1", kind: "page" as const, sortOrder: 0, trashedAt: null, path: "项目主页/子任务.md" },
]);

vi.mock("../../stores/workspaceStore", () => ({ useWorkspaceStore: () => ({ toggleSidebar: vi.fn(), toggleAgentPanel: vi.fn() }) }));
vi.mock("../../stores/pagesStore", () => ({
  usePagesStore: (selector: (state: unknown) => unknown) =>
    selector({
      activePage,
      pages,
      saveState: "idle",
      save: vi.fn(),
      trashPageRecursive,
    }),
}));

describe("Topbar 标签菜单", () => {
  afterEach(cleanup);

  it("中间标签提供左右和全部关闭，点击空白关闭菜单", () => {
    render(<Topbar tabs={[{ pageId: "1", title: "一" }, { pageId: "2", title: "二" }, { pageId: "3", title: "三" }]} activeTabId="2" onCloseTabs={vi.fn()} />);
    fireEvent.contextMenu(screen.getByRole("button", { name: "二" }));
    expect(screen.getByRole("button", { name: "关闭左侧标签页" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "关闭右侧标签页" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "全部关闭" })).toBeInTheDocument();
    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("button", { name: "全部关闭" })).not.toBeInTheDocument();
  });

  it("显示羽毛图标和轻羽品牌", () => {
    render(<Topbar />);
    expect(screen.getByLabelText("轻羽品牌")).toHaveTextContent("轻羽");
    expect(screen.getByLabelText("轻羽应用图标")).toHaveClass("w-10", "rounded-xl");
  });

  it("最左侧标签不显示关闭左侧", () => {
    render(<Topbar tabs={[{ pageId: "1", title: "一" }, { pageId: "2", title: "二" }]} activeTabId="1" onCloseTabs={vi.fn()} />);
    fireEvent.contextMenu(screen.getByRole("button", { name: "一" }));
    expect(screen.queryByRole("button", { name: "关闭左侧标签页" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "关闭右侧标签页" })).toBeInTheDocument();
  });

  it("最右侧标签不显示关闭右侧", () => {
    render(<Topbar tabs={[{ pageId: "1", title: "一" }, { pageId: "2", title: "二" }]} activeTabId="2" onCloseTabs={vi.fn()} />);
    fireEvent.contextMenu(screen.getByRole("button", { name: "二" }));
    expect(screen.getByRole("button", { name: "关闭左侧标签页" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "关闭右侧标签页" })).not.toBeInTheDocument();
  });

  it("点击顶部递归删除按钮打开应用内确认弹窗，确认后调用递归删除", () => {
    trashPageRecursive.mockReset();
    render(<Topbar />);

    const deleteBtn = screen.getByRole("button", { name: "递归删除页面" });
    fireEvent.click(deleteBtn);

    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("删除页面")).toBeInTheDocument();
    expect(screen.getByText(/确定删除页面“项目主页”？/)).toBeInTheDocument();
    expect(screen.getByText(/此次操作将同时删除当前页面及其下的 1 个子页面、数据库或条目。/)).toBeInTheDocument();
    expect(screen.getByText(/此操作可能影响其他页面中的引用链接。/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(trashPageRecursive).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    fireEvent.click(deleteBtn);
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));
    expect(trashPageRecursive).toHaveBeenCalledWith("page-1");
  });
});
