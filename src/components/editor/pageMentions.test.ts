import { describe, expect, it } from "vitest";
import type { PageSummary } from "../../lib/tauri/pages";
import { pageMentionOptions } from "./pageMentions";

const pages = [
  { id: "page-1", title: "项目规划", parentId: "folder-1", kind: "page", sortOrder: 0, trashedAt: null, path: "工作/项目规划.md" },
  { id: "page-2", title: "阅读清单", parentId: null, kind: "page", sortOrder: 0, trashedAt: null, path: "阅读清单.md" },
] as PageSummary[];

describe("pageMentions", () => {
  it("filters by title or path and inserts a stable page link", () => {
    expect(pageMentionOptions("工作", pages)).toEqual([
      expect.objectContaining({
        label: "项目规划",
        detail: "工作/项目规划",
        apply: "[项目规划](qingyu://page/page-1)",
      }),
    ]);
  });
});
