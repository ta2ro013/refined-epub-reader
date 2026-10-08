import { useLayoutEffect, useRef } from "react";
import { clampPage, pageAt, pageCount, resourceUrl } from "./pagination";
import {
  fontStacks,
  type ReaderTheme,
  type ReaderFont,
} from "./ReaderSettings";
import readerStyles from "./PageView.css?inline";

export type PageViewProps = {
  xhtml: string;
  chapterPath: string;
  resourceBase: string;
  page: number;
  theme?: ReaderTheme | "light" | "dark";
  fontSize?: number;
  fontFamily?: ReaderFont;
  onPagination: (result: { page: number; count: number }) => void;
  onError: (error: Error) => void;
  onKeyDown?: (event: KeyboardEvent) => void;
};

type Anchor = { node: Node; offset: number };
const tags = new Set(
  "p div span main aside nav address abbr cite q mark time ins del kbd samp var wbr section article header footer h1 h2 h3 h4 h5 h6 blockquote pre code em strong b i u s small sub sup br hr ul ol li dl dt dd ruby rt rp img a figure figcaption table thead tbody tfoot tr th td caption colgroup col".split(
    " ",
  ),
);
const attributes = new Set(
  "id class title lang dir alt colspan rowspan scope start value".split(" "),
);
// Book styles may decorate text, but must not move, hide or resize its layout.
const decoration =
  /^(font-weight|font-style|text-decoration(-line|-style|-color)?|text-align|text-indent|letter-spacing|word-spacing|border(-[a-z]+)*|margin(-[a-z]+)?|padding(-[a-z]+)?)$/;

type ResourceContext = { path: string; base: string; images: Set<string> };

