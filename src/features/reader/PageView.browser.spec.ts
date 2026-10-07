import { expect, test } from "@playwright/test";

test("長い章を横に分割し、最後の本文まで移動できる", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await page.goto("/src/features/reader/page-fixture.html");
  await expect(page.locator("#status")).toHaveText(/^1 \/ \d+$/);
  const count = Number(
    (await page.locator("#status").innerText()).split("/")[1],
  );
  expect(count).toBeGreaterThan(3);
  await page.screenshot({ path: "/tmp/reader-page-light.png" });
  await page.evaluate((last) => {
    (
      window as unknown as { readerFixture: { go: (page: number) => void } }
    ).readerFixture.go(last);
  }, count - 1);
  await expect(page.locator("#status")).toHaveText(`${count} / ${count}`);
  const rect = await page
    .frameLocator("iframe")
    .locator("#last")
    .evaluate((element) => {
      const r = element.getBoundingClientRect();
      return {
        x: r.x,
        y: r.y,
        right: r.right,
        bottom: r.bottom,
        width: innerWidth,
        height: innerHeight,
      };
    });
  expect(rect.x).toBeGreaterThanOrEqual(0);
  expect(rect.y).toBeGreaterThanOrEqual(0);
  expect(rect.right).toBeLessThanOrEqual(rect.width);
  expect(rect.bottom).toBeLessThanOrEqual(rect.height);
});

async function configure(
  page: import("@playwright/test").Page,
  options: Record<string, unknown>,
) {
  await page.evaluate((value) => {
    (
      window as unknown as {
        readerFixture: {
          configure: (options: Record<string, unknown>) => void;
        };
      }
    ).readerFixture.configure(value);
  }, options);
}

for (const theme of ["light", "dark"] as const) {
  test(`${theme}: 大きい画面から縮めてもページ先頭の文字を保持する`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/src/features/reader/page-fixture.html");
    await configure(page, { theme });
    await expect(page.locator("#status")).toHaveText(/^1 \/ \d+$/);
    for (let i = 0; i < 4; i++) await page.locator("#next").click();
    await expect(page.locator("#status")).toHaveText(/^5 \/ /);
    const anchor = await page
      .frameLocator("iframe")
      .locator("#reader-body")
      .evaluate((main) => {
        const walker = document.createTreeWalker(main, NodeFilter.SHOW_TEXT);
        let node: Node | null;
        while ((node = walker.nextNode())) {
          if (!node.parentElement?.id.startsWith("p")) continue;
          for (
            let offset = 0;
            offset < (node.textContent?.length ?? 0);
            offset++
          ) {
            const range = document.createRange();
            range.setStart(node, offset);
            range.setEnd(node, offset + 1);
            const r = range.getBoundingClientRect();
            if (
              r.x >= 20 &&
              r.right <= innerWidth - 20 &&
              r.y >= 20 &&
              r.bottom <= innerHeight - 20
            )
              return { id: node.parentElement!.id, offset };
          }
        }
        throw new Error("本文位置が見つからない");
      });
    await page.setViewportSize({ width: 800, height: 600 });
    await expect
      .poll(async () =>
        page
          .frameLocator("iframe")
          .locator(`#${anchor.id}`)
          .evaluate((p, offset) => {
            const range = document.createRange();
            range.setStart(p.firstChild!, offset);
            range.setEnd(p.firstChild!, offset + 1);
            const r = range.getBoundingClientRect();
            return (
              r.x >= 0 &&
              r.right <= innerWidth &&
              r.y >= 0 &&
              r.bottom <= innerHeight
            );
          }, anchor.offset),
      )
      .toBe(true);
    const color = await page
      .frameLocator("iframe")
      .locator("body")
      .evaluate(() => getComputedStyle(document.documentElement).color);
    expect(color).toBe(
      theme === "dark" ? "rgb(232, 236, 239)" : "rgb(35, 40, 45)",
    );
  });
}

