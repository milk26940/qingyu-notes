import { create } from "zustand";

export type FontScaleAction = "increase" | "decrease" | "reset";

const STORAGE_KEY = "qingyu.fontScale";
const DEFAULT_SCALE = 100;

function normalizeFontScale(value: number) {
  if (!Number.isFinite(value)) return DEFAULT_SCALE;
  return Math.min(150, Math.max(80, Math.round(value / 10) * 10));
}

export function nextFontScale(current: number, action: FontScaleAction) {
  if (action === "reset") return DEFAULT_SCALE;
  return normalizeFontScale(current + (action === "increase" ? 10 : -10));
}

function storedFontScale() {
  if (typeof localStorage === "undefined") return DEFAULT_SCALE;
  return normalizeFontScale(Number(localStorage.getItem(STORAGE_KEY) ?? DEFAULT_SCALE));
}

function syncFontScale(value: number) {
  const normalized = normalizeFontScale(value);
  if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, String(normalized));
  if (typeof document !== "undefined") document.documentElement.style.setProperty("--qy-font-scale", String(normalized / 100));
  return normalized;
}

type AppearanceStore = {
  fontScale: number;
  increaseFont: () => void;
  decreaseFont: () => void;
  resetFont: () => void;
  applyFontScale: () => void;
};

export const useAppearanceStore = create<AppearanceStore>((set, get) => {
  const update = (action: FontScaleAction) => {
    const fontScale = syncFontScale(nextFontScale(get().fontScale, action));
    set({ fontScale });
  };

  return {
    fontScale: DEFAULT_SCALE,
    increaseFont: () => update("increase"),
    decreaseFont: () => update("decrease"),
    resetFont: () => update("reset"),
    applyFontScale: () => set({ fontScale: syncFontScale(storedFontScale()) }),
  };
});
