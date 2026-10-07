import { useLayoutEffect } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PageViewProps } from "../features/reader/PageView";
import App from "../App";

vi.mock("../features/reader/PageView", () => ({
  PageView: (props: PageViewProps) => {
    useLayoutEffect(() => {
      props.onPagination({ page: 0, count: 2 });
    }, [props.xhtml, props.resourceBase]);
    return <section aria-label="接続した本文">{props.xhtml}</section>;
  },
}));
afterEach(clearMocks);

describe("読書画面の接続", () => {
  it("起動時に書籍を選ぶ案内と操作を表示する", () => {
    render(<App />);
    expect(
      screen.getByRole("heading", { name: "読む本を選ぶ" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "書籍を開く" })).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "Greet" }),
    ).not.toBeInTheDocument();
  });
  it("開く操作からRustコマンドの書籍情報と章本文が画面へつながる", async () => {
    mockIPC((command, args) => {
      if (command === "select_book")
        return {
          id: "9",
          title: "小さな旅の記録",
          chapters: ["OPS/first.xhtml"],
          resource_base: "http://book.localhost/9/",
          toc: [],
        };
      if (
        command === "read_chapter" &&
        args &&
        "bookId" in args &&
        "index" in args &&
        args.bookId === "9" &&
        args.index === 0
      )
        return '<html xmlns="http://www.w3.org/1999/xhtml"><head/><body><h1>朝の道</h1><p>橋の手前で足を止める。</p></body></html>';
      throw "missing_resource";
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "書籍を開く" }));
    expect(
      await screen.findByRole("heading", { name: "小さな旅の記録" }),
    ).toBeInTheDocument();
    expect(
      await screen.findByRole("region", { name: "接続した本文" }),
    ).toHaveTextContent("橋の手前で足を止める。");
    expect(screen.getByText("朝の道")).toBeInTheDocument();
    expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
      "1 / 2",
    );
  });
});
