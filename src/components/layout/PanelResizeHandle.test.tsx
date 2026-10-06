import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PanelResizeHandle } from "./PanelResizeHandle";

describe("分栏拖拽手柄", () => {
  it("拖动左侧手柄时按横向位移调整宽度", async () => {
    const onResize = vi.fn();
    render(<PanelResizeHandle side="left" value={248} min={220} max={420} onResize={onResize} />);
    const handle = screen.getByRole("separator", { name: "调整左侧栏宽度" });

    fireEvent.pointerDown(handle, { pointerId: 1, clientX: 248 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 288 });

    await waitFor(() => expect(onResize).toHaveBeenCalledWith(288));
  });

  it("最右侧手柄支持方向键调整", () => {
    const onResize = vi.fn();
    render(<PanelResizeHandle side="right" value={360} min={300} max={560} onResize={onResize} />);
    const handle = screen.getByRole("separator", { name: "调整右侧栏宽度" });

    fireEvent.keyDown(handle, { key: "ArrowLeft" });
    fireEvent.keyDown(handle, { key: "ArrowRight" });

    expect(onResize).toHaveBeenNthCalledWith(1, 368);
    expect(onResize).toHaveBeenNthCalledWith(2, 352);
  });
});
