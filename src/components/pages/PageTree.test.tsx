import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PageTree } from "./PageTree";

const create = vi.hoisted(() => vi.fn());
const state = vi.hoisted(() => ({
  pages: [
    { id: "page-1", title: "一级", parentId: null, kind: "page", sortOrder: 0, trashedAt: null, path: "一级.md" },
    { id: "page-2", title: "二级", parentId: "page-1", kind: "page", sortOrder: 0, trashedAt: null, path: "一级/二级.md" },
    { id: "page-3", title: "三级", parentId: "page-2", kind: "page", sortOrder: 0, trashedAt: null, path: "一级/二级/三级.md" },
    { id: "page-4", title: "四级", parentId: "page-3", kind: "page", sortOrder: 0, trashedAt: null, path: "一级/二级/三级/四级.md" },
  ],
  trash: [],
  activePage: null,
  create,
  open: vi.fn(),
  move: vi.fn(),
  restore: vi.fn(),
}));

vi.mock("../../stores/pagesStore", () => ({
  usePagesStore: (selector: (value: typeof state) => unknown) => selector(state),
}));

describe("PageTree", () => {
  afterEach(cleanup);

  it("renders deep descendants and creates a child from any node", () => {
    render(<PageTree />);

    expect(screen.getByRole("button", { name: "四级" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "在三级下新建子页面" }));

    expect(create).toHaveBeenCalledWith("未命名页面", "page-3");
  });
});
