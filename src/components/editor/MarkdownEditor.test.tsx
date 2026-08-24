import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MarkdownEditor } from "./MarkdownEditor";

const storePageAsset = vi.hoisted(() => vi.fn());
const readPageAsset = vi.hoisted(() => vi.fn());
vi.mock("../../lib/tauri/pages", () => ({ storePageAsset, readPageAsset }));
const openPage = vi.fn();
const writeText = vi.fn();

function Harness() {
  const [value, setValue] = useState("# 初始");
  return <><button type="button" onClick={() => setValue("# 外部更新")}>更新</button><MarkdownEditor pageId="page-1" value={value} onChange={setValue} /></>;
}

function LinkHarness() {
  const [value, setValue] = useState("[项目规划](qingyu://page/page-2#block-2)");
  return <MarkdownEditor value={value} onChange={setValue} onOpenPage={openPage} pages={[{ id: "page-2", title: "项目规划", parentId: null, kind: "page", sortOrder: 0, trashedAt: null, path: "项目规划.md" }]} />;
}

function DecorationHarness() {
  const [value, setValue] = useState("# 标题\n\n**加粗** 和 *强调*\n\n- 列表\n\n> 引用\n\n`代码`\n\n![示例](/.noteapp/assets/page-1/image.png)");
  return <MarkdownEditor pageId="page-1" value={value} onChange={setValue} />;
}

describe("MarkdownEditor", () => {
  afterEach(cleanup);

  it("keeps one editor root and applies external values", async () => {
    const { unmount } = render(<Harness />);
    expect(screen.getByLabelText("Markdown 编辑器").querySelector(".cm-editor")).toBeInTheDocument();
    screen.getByRole("button", { name: "更新" }).click();
    await waitFor(() => expect(screen.getByRole("textbox")).toHaveTextContent("# 外部更新"));
    unmount();
  });

  it("stores a selected image and inserts its Markdown path", async () => {
    storePageAsset.mockResolvedValue("/.noteapp/assets/page-1/image.png");
    vi.spyOn(File.prototype, "arrayBuffer").mockResolvedValue(new Uint8Array([137, 80, 78, 71]).buffer);
    const { unmount } = render(<Harness />);
    const file = new File([new Uint8Array([137, 80, 78, 71])], "示例.png", { type: "image/png" });

    fireEvent.change(screen.getByLabelText("选择图片"), { target: { files: [file] } });

    await waitFor(() => expect(storePageAsset).toHaveBeenCalledWith("page-1", "示例.png", [137, 80, 78, 71]));
    await waitFor(() => expect(screen.getByRole("textbox")).toHaveTextContent("![示例](/.noteapp/assets/page-1/image.png)"));
    unmount();
  });

  it("opens a stable page link from its editor decoration", () => {
    const { container, unmount } = render(<LinkHarness />);

    fireEvent.click(container.querySelector(".qy-page-mention") as Element);

    expect(openPage).toHaveBeenCalledWith("page-2", "block-2");
    unmount();
  });

  it("copies a stable link for the current block", async () => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    writeText.mockResolvedValue(undefined);
    const { unmount } = render(<Harness />);

    fireEvent.click(screen.getByRole("button", { name: "复制当前区块链接" }));

    await waitFor(() => expect(writeText).toHaveBeenCalledWith(expect.stringMatching(/^qingyu:\/\/page\/page-1#[0-9a-f-]{36}$/)));
    unmount();
  });

  it("renders Notion-style semantic decorations and an inline image", async () => {
    readPageAsset.mockResolvedValue({ mime: "image/png", bytes: [137, 80, 78, 71] });
    Object.defineProperty(URL, "createObjectURL", { configurable: true, value: vi.fn(() => "blob:image") });
    Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: vi.fn() });
    const { container, unmount } = render(<DecorationHarness />);

    expect(container.querySelector(".qy-heading-1")).toBeInTheDocument();
    expect(container.querySelector(".qy-strong")).toBeInTheDocument();
    expect(container.querySelector(".qy-list-item")).toBeInTheDocument();
    expect(container.querySelector(".qy-blockquote")).toBeInTheDocument();
    expect(container.querySelector(".qy-inline-code")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("img", { name: "示例" })).toHaveAttribute("src", "blob:image"));
    unmount();
  });
});
