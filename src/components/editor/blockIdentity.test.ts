import { describe, expect, it } from "vitest";
import { ensureBlockIds, findBlock, findBlockAt } from "./blockIdentity";

describe("blockIdentity", () => {
  it("adds stable ids to non-empty blocks and is idempotent", () => {
    const ids = ["11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222"];
    const markdown = "# 标题\n\n正文";

    const first = ensureBlockIds(markdown, () => ids.shift()!);

    expect(first).toContain("<!-- qy:block:11111111-1111-4111-8111-111111111111 -->");
    expect(first).toContain("<!-- qy:block:22222222-2222-4222-8222-222222222222 -->");
    expect(ensureBlockIds(first, () => "unexpected")).toBe(first);
  });

  it("finds a block by id or cursor position", () => {
    const markdown = "第一段\n<!-- qy:block:11111111-1111-4111-8111-111111111111 -->\n\n第二段\n<!-- qy:block:22222222-2222-4222-8222-222222222222 -->";

    expect(findBlock(markdown, "22222222-2222-4222-8222-222222222222")).toEqual(expect.objectContaining({ id: "22222222-2222-4222-8222-222222222222", text: "第二段" }));
    expect(findBlockAt(markdown, markdown.indexOf("第一段"))?.id).toBe("11111111-1111-4111-8111-111111111111");
  });
});
