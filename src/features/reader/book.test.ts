import { afterEach, describe, expect, it } from "vitest";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import {
  asBookFailure,
  readFirstChapter,
  selectBook,
  type BookInfo,
} from "./book";

const xhtml =
  '<html xmlns="http://www.w3.org/1999/xhtml"><head/><body><h1>本文の見出し</h1><p>最初の本文</p></body></html>';
const book: BookInfo = {
  id: "42",
  title: "森の物語",
  chapters: ["OPS/first.xhtml", "OPS/second.xhtml"],
  resource_base: "http://book.localhost/42/",
  toc: [
    {
      label: "読書",
      path: null,
      fragment: null,
      children: [
        {
          label: "最初の章",
          path: "OPS/first.xhtml",
          fragment: "section",
          children: [],
        },
      ],
    },
  ],
};
afterEach(clearMocks);

describe("書籍コマンドの境界", () => {
  it("標準ファイル選択の結果を取得し、最初の読み順の章を取り出す", async () => {
    mockIPC((command, args) => {
      if (command === "select_book") return book;
      if (
        command === "read_chapter" &&
        args &&
        "bookId" in args &&
        "index" in args &&
        args.bookId === "42" &&
        args.index === 0
      )
        return xhtml;
      throw "missing_resource";
    });
    const selected = await selectBook();
    expect(selected).toEqual(book);
    expect(await readFirstChapter(selected!)).toEqual({
      path: "OPS/first.xhtml",
      title: "最初の章",
      xhtml,
    });
  });
  it("選択キャンセルを正常な結果として返す", async () => {
    mockIPC(() => null);
    expect(await selectBook()).toBeNull();
  });
  it("Rustのエラー分類を保持する", async () => {
    mockIPC(() => {
      throw "unsupported";
    });
    await expect(selectBook()).rejects.toMatchObject({ code: "unsupported" });
  });
  it("目次に対応するラベルがない場合は本文の見出しを使う", async () => {
    mockIPC(() => xhtml);
    expect((await readFirstChapter({ ...book, toc: [] })).title).toBe(
      "本文の見出し",
    );
  });
  it("読み順の章が欠落している場合と不正XHTMLを区別する", async () => {
    mockIPC(() => "<html><body><p>閉じていない");
    await expect(
      readFirstChapter({ ...book, chapters: [] }),
    ).rejects.toMatchObject({ code: "missing_resource" });
    await expect(readFirstChapter(book)).rejects.toMatchObject({
      code: "invalid",
    });
  });
});

it("4分類を利用者へ別々に案内し、不明なエラーの詳細を露出しない", () => {
  const failures = [
    "unreadable",
    "invalid",
    "unsupported",
    "missing_resource",
  ].map((reason) => asBookFailure(reason));
  expect(failures.map((error) => error.code)).toEqual([
    "unreadable",
    "invalid",
    "unsupported",
    "missing_resource",
  ]);
  expect(new Set(failures.map((error) => error.message)).size).toBe(4);
  expect(
    asBookFailure(new Error("/private/local/book.epub")).message,
  ).not.toContain("/private");
});
