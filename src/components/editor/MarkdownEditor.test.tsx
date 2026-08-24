import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { MarkdownEditor } from "./MarkdownEditor";

const storePageAsset = vi.hoisted(() => vi.fn());
vi.mock("../../lib/tauri/pages", () => ({ storePageAsset }));
const openPage = vi.fn();

function Harness() {
  const [value, setValue] = useState("# 初始");
  return <><button type="button" onClick={() => setValue("# 外部更新")}>更新</button><MarkdownEditor pageId="page-1" value={value} onChange={setValue} /></>;
}

function LinkHarness() {
  const [value, setValue] = useState("[项目规划](qingyu://page/page-2)");
  return <MarkdownEditor value={value} onChange={setValue} onOpenPage={openPage} pages={[{ id: "page-2", title: "项目规划", parentId: null, kind: "page", sortOrder: 0, trashedAt: null, path: "项目规划.md" }]} />;
}

describe("MarkdownEditor", () => {
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

    expect(openPage).toHaveBeenCalledWith("page-2");
    unmount();
  });
});
