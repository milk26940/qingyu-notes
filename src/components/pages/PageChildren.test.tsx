import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PageChildren } from "./PageChildren";

const create = vi.hoisted(() => vi.fn());
const open = vi.hoisted(() => vi.fn());
const state = vi.hoisted(() => ({
  pages: [
    { id: "child-1", title: "子页面", parentId: "parent-1" },
    { id: "other-1", title: "其他页面", parentId: null },
  ],
  create,
  open,
}));

vi.mock("../../stores/pagesStore", () => ({
  usePagesStore: (selector: (value: typeof state) => unknown) => selector(state),
}));

describe("PageChildren", () => {
  afterEach(cleanup);

  it("opens existing children and creates a nested page", () => {
    render(<PageChildren pageId="parent-1" />);

    expect(screen.queryByRole("button", { name: "其他页面" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "子页面" }));
    fireEvent.click(screen.getByRole("button", { name: "新建子页面" }));

    expect(open).toHaveBeenCalledWith("child-1");
    expect(create).toHaveBeenCalledWith("未命名页面", "parent-1");
  });
});
