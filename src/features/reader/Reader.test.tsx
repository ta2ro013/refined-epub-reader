import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Reader } from "./Reader";
import { BookFailure, type BookInfo } from "./book";
import type { PageViewProps } from "./PageView";

const boundary = vi.hoisted(() => ({
  select: vi.fn(),
  read: vi.fn(),
  views: [] as PageViewProps[],
}));
vi.mock("./book", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./book")>();
  return {
    ...actual,
    selectBook: boundary.select,
    readFirstChapter: boundary.read,
  };
});
// jsdom cannot measure text layout. The actual PageView is covered in Chromium.
// Keep preparation under test control to verify loading until pagination finishes.
vi.mock("./PageView", () => ({
  PageView: (props: PageViewProps) => {
    boundary.views.push(props);
    return (
      <div role="region" aria-label="本文表示部品" data-page={props.page}>
        {props.xhtml}
      </div>
    );
  },
}));
const info: BookInfo = {
  id: "1",
  title: "森の物語",
  chapters: ["OPS/first.xhtml"],
  toc: [],
  resource_base: "http://book.localhost/1/",
};
const chapter = {
  path: "OPS/first.xhtml",
  title: "朝の道",
  xhtml: "最初の章の本文",
};
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
async function finishPreparation() {
  await waitFor(() => expect(boundary.views.length).toBeGreaterThan(0));
  await act(async () => {
    const view = boundary.views[boundary.views.length - 1]!;
    view.onPagination({ page: view.page, count: 3 });
  });
}
async function openReady(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "書籍を開く" }));
  await finishPreparation();
}
beforeEach(() => {
  boundary.select.mockReset().mockResolvedValue(info);
  boundary.read.mockReset().mockResolvedValue(chapter);
  boundary.views.length = 0;
});

