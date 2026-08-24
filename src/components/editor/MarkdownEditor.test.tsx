import { render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { MarkdownEditor } from "./MarkdownEditor";

function Harness() {
  const [value, setValue] = useState("# 初始");
  return <><button type="button" onClick={() => setValue("# 外部更新")}>更新</button><MarkdownEditor value={value} onChange={setValue} /></>;
}

describe("MarkdownEditor", () => {
  it("keeps one editor root and applies external values", async () => {
    const { unmount } = render(<Harness />);
    expect(screen.getByLabelText("Markdown 编辑器").querySelector(".cm-editor")).toBeInTheDocument();
    screen.getByRole("button", { name: "更新" }).click();
    await waitFor(() => expect(screen.getByRole("textbox")).toHaveTextContent("# 外部更新"));
    unmount();
  });
});
