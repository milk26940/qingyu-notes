import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("应用入口", () => {
  it("renders the scaffold shell", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "Welcome to Tauri + React" })).toBeInTheDocument();
  });
});
