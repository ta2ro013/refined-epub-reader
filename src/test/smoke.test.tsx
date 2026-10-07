import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import App from "../App";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("初期画面のテスト環境", () => {
  it("初期画面と入力操作を表示できる", () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "Welcome to Tauri + React" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveValue("");
    expect(screen.getByRole("button", { name: "Greet" })).toBeEnabled();
  });

  it("名前を入力して送信すると返された挨拶を表示する", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValue("Hello, Reader! You've been greeted from Rust!");
    render(<App />);

    await user.type(screen.getByRole("textbox"), "Reader");
    await user.click(screen.getByRole("button", { name: "Greet" }));

    expect(
      await screen.findByText("Hello, Reader! You've been greeted from Rust!"),
    ).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("greet", { name: "Reader" });
  });
});
