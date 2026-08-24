import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("应用入口", () => {
  it("renders the QingYu shell", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "开始构建你的知识库" })).toBeInTheDocument();
    expect(screen.getByLabelText("Agent 面板")).toBeInTheDocument();
  });
});
