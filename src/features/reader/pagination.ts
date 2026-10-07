// CSS column measurements can differ by a subpixel at the final edge.
export function pageCount(extent: number, width: number): number {
  return width > 0 ? Math.max(1, Math.ceil((extent - 0.5) / width)) : 1;
}
export function clampPage(page: number, count: number): number {
  return Math.max(0, Math.min(Math.floor(page), Math.max(0, count - 1)));
}
export function pageAt(x: number, width: number): number {
  return width > 0 ? Math.max(0, Math.floor((x + 0.5) / width)) : 0;
}

export function resourceUrl(
  reference: string,
  path: string,
  base: string,
): string | null {
  const ref = reference.trim();
  if (
    !ref ||
    /^[\/#]/.test(ref) ||
    /[\\\u0000-\u0020]/.test(ref) ||
    /^[a-z][\w+.-]*:/i.test(ref)
  )
    return null;
  try {
    const decoded = decodeURIComponent(ref.split(/[?#]/)[0]);
    if (/%(?:2f|5c|00)/i.test(ref) || decoded.includes("\\")) return null;
    const root = new URL(base);
    const result = new URL(ref, new URL(path, root));
    if (
      result.origin !== root.origin ||
      result.protocol !== root.protocol ||
      result.host !== root.host ||
      !result.pathname.startsWith(root.pathname)
    )
      return null;
    return result.href;
  } catch {
    return null;
  }
}