describe("書籍選択と本文準備", () => {
  it("未選択の案内からタイトル・章名・章内ページ位置を表示する", async () => {
    const user = userEvent.setup();
    render(<Reader />);
    expect(
      screen.getByRole("heading", { name: "読む本を選ぶ" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "次へ" }),
    ).not.toBeInTheDocument();
    await openReady(user);
    expect(screen.getByText("森の物語")).toBeInTheDocument();
    expect(screen.getByText("朝の道")).toBeInTheDocument();
    expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
      "1 / 3",
    );
    expect(screen.getByRole("button", { name: "書籍を開く" })).toBeEnabled();
  });
  it("選択・章取得・表示準備の間は開く操作を無効にし、重複要求を防ぐ", async () => {
    const selected = deferred<BookInfo | null>();
    const text = deferred<typeof chapter>();
    boundary.select.mockReturnValue(selected.promise);
    boundary.read.mockReturnValue(text.promise);
    render(<Reader />);
    const open = screen.getByRole("button", { name: "書籍を開く" });
    fireEvent.click(open);
    fireEvent.click(open);
    expect(open).toBeDisabled();
    expect(boundary.select).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("status")).toHaveTextContent("読み込");
    await act(async () => selected.resolve(info));
    expect(open).toBeDisabled();
    await act(async () => text.resolve(chapter));
    expect(open).toBeDisabled();
    await finishPreparation();
    expect(open).toBeEnabled();
  });
  it("選択キャンセルで現在の本文とページを保持する", async () => {
    const user = userEvent.setup();
    render(<Reader />);
    await openReady(user);
    await user.click(screen.getByRole("button", { name: "次へ" }));
    const priorView = screen.getByRole("region", { name: "本文表示部品" });
    boundary.select.mockResolvedValue(null);
    await user.click(screen.getByRole("button", { name: "書籍を開く" }));
    expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
      "2 / 3",
    );
    expect(screen.getByRole("region", { name: "本文表示部品" })).toBe(
      priorView,
    );
    expect(boundary.read).toHaveBeenCalledTimes(1);
  });
  it("初回のキャンセルでは未選択案内を維持する", async () => {
    boundary.select.mockResolvedValue(null);
    const user = userEvent.setup();
    render(<Reader />);
    await user.click(screen.getByRole("button", { name: "書籍を開く" }));
    expect(
      screen.getByRole("heading", { name: "読む本を選ぶ" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

describe("ページ操作と再選択", () => {
  it("前後ボタン・左右キーを共通の章内ページ境界で制限する", async () => {
    const user = userEvent.setup();
    render(<Reader />);
    await openReady(user);
    const position = screen.getByLabelText("この章のページ位置");
    expect(screen.getByRole("button", { name: "前へ" })).toBeDisabled();
    await user.keyboard("{ArrowLeft}");
    expect(position).toHaveTextContent("1 / 3");
    await user.keyboard("{ArrowRight}");
    expect(position).toHaveTextContent("2 / 3");
    await user.click(screen.getByRole("button", { name: "次へ" }));
    expect(position).toHaveTextContent("3 / 3");
    expect(screen.getByRole("button", { name: "次へ" })).toBeDisabled();
    await user.keyboard("{ArrowRight}");
    expect(position).toHaveTextContent("3 / 3");
    await user.click(screen.getByRole("button", { name: "前へ" }));
    expect(position).toHaveTextContent("2 / 3");
    await user.keyboard("{ArrowLeft}");
    expect(position).toHaveTextContent("1 / 3");
  });
  it("本文iframeの左右キーを受け、入力・修飾キー・合成入力では移動しない", async () => {
    const user = userEvent.setup();
    render(<Reader />);
    await openReady(user);
    const fromFrame = boundary.views[boundary.views.length - 1];
    const event = new KeyboardEvent("keydown", {
      key: "ArrowRight",
      cancelable: true,
    });
    await act(async () => fromFrame.onKeyDown?.(event));
    expect(event.defaultPrevented).toBe(true);
    expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
      "2 / 3",
    );
    for (const modifier of [
      "ctrlKey",
      "altKey",
      "metaKey",
      "shiftKey",
      "isComposing",
    ]) {
      fireEvent.keyDown(document, { key: "ArrowRight", [modifier]: true });
    }
    for (const tag of ["input", "textarea", "select"]) {
      const input = document.createElement(tag);
      document.body.append(input);
      fireEvent.keyDown(input, { key: "ArrowRight" });
      input.remove();
    }
    const editable = document.createElement("div");
    editable.setAttribute("contenteditable", "true");
    document.body.append(editable);
    fireEvent.keyDown(editable, { key: "ArrowRight" });
    editable.remove();
    expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
      "2 / 3",
    );
    // An event already handled by another control must keep its meaning.
    const handled = new KeyboardEvent("keydown", {
      key: "ArrowRight",
      cancelable: true,
    });
    handled.preventDefault();
    await act(async () => fromFrame.onKeyDown?.(handled));
    expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
      "2 / 3",
    );
  });
  it("別の書籍ではページを先頭へ戻し、古い表示部品からの通知を無視する", async () => {
    const user = userEvent.setup();
    render(<Reader />);
    await openReady(user);
    const old = boundary.views[boundary.views.length - 1];
    await user.click(screen.getByRole("button", { name: "次へ" }));
    boundary.select.mockResolvedValue({
      ...info,
      id: "2",
      title: "次の物語",
      resource_base: "http://book.localhost/2/",
    });
    boundary.read.mockResolvedValue({
      ...chapter,
      xhtml: "次の書籍の本文",
      title: "次の章",
    });
    await user.click(screen.getByRole("button", { name: "書籍を開く" }));
    await waitFor(() =>
      expect(boundary.views[boundary.views.length - 1].xhtml).toBe(
        "次の書籍の本文",
      ),
    );
    await finishPreparation();
    await act(async () => {
      old.onPagination({ page: 8, count: 10 });
      old.onError(new Error("old error"));
      old.onKeyDown?.(new KeyboardEvent("keydown", { key: "ArrowRight" }));
    });
    expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
      "1 / 3",
    );
    expect(screen.getByText("次の物語")).toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: "本文表示部品" }),
    ).toHaveTextContent("次の書籍の本文");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
  it("書籍の再選択をキャンセルした後も本文のページ通知が有効である", async () => {
    const user = userEvent.setup();
    render(<Reader />);
    await openReady(user);
    const view = boundary.views[boundary.views.length - 1];
    boundary.select.mockResolvedValue(null);
    await user.click(screen.getByRole("button", { name: "書籍を開く" }));
    await act(async () => view.onPagination({ page: 2, count: 5 }));
    expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
      "3 / 5",
    );
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
      "2 / 5",
    );
  });
});