function cleanDeclarations(
  style: CSSStyleDeclaration,
  resources: ResourceContext,
): string {
  const result: string[] = [];
  for (const property of Array.from(style)) {
    const value = style.getPropertyValue(property);
    if (property === "background-image") {
      const match = /^url\("([^"\\]+)"\)$/.exec(value);
      const url =
        match && resourceUrl(match[1], resources.path, resources.base);
      if (url) {
        resources.images.add(url);
        result.push(`background-image:url("${url.replace(/"/g, "%22")}")`);
      }
      continue;
    }
    if (
      decoration.test(property) &&
      !/url\s*\(|\\|[<>]|expression/i.test(value)
    ) {
      if (
        /^(margin|padding|text-indent|letter-spacing|word-spacing|border)/.test(
          property,
        )
      ) {
        if (/var\(|calc\(|[0-9]%/.test(value)) continue;
        const lengths = Array.from(
          value.matchAll(/(-?[\d.]+)(px|em|rem|vw|vh|cm|mm|pt|pc|in)/g),
        );
        if (
          lengths.some(
            ([, number, unit]) =>
              Number(number) < 0 ||
              Number(number) >
                (unit === "px" ? 40 : unit === "em" || unit === "rem" ? 2 : 0),
          )
        )
          continue;
      }
      result.push(`${property}:${value}`);
    }
  }
  return result.join(";");
}

function cleanCss(css: string, resources: ResourceContext): string {
  const sheet = new CSSStyleSheet();
  sheet.replaceSync(css);
  const rules = (list: CSSRuleList): string =>
    Array.from(list)
      .map((rule) => {
        if (
          rule instanceof CSSStyleRule &&
          !/[<>]|::(?:before|after)|\\/.test(rule.selectorText)
        ) {
          return `#reader-body :is(${rule.selectorText}){${cleanDeclarations(rule.style, resources)}}`;
        }
        if (rule instanceof CSSMediaRule && !/[<>\\]/.test(rule.conditionText))
          return `@media ${rule.conditionText}{${rules(rule.cssRules)}}`;
        return "";
      })
      .join("\n");
  return rules(sheet.cssRules);
}

async function prepareDocument(
  props: PageViewProps,
  signal: AbortSignal,
): Promise<string> {
  const source = new DOMParser().parseFromString(
    props.xhtml,
    "application/xhtml+xml",
  );
  const body = source.querySelector("body");
  if (!body || source.querySelector("parsererror"))
    throw new Error("章のXHTMLを解析できません。");
  const resources: ResourceContext = {
    path: props.chapterPath,
    base: props.resourceBase,
    images: new Set(),
  };
  const target = document.implementation.createHTMLDocument();
  const main = target.createElement("main");
  const copy = (node: Node, parent: Element) => {
    if (node.nodeType === Node.TEXT_NODE) {
      parent.append(target.createTextNode(node.textContent ?? ""));
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const original = node as Element;
    const tag = original.localName.toLowerCase();
    if (
      original.namespaceURI !== "http://www.w3.org/1999/xhtml" ||
      !tags.has(tag)
    )
      return;
    const element = target.createElement(tag);
    for (const attribute of Array.from(original.attributes)) {
      if (
        attributes.has(attribute.name) &&
        !(attribute.name === "id" && attribute.value === "reader-body")
      )
        element.setAttribute(attribute.name, attribute.value);
    }
    if (original.hasAttribute("style")) {
      const declaration = document.createElement("span").style;
      declaration.cssText = original.getAttribute("style")!;
      element.setAttribute("style", cleanDeclarations(declaration, resources));
    }
    if (tag === "img") {
      const src = resourceUrl(
        original.getAttribute("src") ?? "",
        props.chapterPath,
        props.resourceBase,
      );
      if (!src) return;
      element.setAttribute("src", src);
      element.setAttribute("loading", "eager");
    }
    parent.append(element);
    for (const child of Array.from(original.childNodes)) copy(child, element);
  };
  for (const child of Array.from(body.childNodes)) copy(child, main);
  const css: string[] = [];
  for (const element of Array.from(source.querySelectorAll("style,link"))) {
    if (element.localName === "style")
      css.push(cleanCss(element.textContent ?? "", resources));
    else if (
      (element.getAttribute("rel") ?? "").split(/\s+/).includes("stylesheet")
    ) {
      const url = resourceUrl(
        element.getAttribute("href") ?? "",
        props.chapterPath,
        props.resourceBase,
      );
      if (!url) continue;
      const response = await fetch(url, {
        signal,
        redirect: "error",
        credentials: "omit",
      });
      if (!response.ok)
        throw new Error(`書籍内CSSを取得できません (${response.status})。`);
      css.push(cleanCss(await response.text(), { ...resources, path: url }));
    }
  }
  await Promise.all(
    Array.from(resources.images).map(async (url) => {
      const response = await fetch(url, {
        signal,
        redirect: "error",
        credentials: "omit",
      });
      if (!response.ok)
        throw new Error(
          `書籍内CSSの画像を取得できません (${response.status})。`,
        );
      const bitmap = await createImageBitmap(await response.blob());
      bitmap.close();
    }),
  );
  // No scripts, fonts, networking, forms or child frames, even if a future sanitizer regresses.
  const csp = `default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src ${props.resourceBase}; font-src 'none'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`;
  const escapedCsp = csp.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
  return `<!doctype html><html lang="ja"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${escapedCsp}"><style>${css.join("\n")}</style><style>${readerStyles}</style></head><body><main id="reader-body">${main.innerHTML}</main></body></html>`;
}

function anchorRect(anchor: Anchor, doc: Document): DOMRect {
  if (anchor.node.nodeType !== Node.TEXT_NODE)
    return (anchor.node as Element).getBoundingClientRect();
  const range = doc.createRange();
  range.setStart(anchor.node, anchor.offset);
  range.setEnd(
    anchor.node,
    Math.min(anchor.offset + 1, anchor.node.textContent!.length),
  );
  return range.getBoundingClientRect();
}

function firstAnchor(
  main: HTMLElement,
  current: number,
  stride: number,
  inset: number,
): Anchor | null {
  const doc = main.ownerDocument;
  const walker = doc.createTreeWalker(
    main,
    NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT,
  );
  let node: Node | null;
  while ((node = walker.nextNode())) {
    if (node.nodeType === Node.ELEMENT_NODE) {
      if ((node as Element).localName !== "img") continue;
      const anchor = { node, offset: 0 };
      if (
        pageAt(
          anchorRect(anchor, doc).left + current * stride - inset,
          stride,
        ) === current
      )
        return anchor;
    } else {
      if (!node.textContent?.trim() || node.parentElement?.closest("rt,rp"))
        continue;
      let low = 0,
        high = node.textContent.length;
      while (low < high) {
        const mid = Math.floor((low + high) / 2);
        const position = pageAt(
          anchorRect({ node, offset: mid }, doc).left +
            current * stride -
            inset,
          stride,
        );
        if (position < current) low = mid + 1;
        else high = mid;
      }
      if (low < node.textContent.length) {
        const anchor = { node, offset: low };
        if (
          pageAt(
            anchorRect(anchor, doc).left + current * stride - inset,
            stride,
          ) === current
        )
          return anchor;
      }
    }
  }
  return null;
}

function applyDisplaySettings(doc: Document, props: PageViewProps) {
  const root = doc.documentElement;
  root.dataset.theme = props.theme ?? "auto";
  if (props.fontSize !== undefined) {
    const size = [15, 17, 19, 21, 24, 27].includes(props.fontSize)
      ? props.fontSize
      : 19;
    root.style.setProperty("--reader-font-size", `${size}px`);
  } else root.style.removeProperty("--reader-font-size");
  if (props.fontFamily)
    root.style.setProperty(
      "--reader-font-family",
      fontStacks[props.fontFamily],
    );
  else root.style.removeProperty("--reader-font-family");
}

export function PageView(props: PageViewProps) {
  const frame = useRef<HTMLIFrameElement>(null);
  const latest = useRef(props);
  useLayoutEffect(() => {
    latest.current = props;
  });
  const generation = useRef(0);
  const controls = useRef<{
    move: (page: number) => void;
    layout: () => void;
  } | null>(null);

  useLayoutEffect(() => {
    const iframe = frame.current!;
    const abort = new AbortController();
    const version = String(++generation.current);
    controls.current = null;
    iframe.style.visibility = "hidden";
    let observer: ResizeObserver | undefined;
    let removeListeners = () => {};
    let scheduled = 0;
    const fail = (error: unknown) => {
      if (!abort.signal.aborted)
        latest.current.onError(
          error instanceof Error ? error : new Error(String(error)),
        );
    };
    iframe.onload = async () => {
      if (
        abort.signal.aborted ||
        iframe.contentDocument?.documentElement.dataset.generation !== version
      )
        return;
      const doc = iframe.contentDocument;
      const main = doc.getElementById("reader-body")!;
      applyDisplaySettings(doc, latest.current);
      const click = (event: Event) => {
        if ((event.target as Element).closest("a")) event.preventDefault();
      };
      const keydown = (event: KeyboardEvent) =>
        latest.current.onKeyDown?.(event);
      doc.addEventListener("click", click);
      doc.addEventListener("keydown", keydown);
      removeListeners = () => {
        doc.removeEventListener("click", click);
        doc.removeEventListener("keydown", keydown);
      };
      try {
        await Promise.all(
          Array.from(doc.images).map(async (image) => {
            await image.decode();
          }),
        );
        await doc.fonts.ready;
        if (abort.signal.aborted) return;
        let current = 0,
          count = 1,
          stride = iframe.clientWidth;
        let anchor: Anchor | null = null;
        const inset = () => parseFloat(getComputedStyle(doc.body).paddingLeft);
        const move = (requested: number) => {
          current = clampPage(requested, count);
          main.style.transform = `translateX(${-current * stride}px)`;
          anchor = firstAnchor(main, current, stride, inset());
          latest.current.onPagination({ page: current, count });
        };
        const layout = () => {
          applyDisplaySettings(doc, latest.current);
          main.style.transform = "none";
          stride = iframe.clientWidth;
          count = pageCount(main.scrollWidth + inset() * 2, stride);
          const desired = anchor
            ? pageAt(anchorRect(anchor, doc).left - inset(), stride)
            : latest.current.page;
          move(desired);
        };
        const schedule = () => {
          cancelAnimationFrame(scheduled);
          scheduled = requestAnimationFrame(() => {
            if (!abort.signal.aborted) layout();
          });
        };
        iframe.style.visibility = "visible";
        controls.current = { move, layout };
        layout();
        observer = new ResizeObserver(schedule);
        observer.observe(iframe);
        doc.fonts.addEventListener("loadingdone", schedule);
        const priorCleanup = removeListeners;
        removeListeners = () => {
          priorCleanup();
          doc.fonts.removeEventListener("loadingdone", schedule);
        };
      } catch (error) {
        fail(
          new Error(
            `本文の画像またはフォントを準備できません: ${String(error)}`,
          ),
        );
      }
    };
    void prepareDocument(props, abort.signal)
      .then((html) => {
        if (!abort.signal.aborted)
          iframe.srcdoc = html.replace(
            '<html lang="ja">',
            `<html lang="ja" data-generation="${version}">`,
          );
      })
      .catch(fail);
    return () => {
      abort.abort();
      observer?.disconnect();
      cancelAnimationFrame(scheduled);
      removeListeners();
      iframe.onload = null;
      controls.current = null;
    };
  }, [props.xhtml, props.chapterPath, props.resourceBase]);

  useLayoutEffect(() => {
    controls.current?.move(props.page);
  }, [props.page]);
  useLayoutEffect(() => {
    controls.current?.layout();
  }, [props.theme, props.fontSize, props.fontFamily]);
  return (
    <iframe
      ref={frame}
      className="page-view"
      style={{ display: "block", border: 0, width: "100%", height: "100%" }}
      title="章の本文"
      sandbox="allow-same-origin"
    />
  );
}