test("本文のスクリプト・外部通信・フォーム・埋め込み・遷移を除去する", async ({
  page,
}) => {
  const requests: string[] = [];
  page.on("request", (request) => {
    if (request.url().includes("evil.example")) requests.push(request.url());
  });
  await page.goto("/src/features/reader/page-fixture.html");
  await configure(page, {
    xhtml: `<html xmlns="http://www.w3.org/1999/xhtml"><head><base href="https://evil.example/"/><meta http-equiv="refresh" content="0;url=https://evil.example/"/><link rel="stylesheet" href="https://evil.example/a.css"/><style>@import 'https://evil.example/b.css';p {background:url('https://evil.example/a.png')}</style></head><body onload="parent.pwned=1"><script>parent.pwned=1;parent.__TAURI_INTERNALS__.invoke('select_book')</script><p id="safe" onclick="parent.pwned=2">本文</p><img src="https://evil.example/img.png"/><iframe src="https://evil.example/"/><form action="https://evil.example/"><input/><button>送信</button></form><a href="https://evil.example/">外部リンク</a></body></html>`,
  });
  await expect(page.frameLocator("iframe").locator("#safe")).toHaveText("本文");
  await expect(
    page
      .frameLocator("iframe")
      .locator(
        'script,iframe,form,input,button,base,meta[http-equiv="refresh"],link,[onload],[onclick],[href]',
      ),
  ).toHaveCount(0);
  expect(requests).toEqual([]);
  expect(
    await page.evaluate(() => Reflect.get(window, "pwned")),
  ).toBeUndefined();
  const before = page.url();
  await page.frameLocator("iframe").getByText("外部リンク").click();
  expect(page.url()).toBe(before);
});

test("欠落した書籍画像とCSSは呼び出し側へ通知する", async ({ page }) => {
  await page.route("**/book/1/**", (route) =>
    route.fulfill({ status: 404, body: "missing" }),
  );
  await page.goto("/src/features/reader/page-fixture.html");
  await configure(page, {
    xhtml:
      '<html xmlns="http://www.w3.org/1999/xhtml"><head><link rel="stylesheet" href="styles/missing.css"/></head><body><img src="images/missing.png"/></body></html>',
  });
  await expect(page.locator("#status")).toHaveText(/^エラー:/);
});

for (const size of [
  { width: 800, height: 600 },
  { width: 1280, height: 800 },
]) {
  test(`${size.width}×${size.height}: 全文字とルビを切らずに分割する`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/src/features/reader/page-fixture.html");
    await expect(page.locator("#status")).toHaveText(/^1 \/ \d+$/);
    const result = await page
      .frameLocator("iframe")
      .locator("#reader-body")
      .evaluate((main) => {
        const errors: string[] = [];
        const walker = document.createTreeWalker(main, NodeFilter.SHOW_TEXT);
        let node: Node | null,
          letters = 0,
          ruby = 0;
        const inset = parseFloat(getComputedStyle(document.body).paddingLeft);
        while ((node = walker.nextNode())) {
          for (let i = 0; i < (node.textContent?.length ?? 0); i++) {
            const range = document.createRange();
            range.setStart(node, i);
            range.setEnd(node, i + 1);
            const rect = range.getBoundingClientRect();
            if (!rect.width || !node.textContent![i].trim()) continue;
            letters++;
            if (node.parentElement?.localName === "rt") ruby++;
            const column = Math.floor((rect.left - inset + 0.5) / innerWidth);
            const x = rect.left - column * innerWidth;
            if (
              x < inset - 1 ||
              rect.right - column * innerWidth > innerWidth - inset + 1 ||
              rect.top < inset - 1 ||
              rect.bottom > innerHeight - inset + 1
            )
              errors.push(`${node.parentElement?.id}:${i}`);
          }
        }
        return {
          errors,
          letters,
          ruby,
          vertical: document.documentElement.scrollHeight > innerHeight,
        };
      });
    expect(result.letters).toBeGreaterThan(10000);
    expect(result.ruby).toBe(240);
    expect(result.errors).toEqual([]);
    expect(result.vertical).toBe(false);
  });
}

