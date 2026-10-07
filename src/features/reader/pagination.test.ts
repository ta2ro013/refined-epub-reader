import { describe, expect, it } from "vitest";
import { pageCount, clampPage, pageAt, resourceUrl } from "./pagination";

describe("ページ境界", () => {
  it("空の章も1ページとし、端数と計測誤差を扱う", () => {
    expect(pageCount(0, 600)).toBe(1);
    expect(pageCount(1800, 600)).toBe(3);
    expect(pageCount(1800.2, 600)).toBe(3);
    expect(pageCount(1802, 600)).toBe(4);
  });
  it("指定ページを章の範囲に収める", () => {
    expect(clampPage(-1, 3)).toBe(0);
    expect(clampPage(1, 3)).toBe(1);
    expect(clampPage(9, 3)).toBe(2);
  });
  it("本文位置を含むページを求める", () => {
    expect(pageAt(599, 600)).toBe(0);
    expect(pageAt(600, 600)).toBe(1);
    expect(pageAt(1250, 600)).toBe(2);
  });
});

describe("書籍内の参照", () => {
  const base = "http://book.localhost/12/";
  it("章とCSSそれぞれの位置から相対参照を解決する", () => {
    expect(resourceUrl("../images/a.png", "OPS/text/ch1.xhtml", base)).toBe(
      base + "OPS/images/a.png",
    );
    expect(
      resourceUrl("../images/a.png?v=1", "OPS/styles/book.css", base),
    ).toBe(base + "OPS/images/a.png?v=1");
    expect(resourceUrl("%E6%A3%AE.png", "OPS/ch1.xhtml", base)).toBe(
      base + "OPS/%E6%A3%AE.png",
    );
  });
  it("外部・絶対・アーカイブ外・特殊URLを許可しない", () => {
    for (const url of [
      "https://example.com/a",
      "//example.com/a",
      "/etc/file",
      "../../file",
      "javascript:alert(1)",
      "data:image/png;base64,a",
      "a\\b",
      "%2e%2e/%2e%2e/file",
      "%2fetc",
      "#part",
    ]) {
      expect(resourceUrl(url, "OPS/ch1.xhtml", base)).toBeNull();
    }
  });
});
