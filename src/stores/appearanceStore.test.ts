import { beforeEach, describe, expect, it } from "vitest";
import { nextFontScale, useAppearanceStore } from "./appearanceStore";

describe("appearanceStore", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.style.removeProperty("--qy-font-scale");
    useAppearanceStore.setState({ fontScale: 100 });
  });

  it("keeps font scaling between 80 and 150 percent", () => {
    expect(nextFontScale(100, "increase")).toBe(110);
    expect(nextFontScale(150, "increase")).toBe(150);
    expect(nextFontScale(80, "decrease")).toBe(80);
    expect(nextFontScale(130, "reset")).toBe(100);
  });

  it("loads, persists, and applies the selected font scale", () => {
    localStorage.setItem("qingyu.fontScale", "130");

    useAppearanceStore.getState().applyFontScale();

    expect(useAppearanceStore.getState().fontScale).toBe(130);
    expect(document.documentElement.style.getPropertyValue("--qy-font-scale")).toBe("1.3");

    useAppearanceStore.getState().increaseFont();

    expect(useAppearanceStore.getState().fontScale).toBe(140);
    expect(localStorage.getItem("qingyu.fontScale")).toBe("140");
    expect(document.documentElement.style.getPropertyValue("--qy-font-scale")).toBe("1.4");
  });
});
