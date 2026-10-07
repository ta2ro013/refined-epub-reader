import { useCallback, useEffect, useRef, useState } from "react";
import { PageView } from "./PageView";
import {
  asBookFailure,
  readFirstChapter,
  selectBook,
  type BookInfo,
  type Chapter,
  type BookFailure,
} from "./book";

type Reading = { info: BookInfo; chapter: Chapter; version: number };
export function Reader() {
  const [reading, setReading] = useState<Reading | null>(null);
  const [busy, setBusy] = useState(false);
  const [ready, setReady] = useState(false);
  const [failure, setFailure] = useState<BookFailure | null>(null);
  const [position, setPosition] = useState({ page: 0, count: 1 });
  const gate = useRef(false);
  const operation = useRef(0);
  const activeReading = useRef<Reading | null>(null);
  const openButton = useRef<HTMLButtonElement>(null);
  const restoreFocus = useRef(false);

  useEffect(
    () => () => {
      operation.current++;
      activeReading.current = null;
    },
    [],
  );
  useEffect(() => {
    if (!busy && restoreFocus.current) {
      openButton.current?.focus();
      restoreFocus.current = false;
    }
  }, [busy]);
  const finish = () => {
    gate.current = false;
    setBusy(false);
  };
  async function open() {
    if (gate.current) return;
    gate.current = true;
    restoreFocus.current = true;
    setBusy(true);
    const version = ++operation.current;
    try {
      const info = await selectBook();
      if (version !== operation.current) return;
      if (!info) {
        finish();
        return;
      }
      activeReading.current = null;
      setReading(null);
      setReady(false);
      setFailure(null);
      setPosition({ page: 0, count: 1 });
      const chapter = await readFirstChapter(info);
      if (version !== operation.current) return;
      const next = { info, chapter, version };
      activeReading.current = next;
      setReading(next);
    } catch (reason) {
      if (version !== operation.current) return;
      setFailure(asBookFailure(reason));
      finish();
    }
  }
  function paginate(version: number, result: { page: number; count: number }) {
    if (activeReading.current?.version !== version) return;
    setPosition(result);
    setReady(true);
    if (operation.current === version) finish();
  }
  function displayError(version: number, reason: unknown) {
    if (activeReading.current?.version !== version) return;
    setFailure(asBookFailure(reason, "missing_resource"));
    setReady(false);
    if (operation.current === version) finish();
  }
  const canRead = Boolean(reading && ready && !busy && !failure);
  const move = useCallback(
    (delta: number) => {
      if (!canRead) return;
      setPosition((current) => ({
        ...current,
        page: Math.max(0, Math.min(current.count - 1, current.page + delta)),
      }));
    },
    [canRead],
  );
  const handleKey = useCallback(
    (event: KeyboardEvent, version?: number) => {
      if (version !== undefined && activeReading.current?.version !== version)
        return;
      if (
        !canRead ||
        gate.current ||
        event.defaultPrevented ||
        event.isComposing ||
        event.altKey ||
        event.ctrlKey ||
        event.metaKey ||
        event.shiftKey
      )
        return;
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      // The target can belong to the iframe's realm, so do not use instanceof Element.
      const target = event.target as HTMLElement | null;
      if (
        target?.nodeType === 1 &&
        (target.isContentEditable ||
          target.closest(
            'input,textarea,select,[contenteditable]:not([contenteditable="false"]),[role="textbox"],[role="combobox"],[role="slider"],[role="spinbutton"]',
          ))
      )
        return;
      event.preventDefault();
      move(event.key === "ArrowRight" ? 1 : -1);
    },
    [canRead, move],
  );
  useEffect(() => {
    const key = (event: KeyboardEvent) => handleKey(event);
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [handleKey]);
  return (
    <main className="reader-window" aria-label="EPUBリーダー">
      <header className="reader-toolbar">
        <h1 className="reader-book-title" title={reading?.info.title}>
          {reading?.info.title || "EPUB Reader"}
        </h1>
        <button
          ref={openButton}
          className="reader-open"
          type="button"
          disabled={busy}
          onClick={() => void open()}
        >
          書籍を開く
        </button>
      </header>
      <section className="reader-content" aria-label="読書領域">
        {reading && (
          <div
            className={`reader-document${canRead ? "" : " reader-document-muted"}`}
            inert={!canRead}
            aria-hidden={!canRead}
          >
            <PageView
              key={reading.version}
              xhtml={reading.chapter.xhtml}
              chapterPath={reading.chapter.path}
              resourceBase={reading.info.resource_base}
              page={position.page}
              onPagination={(result) => paginate(reading.version, result)}
              onError={(reason) => displayError(reading.version, reason)}
              onKeyDown={(event) => handleKey(event, reading.version)}
            />
          </div>
        )}
        {busy ? (
          <section
            className="reader-state"
            role="status"
            aria-live="polite"
            aria-atomic="true"
          >
            <h2>書籍を読み込んでいます</h2>
            <p>本文を準備しています。</p>
            <div className="reader-loading-lines" aria-hidden="true">
              <span />
              <span />
              <span />
            </div>
          </section>
        ) : failure ? (
          <section className="reader-state" role="alert" aria-atomic="true">
            <h2>この書籍を開けませんでした</h2>
            <p>{failure.message}</p>
            <p>「書籍を開く」から、もう一度選んでください。</p>
          </section>
        ) : !reading ? (
          <section className="reader-state">
            <h2>読む本を選ぶ</h2>
            <p>「書籍を開く」から、EPUBファイルを選んでください。</p>
            <p>EPUB 2・3の横書きリフロー型に対応します。</p>
          </section>
        ) : null}
      </section>
      <footer className="reader-footer">
        {reading && ready && !failure && (
          <>
            <span className="reader-chapter" title={reading.chapter.title}>
              {reading.chapter.title}
            </span>
            <nav className="reader-navigation" aria-label="章内のページ移動">
              <button
                type="button"
                disabled={!canRead || position.page === 0}
                onClick={() => move(-1)}
              >
                前へ
              </button>
              <span
                aria-label="この章のページ位置"
                aria-live="polite"
                aria-atomic="true"
              >
                {position.page + 1} / {position.count}
              </span>
              <button
                type="button"
                disabled={!canRead || position.page >= position.count - 1}
                onClick={() => move(1)}
              >
                次へ
              </button>
            </nav>
          </>
        )}
      </footer>
    </main>
  );
}
