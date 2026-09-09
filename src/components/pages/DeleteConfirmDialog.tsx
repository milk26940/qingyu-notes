import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { useState } from "react";
import type { PageKind } from "../../lib/tauri/pages";

export function getPageKindLabel(kind?: PageKind): string {
  switch (kind) {
    case "database":
      return "数据库";
    case "databaseRecord":
      return "条目";
    case "page":
    default:
      return "页面";
  }
}

export function countDescendants(
  id: string,
  pages: Array<{ id: string; parentId: string | null }>,
): number {
  return pages
    .filter((page) => page.parentId === id)
    .reduce((sum, page) => sum + 1 + countDescendants(page.id, pages), 0);
}

export type DeleteConfirmDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  kind?: PageKind;
  descendantCount?: number;
  onConfirm: () => Promise<void> | void;
};

export function DeleteConfirmDialog({
  open,
  onOpenChange,
  title,
  kind = "page",
  descendantCount = 0,
  onConfirm,
}: DeleteConfirmDialogProps) {
  const [submitting, setSubmitting] = useState(false);
  const typeName = getPageKindLabel(kind);
  const displayTitle = title.trim() || (kind === "database" ? "未命名数据库" : kind === "databaseRecord" ? "未命名条目" : "未命名页面");

  const handleConfirm = async () => {
    if (submitting) return;
    setSubmitting(true);
    try {
      await onConfirm();
      onOpenChange(false);
    } finally {
      setSubmitting(false);
    }
  };

  const detail =
    descendantCount > 0
      ? `此次操作将同时删除当前${typeName}及其下的 ${descendantCount} 个子页面、数据库或条目。此操作可能影响其他页面中的引用链接。`
      : `删除后，该${typeName}将不再显示。`;

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(next) => {
        if (!submitting) onOpenChange(next);
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/25 backdrop-blur-[2px]" />
        <Dialog.Content
          className="fixed left-1/2 top-1/2 z-50 flex w-[min(440px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-[24px] border border-white bg-[#f7f8fa] p-6 shadow-[0_28px_80px_rgba(22,28,36,.24)]"
          onPointerDownOutside={(event) => {
            if (submitting) event.preventDefault();
          }}
          onEscapeKeyDown={(event) => {
            if (submitting) event.preventDefault();
          }}
        >
          <header className="flex shrink-0 items-center justify-between">
            <Dialog.Title className="text-[17px] font-bold text-[var(--qy-ink)]">
              删除{typeName}
            </Dialog.Title>
            <Dialog.Close
              disabled={submitting}
              className="icon-button h-8 w-8 rounded-xl text-[var(--qy-muted)] hover:bg-[#eef0f3] disabled:opacity-50"
              aria-label="关闭"
            >
              <X size={16} />
            </Dialog.Close>
          </header>

          <Dialog.Description className="mt-3 text-[13px] leading-relaxed text-[var(--qy-muted)]">
            <span className="block font-semibold text-[var(--qy-ink)]">
              确定删除{typeName}“{displayTitle}”？
            </span>
            <span className="mt-1.5 block">
              {detail}
            </span>
          </Dialog.Description>

          <div className="mt-6 flex items-center justify-end gap-2.5">
            <button
              type="button"
              disabled={submitting}
              className="h-9 rounded-xl border border-[#cbd0d6] bg-white px-4 text-[12px] font-semibold text-[var(--qy-ink)] hover:bg-[#f0f2f5] disabled:opacity-50"
              onClick={() => onOpenChange(false)}
            >
              取消
            </button>
            <button
              type="button"
              disabled={submitting}
              className="h-9 rounded-xl bg-[#d92d20] px-4 text-[12px] font-semibold text-white shadow-sm hover:bg-[#b42318] disabled:opacity-50"
              onClick={() => void handleConfirm()}
            >
              {submitting ? "删除中..." : "确认删除"}
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
