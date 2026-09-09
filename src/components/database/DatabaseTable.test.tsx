import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DatabaseRecordProperties, DatabaseTable } from "./DatabaseTable";
import type { Page } from "../../lib/tauri/pages";

const updateRecord = vi.hoisted(() => vi.fn());
const rename = vi.hoisted(() => vi.fn());
const openDatabase = vi.hoisted(() => vi.fn());
const openPage = vi.hoisted(() => vi.fn());
const state = vi.hoisted(() => ({
  bundle: {
    page: { id: "db-1", title: "任务", kind: "database" },
    definition: { id: "db-1", pageId: "db-1", fields: [
      { id: "field-1", name: "说明", fieldType: "text", options: [] },
      { id: "field-2", name: "关联", fieldType: "relation", options: [] },
    ], views: [], createdAt: "0", updatedAt: "0" },
  },
  records: [{ id: "record-1", title: "记录", kind: "databaseRecord", databaseId: "db-1", properties: { "field-1": "旧值", "field-2": [] } }],
  filters: [], sorts: [], open: openDatabase, addField: vi.fn(), addRecord: vi.fn(), updateRecord, rename,
  setFilters: vi.fn(), setSorts: vi.fn(),
}));

vi.mock("../../stores/databaseStore", () => ({ useDatabaseStore: (selector?: (value: typeof state) => unknown) => selector ? selector(state) : state }));
vi.mock("../../stores/pagesStore", () => ({ usePagesStore: (selector: (value: { open: () => void; pages: Array<{ id: string; title: string }> }) => unknown) => selector({ open: openPage, pages: [{ id: "page-2", title: "目标页面" }] }) }));

describe("DatabaseTable", () => {
  beforeEach(() => { updateRecord.mockReset(); openDatabase.mockReset(); openPage.mockReset(); });
  afterEach(cleanup);

  it("commits an inline property edit to the record page", () => {
    render(<DatabaseTable databaseId="db-1" />);
    const input = screen.getByDisplayValue("旧值");
    fireEvent.change(input, { target: { value: "新值" } });
    fireEvent.blur(input);
    expect(updateRecord).toHaveBeenCalledWith(
      expect.objectContaining({ id: "record-1" }),
      "记录",
      { "field-1": "新值", "field-2": [] },
    );
  });

  it("shows relation targets by title and persists their page ids", () => {
    render(<DatabaseTable databaseId="db-1" />);
    const select = screen.getByLabelText("记录-关联") as HTMLSelectElement;
    const option = screen.getByRole("option", { name: "目标页面" }) as HTMLOptionElement;
    option.selected = true;
    fireEvent.change(select);
    expect(updateRecord).toHaveBeenCalledWith(
      expect.objectContaining({ id: "record-1" }),
      "记录",
      { "field-1": "旧值", "field-2": ["page-2"] },
    );
  });

  it("shows editable properties on a database record page", () => {
    render(<DatabaseRecordProperties record={state.records[0] as unknown as Page} />);

    expect(screen.getByText("说明")).toBeInTheDocument();
    expect(screen.getByDisplayValue("旧值")).toBeInTheDocument();
    expect(screen.getByText("关联")).toBeInTheDocument();
  });

  it("opens a record page with one title click", () => {
    render(<DatabaseTable databaseId="db-1" />);

    fireEvent.click(screen.getByRole("button", { name: "打开记录" }));

    expect(openPage).toHaveBeenCalledWith("record-1");
  });

  it("失焦后保存数据库名称", () => {
    render(<DatabaseTable databaseId="db-1" />);
    const title = screen.getByLabelText("数据库名称");
    fireEvent.change(title, { target: { value: "新的数据库名称" } });
    fireEvent.blur(title);
    expect(rename).toHaveBeenCalledWith("新的数据库名称");
  });
});
