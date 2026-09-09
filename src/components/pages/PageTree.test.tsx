import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PageTree } from "./PageTree";

const create = vi.hoisted(() => vi.fn());
const move = vi.hoisted(() => vi.fn());
const trashPageRecursive = vi.hoisted(() => vi.fn());
const state = vi.hoisted(() => ({
  pages: [
    { id: "page-1", title: "一级", parentId: null, kind: "page", sortOrder: 0, trashedAt: null, path: "一级.md" },
    { id: "page-2", title: "二级", parentId: "page-1", kind: "page", sortOrder: 0, trashedAt: null, path: "一级/二级.md" },
    { id: "page-3", title: "三级", parentId: "page-2", kind: "page", sortOrder: 0, trashedAt: null, path: "一级/二级/三级.md" },
    { id: "page-4", title: "四级", parentId: "page-3", kind: "page", sortOrder: 0, trashedAt: null, path: "一级/二级/三级/四级.md" },
    { id: "page-5", title: "另一个根页面", parentId: null, kind: "page", sortOrder: 1, trashedAt: null, path: "另一个根页面.md" },
  ],
  trash: [{ id: "trash-1", title: "已删除", parentId: null, kind: "page", sortOrder: 0, trashedAt: "1", path: ".noteapp/trash/trash-1.md" }],
  activePage: null,
  create,
  open: vi.fn(),
  move,
  restore: vi.fn(),
  trashPageRecursive,
}));

vi.mock("../../stores/pagesStore", () => ({
  usePagesStore: Object.assign((selector: (value: typeof state) => unknown) => selector(state), { getState: () => state }),
}));

describe("PageTree", () => {
  afterEach(() => { cleanup(); move.mockReset(); });

  it("renders deep descendants and creates a child from any node", () => {
    render(<PageTree />);

    expect(screen.getByRole("button", { name: "四级" })).toBeInTheDocument();
    fireEvent.contextMenu(screen.getByRole("button", { name: "三级" }));
    fireEvent.click(screen.getByRole("button", { name: "新建子页面" }));

    expect(create).toHaveBeenCalledWith("未命名页面", "page-3");
  });

  it("moves a nested page back to the root drop target", () => {
    render(<PageTree />);
    const transfer = { getData: vi.fn(() => "page-2"), setData: vi.fn() };

    fireEvent.drop(screen.getByLabelText("移动到根目录"), { dataTransfer: transfer });

    expect(move).toHaveBeenCalledWith("page-2", null);
  });

  it("点击页面节点时使用工作区打开回调", () => {
    const onOpen = vi.fn();
    render(<PageTree onOpen={onOpen} />);

    fireEvent.click(screen.getByRole("button", { name: "一级" }));

    expect(onOpen).toHaveBeenCalledWith("page-1");
  });

  it("点击菜单外的空白区域后关闭右键菜单", () => {
    render(<PageTree />);
    fireEvent.contextMenu(screen.getByRole("button", { name: "一级" }));
    expect(screen.getByRole("button", { name: "新建子页面" })).toBeInTheDocument();
    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("button", { name: "新建子页面" })).not.toBeInTheDocument();
  });

  it("拖到页面上缘时调整同级顺序", () => {
    render(<PageTree />);
    const row = screen.getByRole("button", { name: "一级" }).closest("[draggable]") as HTMLElement;
    vi.spyOn(row, "getBoundingClientRect").mockReturnValue({ top: 0, bottom: 32, height: 32, left: 0, right: 200, width: 200, x: 0, y: 0, toJSON: () => ({}) });
    const transfer = { getData: vi.fn(() => "page-5"), setData: vi.fn() };

    fireEvent.drop(row, { dataTransfer: transfer, clientY: 2 });

    expect(move).toHaveBeenCalledWith("page-5", null, 0);
  });

  it("右键删除先关闭菜单并显示应用内确认弹窗，取消不执行，确认调用递归删除", async () => {
    trashPageRecursive.mockReset();
    render(<PageTree />);

    // 一级 page 有 page-2, page-3, page-4 共 3 个后代
    fireEvent.contextMenu(screen.getByRole("button", { name: "一级" }));
    const deleteBtn = screen.getByRole("button", { name: "递归删除" });
    expect(deleteBtn).toBeInTheDocument();

    fireEvent.click(deleteBtn);
    // 右键菜单已关闭
    expect(screen.queryByRole("button", { name: "新建子页面" })).not.toBeInTheDocument();

    // 确认弹窗已显示
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("删除页面")).toBeInTheDocument();
    expect(screen.getByText(/确定删除页面“一级”？/)).toBeInTheDocument();
    expect(screen.getByText(/此次操作将同时删除当前页面及其下的 3 个子页面、数据库或条目。/)).toBeInTheDocument();
    expect(screen.getByText(/此操作可能影响其他页面中的引用链接。/)).toBeInTheDocument();

    // 点击取消
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(trashPageRecursive).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    // 重新打开并点击确认删除
    fireEvent.contextMenu(screen.getByRole("button", { name: "一级" }));
    fireEvent.click(screen.getByRole("button", { name: "递归删除" }));
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));

    expect(trashPageRecursive).toHaveBeenCalledWith("page-1");
  });

});
