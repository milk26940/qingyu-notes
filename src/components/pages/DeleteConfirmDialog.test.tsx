import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DeleteConfirmDialog, countDescendants, getPageKindLabel } from "./DeleteConfirmDialog";

describe("DeleteConfirmDialog", () => {
  afterEach(cleanup);

  it("正确映射页面、数据库和条目的对象类型标签", () => {
    expect(getPageKindLabel("page")).toBe("页面");
    expect(getPageKindLabel("database")).toBe("数据库");
    expect(getPageKindLabel("databaseRecord")).toBe("条目");
    expect(getPageKindLabel(undefined)).toBe("页面");
  });

  it("正确计算后代节点数量", () => {
    const pages = [
      { id: "1", parentId: null },
      { id: "2", parentId: "1" },
      { id: "3", parentId: "2" },
      { id: "4", parentId: "1" },
      { id: "5", parentId: null },
    ];
    expect(countDescendants("1", pages)).toBe(3);
    expect(countDescendants("2", pages)).toBe(1);
    expect(countDescendants("3", pages)).toBe(0);
    expect(countDescendants("5", pages)).toBe(0);
  });

  it("显示数据库类型、对象名称及子项后代数量，点击确认执行一次并防止重复提交", async () => {
    let resolveConfirm: () => void;
    const confirmPromise = new Promise<void>((resolve) => {
      resolveConfirm = resolve;
    });
    const onConfirm = vi.fn().mockImplementation(() => confirmPromise);
    const onOpenChange = vi.fn();

    render(
      <DeleteConfirmDialog
        open={true}
        onOpenChange={onOpenChange}
        title="客户库"
        kind="database"
        descendantCount={5}
        onConfirm={onConfirm}
      />
    );

    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("删除数据库")).toBeInTheDocument();
    expect(screen.getByText(/确定删除数据库“客户库”？/)).toBeInTheDocument();
    expect(screen.getByText(/此次操作将同时删除当前数据库及其下的 5 个子页面、数据库或条目。/)).toBeInTheDocument();
    expect(screen.getByText(/此操作可能影响其他页面中的引用链接。/)).toBeInTheDocument();
    expect(screen.queryByText(/回收站/)).not.toBeInTheDocument();
    expect(screen.queryByText(/恢复/)).not.toBeInTheDocument();

    const confirmBtn = screen.getByRole("button", { name: "确认删除" });
    const cancelBtn = screen.getByRole("button", { name: "取消" });

    // 危险色确认按钮
    expect(confirmBtn).toHaveClass("bg-[#d92d20]");

    // 第一次点击
    fireEvent.click(confirmBtn);
    expect(onConfirm).toHaveBeenCalledTimes(1);

    // 处于 submitting 状态，按钮显示删除中且被禁用
    expect(screen.getByRole("button", { name: "删除中..." })).toBeDisabled();
    expect(cancelBtn).toBeDisabled();

    // 再次点击无效
    fireEvent.click(screen.getByRole("button", { name: "删除中..." }));
    expect(onConfirm).toHaveBeenCalledTimes(1);

    // 完成异步操作
    resolveConfirm!();
    await waitFor(() => {
      expect(onOpenChange).toHaveBeenCalledWith(false);
    });
  });

  it("条目类型弹窗在没有子项时显示删除后不再显示", () => {
    render(
      <DeleteConfirmDialog
        open={true}
        onOpenChange={vi.fn()}
        title="张三"
        kind="databaseRecord"
        descendantCount={0}
        onConfirm={vi.fn()}
      />
    );

    expect(screen.getByText("删除条目")).toBeInTheDocument();
    expect(screen.getByText(/确定删除条目“张三”？/)).toBeInTheDocument();
    expect(screen.getByText("删除后，该条目将不再显示。")).toBeInTheDocument();
    expect(screen.queryByText(/回收站/)).not.toBeInTheDocument();
    expect(screen.queryByText(/恢复/)).not.toBeInTheDocument();
  });

  it("页面类型弹窗在没有子项时显示删除后不再显示", () => {
    render(
      <DeleteConfirmDialog
        open={true}
        onOpenChange={vi.fn()}
        title="项目规划"
        kind="page"
        descendantCount={0}
        onConfirm={vi.fn()}
      />
    );

    expect(screen.getByText("删除页面")).toBeInTheDocument();
    expect(screen.getByText(/确定删除页面“项目规划”？/)).toBeInTheDocument();
    expect(screen.getByText("删除后，该页面将不再显示。")).toBeInTheDocument();
    expect(screen.queryByText(/回收站/)).not.toBeInTheDocument();
    expect(screen.queryByText(/恢复/)).not.toBeInTheDocument();
  });

  it("点击取消不执行删除回调并关闭弹窗", () => {
    const onConfirm = vi.fn();
    const onOpenChange = vi.fn();

    render(
      <DeleteConfirmDialog
        open={true}
        onOpenChange={onOpenChange}
        title="测试页面"
        kind="page"
        onConfirm={onConfirm}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(onConfirm).not.toHaveBeenCalled();
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