for (const code of [
  "unreadable",
  "invalid",
  "unsupported",
  "missing_resource",
] as const) {
  it(`${code}の案内後に別の書籍を選べる`, async () => {
    boundary.select.mockRejectedValueOnce(new BookFailure(code));
    const user = userEvent.setup();
    render(<Reader />);
    await user.click(screen.getByRole("button", { name: "書籍を開く" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      new BookFailure(code).message,
    );
    expect(screen.getByRole("button", { name: "書籍を開く" })).toBeEnabled();
    await openReady(user);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
      "1 / 3",
    );
  });
}

it("画像・CSSの表示失敗を通知して再選択を可能にする", async () => {
  const user = userEvent.setup();
  render(<Reader />);
  await user.click(screen.getByRole("button", { name: "書籍を開く" }));
  await waitFor(() => expect(boundary.views.length).toBeGreaterThan(0));
  await act(async () =>
    boundary.views[boundary.views.length - 1].onError(
      new Error("decode failed"),
    ),
  );
  expect(screen.getByRole("alert")).toHaveTextContent(
    new BookFailure("missing_resource").message,
  );
  expect(screen.getByRole("button", { name: "書籍を開く" })).toBeEnabled();
  await openReady(user);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("選択成功後の章取得失敗で旧本文と新タイトルを混ぜず、再選択できる", async () => {
  const user = userEvent.setup();
  render(<Reader />);
  await openReady(user);
  boundary.select.mockResolvedValueOnce({
    ...info,
    id: "2",
    title: "新しいタイトル",
  });
  boundary.read.mockRejectedValueOnce(new BookFailure("missing_resource"));
  await user.click(screen.getByRole("button", { name: "書籍を開く" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    new BookFailure("missing_resource").message,
  );
  expect(screen.queryByText("新しいタイトル")).not.toBeInTheDocument();
  expect(screen.queryByText(chapter.xhtml)).not.toBeInTheDocument();
  await openReady(user);
  expect(screen.getByText(info.title)).toBeInTheDocument();
});

it("アンマウント前の章取得応答を、次に開いた画面へ適用しない", async () => {
  const oldText = deferred<typeof chapter>();
  boundary.read.mockReturnValueOnce(oldText.promise);
  const user = userEvent.setup();
  const first = render(<Reader />);
  await user.click(screen.getByRole("button", { name: "書籍を開く" }));
  await waitFor(() => expect(boundary.read).toHaveBeenCalledTimes(1));
  first.unmount();
  boundary.select.mockResolvedValue({ ...info, id: "2", title: "現在の書籍" });
  boundary.read.mockResolvedValue({ ...chapter, xhtml: "現在の本文" });
  render(<Reader />);
  await openReady(user);
  await act(async () => oldText.resolve(chapter));
  expect(
    screen.getByRole("region", { name: "本文表示部品" }),
  ).toHaveTextContent("現在の本文");
  expect(screen.getByText("現在の書籍")).toBeInTheDocument();
});

it("再選択中の旧本文の再配置通知で開く操作を有効にせず、キャンセル後にフォーカスを戻す", async () => {
  const user = userEvent.setup();
  render(<Reader />);
  await openReady(user);
  const view = boundary.views[boundary.views.length - 1];
  const selected = deferred<BookInfo | null>();
  boundary.select.mockReturnValueOnce(selected.promise);
  const open = screen.getByRole("button", { name: "書籍を開く" });
  await user.click(open);
  await act(async () => view.onPagination({ page: 1, count: 5 }));
  expect(open).toBeDisabled();
  expect(screen.getByRole("status")).toHaveAttribute("aria-live", "polite");
  fireEvent.keyDown(document, { key: "ArrowRight" });
  await act(async () => selected.resolve(null));
  expect(open).toBeEnabled();
  expect(open).toHaveFocus();
  expect(screen.getByLabelText("この章のページ位置")).toHaveTextContent(
    "2 / 5",
  );
});

it("エラー画面からのキャンセルでは、その案内を維持する", async () => {
  boundary.select
    .mockRejectedValueOnce(new BookFailure("invalid"))
    .mockResolvedValueOnce(null);
  const user = userEvent.setup();
  render(<Reader />);
  await user.click(screen.getByRole("button", { name: "書籍を開く" }));
  const message = screen.getByRole("alert").textContent;
  await user.click(screen.getByRole("button", { name: "書籍を開く" }));
  expect(screen.getByRole("alert").textContent).toBe(message);
  expect(screen.getByRole("button", { name: "書籍を開く" })).toHaveFocus();
});
