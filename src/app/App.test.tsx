import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("../lib/tauri/pages", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/tauri/pages")>()),
  subscribeVaultChanges: vi.fn().mockResolvedValue(() => undefined),
}));

describe("应用入口", () => {
  it("renders the QingYu shell", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "开始构建你的知识库" })).toBeInTheDocument();
    expect(screen.getByLabelText("Agent 面板")).toBeInTheDocument();
  });
});