test("PNG・JPEG・GIFを実デコードし、大きい画像の比率と画像ページの位置を保持する", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/src/features/reader/page-fixture.html");
  const images = await page.evaluate(() => {
    const canvas = document.createElement("canvas");
    canvas.width = 1600;
    canvas.height = 2400;
    const ctx = canvas.getContext("2d")!;
    ctx.fillStyle = "#215e53";
    ctx.fillRect(0, 0, 1600, 2400);
    return {
      png: canvas.toDataURL("image/png").split(",")[1],
      jpeg: canvas.toDataURL("image/jpeg").split(",")[1],
      gif: "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7",
    };
  });
  // @ts-expect-error Node is provided by Playwright; the frontend intentionally has no Node typings.
  const { Buffer } = await import("node:buffer");
  await page.route("**/book/1/OPS/images/**", async (route) => {
    const type = route.request().url().split(".").pop() as keyof typeof images;
    await new Promise((resolve) => setTimeout(resolve, 100));
    await route.fulfill({
      contentType: `image/${type}`,
      body: Buffer.from(images[type], "base64"),
    });
  });
  await page.route("**/book/1/OPS/styles/book.css", (route) =>
    route.fulfill({
      contentType: "text/css",
      body: "p { font-weight:700; font-size:90px; min-height:9000px; width:9000px; writing-mode:vertical-rl } h1 {text-decoration:underline}",
    }),
  );
  await configure(page, {
    xhtml:
      '<html xmlns="http://www.w3.org/1999/xhtml"><head><link rel="stylesheet" href="styles/book.css"/></head><body><h1>画像の章</h1><p id="decorated">装飾された本文</p><img id="png" src="images/picture.png"/><img id="jpeg" src="images/picture.jpeg"/><img id="gif" src="images/picture.gif"/><p id="last">最後の画像まで表示</p></body></html>',
  });
  await expect(page.locator("#status")).toHaveText(/^1 \/ \d+$/);
  const info = await page
    .frameLocator("iframe")
    .locator("#reader-body")
    .evaluate((main) => {
      const images = Array.from(main.querySelectorAll("img")).map((img) => ({
        width: img.width,
        height: img.height,
        naturalWidth: img.naturalWidth,
        naturalHeight: img.naturalHeight,
        x: img.getBoundingClientRect().x,
        y: img.getBoundingClientRect().y,
      }));
      const style = getComputedStyle(main.querySelector("p")!);
      return {
        images,
        width: innerWidth,
        height: innerHeight,
        weight: style.fontWeight,
        font: style.fontSize,
        direction: style.writingMode,
      };
    });
  expect(info.weight).toBe("700");
  expect(info.font).toBe("20px");
  expect(info.direction).toBe("horizontal-tb");
  expect(info.images).toHaveLength(3);
  for (const image of info.images) {
    expect(image.naturalWidth).toBeGreaterThan(0);
    expect(image.width).toBeLessThanOrEqual(info.width - 64);
    expect(image.height).toBeLessThanOrEqual(info.height - 64);
    expect(
      Math.abs(
        image.width / image.height - image.naturalWidth / image.naturalHeight,
      ),
    ).toBeLessThan(0.01);
  }
  const imagePage = Math.floor((info.images[1].x - 32) / info.width);
  await page.evaluate(
    (value) =>
      (
        window as unknown as { readerFixture: { go: (value: number) => void } }
      ).readerFixture.go(value),
    imagePage,
  );
  await expect(page.locator("#status")).toHaveText(
    new RegExp(`^${imagePage + 1} / `),
  );
  await page.setViewportSize({ width: 800, height: 600 });
  await expect
    .poll(() =>
      page
        .frameLocator("iframe")
        .locator("#jpeg")
        .evaluate((img) => {
          const r = img.getBoundingClientRect();
          return (
            r.x >= 0 &&
            r.right <= innerWidth &&
            r.y >= 0 &&
            r.bottom <= innerHeight
          );
        }),
    )
    .toBe(true);
});

for (const [name, body] of [
  ["画像", '<img src="images/missing.png"/>'],
  ["壊れた画像", '<img src="images/broken.png"/>'],
] as const) {
  test(`${name}の取得・デコード失敗を通知する`, async ({ page }) => {
    await page.route("**/book/1/**", (route) =>
      route.fulfill({
        status: name === "画像" ? 404 : 200,
        contentType: "image/png",
        body: "not an image",
      }),
    );
    await page.goto("/src/features/reader/page-fixture.html");
    await configure(page, {
      xhtml: `<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body>${body}</body></html>`,
    });
    await expect(page.locator("#status")).toHaveText(/^エラー:/);
  });
}

