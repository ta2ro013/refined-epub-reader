import { invoke } from "@tauri-apps/api/core";

export type TocEntry = {
  label: string;
  path: string | null;
  fragment: string | null;
  children: TocEntry[];
};
export type BookInfo = {
  id: string;
  title: string;
  chapters: string[];
  toc: TocEntry[];
  resource_base: string;
};
export type Chapter = { path: string; title: string; xhtml: string };
const messages = {
  unreadable:
    "ファイルを読み取れません。保存場所やアクセス権を確認して、もう一度選んでください。",
  invalid:
    "ファイルが破損しているか、EPUBの形式が正しくありません。別の書籍を選んでください。",
  unsupported:
    "この書籍の形式には対応していません。DRMのないEPUB 2・3の横書きリフロー型を選んでください。",
  missing_resource:
    "本文・画像・CSSなど、書籍に必要なデータが見つからないか、読み込めません。別の書籍を選んでください。",
  stale_book:
    "書籍が切り替わったため、本文を取得できません。もう一度書籍を選んでください。",
  busy: "別の書籍を選択中です。選択が終わってから、もう一度お試しください。",
  forbidden:
    "書籍を開けませんでした。アプリを再起動して、もう一度お試しください。",
  internal:
    "書籍を開けませんでした。もう一度選ぶか、アプリを再起動してください。",
};
export type ErrorCode = keyof typeof messages;
export class BookFailure extends Error {
  constructor(public readonly code: ErrorCode) {
    super(messages[code]);
    this.name = "BookFailure";
  }
}
export function asBookFailure(
  reason: unknown,
  fallback: ErrorCode = "internal",
): BookFailure {
  if (reason instanceof BookFailure) return reason;
  return new BookFailure(
    typeof reason === "string" &&
      Object.prototype.hasOwnProperty.call(messages, reason)
      ? (reason as ErrorCode)
      : fallback,
  );
}
export async function selectBook(): Promise<BookInfo | null> {
  try {
    return await invoke<BookInfo | null>("select_book");
  } catch (reason) {
    throw asBookFailure(reason);
  }
}
function tocLabel(entries: TocEntry[], path: string): string | undefined {
  for (const entry of entries) {
    if (entry.path === path && entry.label.trim()) return entry.label.trim();
    const child = tocLabel(entry.children, path);
    if (child) return child;
  }
}
export async function readFirstChapter(book: BookInfo): Promise<Chapter> {
  const path = book.chapters[0];
  if (!path) throw new BookFailure("missing_resource");
  try {
    const xhtml = await invoke<string>("read_chapter", {
      bookId: book.id,
      index: 0,
    });
    const document = new DOMParser().parseFromString(
      xhtml,
      "application/xhtml+xml",
    );
    if (
      document.querySelector("parsererror") ||
      document.querySelector("body")?.namespaceURI !==
        "http://www.w3.org/1999/xhtml"
    )
      throw new BookFailure("invalid");
    const title =
      tocLabel(book.toc, path) ||
      document.querySelector("h1,h2,h3")?.textContent?.trim() ||
      "最初の章";
    return { path, title, xhtml };
  } catch (reason) {
    throw asBookFailure(reason);
  }
}
