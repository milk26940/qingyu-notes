import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";

type PanelResizeHandleProps = {
  side: "left" | "right";
  value: number;
  min: number;
  max: number;
  onResize: (width: number) => void;
};

export function PanelResizeHandle({ side, value, min, max, onResize }: PanelResizeHandleProps) {
  const dragRef = useRef<{ x: number; width: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const label = side === "left" ? "调整左侧栏宽度" : "调整右侧栏宽度";
  const resize = (width: number) => onResize(Math.min(max, Math.max(min, width)));
  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    dragRef.current = { x: event.clientX, width: value };
    setDragging(true);
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const start = dragRef.current;
    if (!start) return;
    const distance = event.clientX - start.x;
    resize(start.width + (side === "left" ? distance : -distance));
  };
  const stopDragging = (event: ReactPointerEvent<HTMLDivElement>) => {
    dragRef.current = null;
    setDragging(false);
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };
  return (
    <div
      role="separator"
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      tabIndex={0}
      data-dragging={dragging || undefined}
      className="qy-panel-resizer"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={stopDragging}
      onPointerCancel={stopDragging}
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault();
        const direction = event.key === "ArrowRight" ? 1 : -1;
        resize(value + direction * (side === "left" ? 8 : -8));
      }}
    >
      <span aria-hidden="true" />
    </div>
  );
}
