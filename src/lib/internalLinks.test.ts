import { describe, expect, it } from "vitest";
import { parseQingyuPageLink } from "./internalLinks";

describe("parseQingyuPageLink", () => {
  it("parses page and block links and rejects other protocols", () => {
    expect(parseQingyuPageLink("qingyu://page/page-1#block-1")).toEqual({ pageId: "page-1", blockId: "block-1" });
    expect(parseQingyuPageLink("qingyu://page/page-2")).toEqual({ pageId: "page-2", blockId: null });
    expect(parseQingyuPageLink("https://example.com/page-1")).toBeNull();
  });
});