test("CSS自身のパスから書籍内の背景画像を解決する", async ({ page }) => {
  const requests: string[] = [];
  await page.route("**/book/1/OPS/styles/book.css", (route) =>
    route.fulfill({
      contentType: "text/css",
      body: '.ornament {background-image:url("../images/paper.png");font-style:italic}',
    }),
  );
  await page.route("**/book/1/OPS/images/paper.png", (route) => {
    requests.push(route.request().url());
    return route.fulfill({
      contentType: "image/png",
      path: "src-tauri/icons/128x128.png",
    });
  });
  await page.goto("/src/features/reader/page-fixture.html");
  await configure(page, {
    xhtml:
      '<html xmlns="http://www.w3.org/1999/xhtml"><head><link rel="stylesheet" href="styles/book.css"/></head><body><p class="ornament">背景画像の本文</p></body></html>',
  });
  await expect(page.locator("#status")).toHaveText("1 / 1");
  await expect
    .poll(() =>
      page
        .frameLocator("iframe")
        .locator(".ornament")
        .evaluate((p) => getComputedStyle(p).backgroundImage),
    )
    .toContain("/book/1/OPS/images/paper.png");
  expect(requests.length).toBeGreaterThan(0);
});

test("構造要素の本文を保持し、巨大な余白や予約IDに組版を壊されない", async ({
  page,
}) => {
  await page.goto("/src/features/reader/page-fixture.html");
  await configure(page, {
    xhtml:
      '<html xmlns="http://www.w3.org/1999/xhtml"><head><style>#reader-body {margin:9999px} p{padding:9999px;text-indent:9999px}</style></head><body><main><aside><p id="preserved" style="margin:9000px">保持される本文</p></aside></main><div id="reader-body">予約IDの本文</div></body></html>',
  });
  await expect(page.frameLocator("iframe").locator("#preserved")).toHaveText(
    "保持される本文",
  );
  await expect(page.frameLocator("iframe").locator("#reader-body")).toHaveCount(
    1,
  );
  const rect = await page
    .frameLocator("iframe")
    .locator("#preserved")
    .evaluate((p) => {
      const range = document.createRange();
      range.selectNodeContents(p);
      const r = range.getBoundingClientRect();
      return {
        left: r.left,
        right: r.right,
        top: r.top,
        bottom: r.bottom,
        width: innerWidth,
        height: innerHeight,
      };
    });
  expect(rect.left).toBeGreaterThanOrEqual(0);
  expect(rect.right).toBeLessThanOrEqual(rect.width);
  expect(rect.top).toBeGreaterThanOrEqual(0);
  expect(rect.bottom).toBeLessThanOrEqual(rect.height);
});

test("本文のキーイベントを親へ渡し、古いCSS応答を新しい章へ適用しない", async ({
  page,
}) => {
  let release!: () => void;
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route("**/book/1/OPS/slow.css", async (route) => {
    await pending;
    await route
      .fulfill({ contentType: "text/css", body: "p{font-weight:bold}" })
      .catch(() => {});
  });
  await page.goto("/src/features/reader/page-fixture.html");
  await configure(page, {
    xhtml:
      '<html xmlns="http://www.w3.org/1999/xhtml"><head><link rel="stylesheet" href="slow.css"/></head><body><p>古い章</p></body></html>',
  });
  await configure(page, {
    xhtml:
      '<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><p id="new">新しい章</p></body></html>',
  });
  await expect(page.locator("#status")).toHaveText("1 / 1");
  release();
  await expect(page.frameLocator("iframe").locator("#new")).toHaveText(
    "新しい章",
  );
  await page.frameLocator("iframe").locator("body").click();
  await page.keyboard.press("ArrowRight");
  expect(await page.evaluate(() => Reflect.get(window, "readerKey"))).toBe(
    "ArrowRight",
  );
  await expect(page.frameLocator("iframe").getByText("古い章")).toHaveCount(0);
});

test("CSS画像の欠落を通知する", async ({ page }) => {
  await page.route("**/book/1/OPS/styles/book.css", (route) =>
    route.fulfill({
      contentType: "text/css",
      body: 'p{background-image:url("../images/missing.png")}',
    }),
  );
  await page.route("**/book/1/OPS/images/missing.png", (route) =>
    route.fulfill({ status: 404, body: "missing" }),
  );
  await page.goto("/src/features/reader/page-fixture.html");
  await configure(page, {
    xhtml:
      '<html xmlns="http://www.w3.org/1999/xhtml"><head><link rel="stylesheet" href="styles/book.css"/></head><body><p>本文</p></body></html>',
  });
  await expect(page.locator("#status")).toHaveText(/^エラー:/);
});
