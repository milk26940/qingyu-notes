import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { BackupDialog } from "./BackupDialog";

const load = vi.hoisted(() => vi.fn());

vi.mock("../../stores/pagesStore", () => ({
  usePagesStore: (selector: (state: unknown) => unknown) =>
    selector({ activePage: { id: "page-1", title: "页面" } }),
}));

vi.mock("../../stores/backupStore", () => ({
  useBackupStore: () => ({
    backups: [],
    backupLocation: "D:\\backup\\snapshots",
    loading: false,
    error: null,
    exportedPath: null,
    importReport: null,
    importPreview: null,
    load,
    checkpoint: vi.fn(),
    restore: vi.fn(),
    exportLossless: vi.fn(),
    exportMarkdown: vi.fn(),
    previewContent: vi.fn(),
    importContent: vi.fn(),
  }),
}));

describe("备份与恢复弹窗", () => {
  it("使用分区布局并要求先分析再确认导入", () => {
    render(<BackupDialog open onOpenChange={vi.fn()} />);

    expect(screen.getByRole("dialog")).toHaveClass("rounded-[24px]");
    expect(screen.getByText("创建与导出")).toBeInTheDocument();
    expect(screen.getByText("本地快照")).toBeInTheDocument();
    expect(screen.getByText(/备份位置：/)).toBeInTheDocument();
    expect(screen.getByText("D:\\backup\\snapshots")).toBeInTheDocument();
    expect(screen.getByText("导入内容")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "分析导入内容" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "确认导入" })).not.toBeInTheDocument();
  });
});
