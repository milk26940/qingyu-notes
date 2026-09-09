import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MarkdownEditor } from "./MarkdownEditor";

const storePageAsset = vi.hoisted(() => vi.fn());
const readPageAsset = vi.hoisted(() => vi.fn());
vi.mock("../../lib/tauri/pages", () => ({ storePageAsset, readPageAsset }));
const openPage = vi.fn();

function Harness() {
  const [value, setValue] = useState("# 初始");
  return <><button type="button" onClick={() => setValue("# 外部更新")}>更新</button><output aria-label="当前 Markdown">{value}</output><MarkdownEditor pageId="page-1" value={value} onChange={setValue} /></>;
}

function LinkHarness() {
  const [value, setValue] = useState("[项目规划](qingyu://page/page-2#block-2)");
  return <MarkdownEditor value={value} onChange={setValue} onOpenPage={openPage} pages={[{ id: "page-2", title: "项目规划", parentId: null, kind: "page", sortOrder: 0, trashedAt: null, path: "项目规划.md" }]} />;
}

function DecorationHarness() {
  const [value, setValue] = useState("# 标题\n\n**加粗** 和 *强调*\n\n---\n\n<aside>\n提示内容\n</aside>\n\n```mermaid\ngraph LR\nA --> B\n```\n\n- 列表\n\n> 引用\n\n`代码`\n\n![示例](/.noteapp/assets/page-1/image.png)");
  return <MarkdownEditor pageId="page-1" value={value} onChange={setValue} />;
}

function ContextMenuHarness() {
  const [value, setValue] = useState("第一段\n\n第二段");
  return <MarkdownEditor pageId="page-1" value={value} onChange={setValue} onCreateChild={vi.fn()} />;
}

describe("MarkdownEditor", () => {
  afterEach(() => {
    cleanup();
  });

  it("keeps one editor root and applies external values", async () => {
    const { unmount } = render(<Harness />);
    expect(screen.getByLabelText("Markdown 编辑器").querySelector(".cm-editor")).toBeInTheDocument();
    screen.getByRole("button", { name: "更新" }).click();
    await waitFor(() => expect(screen.getByRole("textbox")).toHaveTextContent("外部更新"));
    expect(screen.getByRole("textbox")).not.toHaveTextContent("#");
    unmount();
  });

  it("stores a selected image and inserts its Markdown path", async () => {
    storePageAsset.mockResolvedValue("/.noteapp/assets/page-1/image.png");
    vi.spyOn(File.prototype, "arrayBuffer").mockResolvedValue(new Uint8Array([137, 80, 78, 71]).buffer);
    const onChange = vi.fn();
    const { unmount } = render(<MarkdownEditor pageId="page-1" value="# 初始" onChange={onChange} />);
    const file = new File([new Uint8Array([137, 80, 78, 71])], "示例.png", { type: "image/png" });

    fireEvent.change(screen.getByLabelText("选择图片"), { target: { files: [file] } });

    await waitFor(() => expect(storePageAsset).toHaveBeenCalledWith("page-1", "示例.png", [137, 80, 78, 71]));
    await waitFor(() => expect(onChange).toHaveBeenCalledWith(expect.stringContaining("![示例](/.noteapp/assets/page-1/image.png)")));
    unmount();
  });

  it("opens a stable page link from its editor decoration", () => {
    const { container, unmount } = render(<LinkHarness />);

    const link = container.querySelector(".qy-page-mention") as Element;
    expect(link).toHaveTextContent("项目规划");
    expect(link).not.toHaveTextContent("qingyu://");
    fireEvent.click(link);

    expect(openPage).toHaveBeenCalledWith("page-2", "block-2");
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
    expect(container.querySelector(".qy-divider-widget")).toBeInTheDocument();
    expect(container.querySelector(".qy-aside-line")).toBeInTheDocument();
    expect(container.querySelector(".qy-mermaid-widget")).toBeInTheDocument();
    expect(container.querySelector(".qy-heading-1")).not.toHaveTextContent("#");
    await waitFor(() => expect(screen.getByRole("img", { name: "示例" })).toHaveAttribute("src", "blob:image"));
    unmount();
  });

  it("在正文空白行右键显示页面和数据库创建菜单", () => {
    render(<ContextMenuHarness />);
    const blankLine = Array.from(screen.getByRole("textbox").querySelectorAll(".cm-line")).find((line) => line.querySelector("br"));
    expect(blankLine).toBeDefined();
    fireEvent.contextMenu(blankLine!);
    expect(screen.getByRole("button", { name: "新建子页面" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "新建数据库" })).toBeInTheDocument();
  });

});
