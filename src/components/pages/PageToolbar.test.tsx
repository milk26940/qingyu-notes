import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PageToolbar } from "./PageToolbar";

const writeText = vi.hoisted(() => vi.fn());
const state = vi.hoisted(() => ({
  activePage: { id: "page-1", title: "项目规划" },
  saveState: "saved",
  links: [],
  backlinks: [],
  history: [],
  searchQuery: "",
  save: vi.fn(),
  create: vi.fn(),
  trashPage: vi.fn(),
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
});
