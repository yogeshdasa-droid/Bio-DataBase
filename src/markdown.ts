import { marked } from "marked";
import DOMPurify from "dompurify";

const IMAGE_EXT = /\.(png|jpe?g|gif|bmp|webp|svg|avif)$/i;

const esc = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/**
 * Obsidian-flavoured markdown → sanitized HTML.
 * Images and wikilinks become data-* placeholders the reader resolves later.
 */
export function renderMarkdown(src: string): string {
  let s = src
    // VedaBase exports indent lines with spaces: don't turn them into code blocks.
    .replace(/^[ \t]{4,}(?=\S)/gm, "")
    // Obsidian block ids "text ^0f7dd6"
    .replace(/[ \t]\^[A-Za-z0-9-]{3,}[ \t]*$/gm, "")
    // Callouts "> [!example] TITLE" → bold title inside the quote.
    .replace(/^(>\s*)\[![\w-]+\][+-]?\s*(.*)$/gm, (_m, p: string, title: string) => (title ? `${p}**${title}**` : p))
    // Embeds ![[file|300]]
    .replace(/!\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]*))?\]\]/g, (_m, name: string, opt?: string) => {
      const n = name.trim();
      if (IMAGE_EXT.test(n)) {
        const width = opt && /^\d+/.test(opt) ? ` width="${parseInt(opt, 10)}"` : "";
        return `<img data-attach="${esc(n)}" alt="${esc(n)}"${width}>`;
      }
      return `<a data-wikilink="${esc(n)}" class="embed">📎 ${esc(n)}</a>`;
    })
    // Wikilinks [[target|alias]]
    .replace(/\[\[([^\]|]+)(?:\|([^\]]*))?\]\]/g, (_m, target: string, alias?: string) => {
      const label = (alias || target.replace(/#.*/, "")).trim();
      return `<a data-wikilink="${esc(target.trim())}">${esc(label)}</a>`;
    })
    // #tags
    .replace(/(^|[\s(])#(\p{L}[\p{L}\p{N}_/-]*)/gmu, (_m, pre: string, tag: string) => `${pre}<span class="tag">#${tag}</span>`);

  const html = marked.parse(s, { gfm: true, breaks: true, async: false }) as string;
  return DOMPurify.sanitize(html, { ADD_ATTR: ["data-attach", "data-wikilink", "target"] });
}

/** Inline marks of one escaped source line: the markdown signs stay visible. */
function sourceInline(s: string): string {
  return (
    s
      // `code`
      .replace(/(`+)([^`]+?)\1/g, '<span class="md-code">$1$2$1</span>')
      // [[target|alias]] → clickable, brackets kept
      .replace(/(!?)\[\[([^\]|]+)((?:\|[^\]]*)?)\]\]/g, (_m, bang: string, target: string, alias: string) =>
        `<a data-wikilink="${target.replace(/#.*/, "").trim()}">${bang}[[${target}${alias}]]</a>`,
      )
      // [text](https://…)
      .replace(/\[([^\]]+)\]\(((?:https?:|mailto:)[^)\s]+)\)/g, '<span class="md-punct">[</span><a href="$2">$1</a><span class="md-punct">]($2)</span>')
      // **bold** / __bold__
      .replace(/(\*\*|__)(?=\S)(.+?)(?<=\S)\1/g, '<strong class="md-b">$1$2$1</strong>')
      // *italic* / _italic_
      .replace(/(^|[^\w*])([*_])(?=\S)([^*_]+?)(?<=\S)\2(?![\w*])/g, '$1<em class="md-i">$2$3$2</em>')
      // #tags
      .replace(/(^|[\s(])#(\p{L}[\p{L}\p{N}_/-]*)/gu, '$1<span class="tag">#$2</span>')
  );
}

/**
 * Markdown shown as written, like a code editor: monospace, the signs
 * (##, **, `, -) visible and coloured. Each line is one block.
 */
export function renderSource(src: string): string {
  const lines = src
    .replace(/[ \t]\^[A-Za-z0-9-]{3,}[ \t]*$/gm, "")
    .replace(/\r\n?/g, "\n")
    // runs of empty lines (also "> " alone inside callouts) read as one
    .replace(/\n(?:[ \t>]*\n){2,}/g, "\n\n")
    .replace(/^\s+|\s+$/g, "")
    .split("\n");
  let fence = false;
  const out = lines.map((raw) => {
    const line = esc(raw);
    if (/^\s*(```|~~~)/.test(raw)) {
      fence = !fence;
      return `<div class="ln fence">${line}</div>`;
    }
    if (fence) return `<div class="ln code">${line || " "}</div>`;
    if (!raw.trim()) return '<div class="ln blank"> </div>';
    let m = /^(#{1,6})(\s.*)$/.exec(line);
    if (m) return `<div class="ln h h${m[1].length}">${m[1]}${sourceInline(m[2])}</div>`;
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(raw)) return `<div class="ln rule">${line}</div>`;
    m = /^(\s*)([-*+]|\d+[.)])(\s+)(.*)$/.exec(line);
    if (m) {
      const w = m[1].length + m[2].length + m[3].length;
      return `<div class="ln li" style="padding-left:${w}ch;text-indent:-${w}ch">${m[1]}<span class="md-bullet">${m[2]}</span>${m[3]}${sourceInline(m[4])}</div>`;
    }
    m = /^(\s*(?:&gt;\s*)+)(.*)$/.exec(line);
    if (m) {
      // Obsidian callout "> [!example] TITLE": the tag dimmed, the title as a heading
      const c = /^(\[![\w-]+\][+-]?)(.*)$/.exec(m[2]);
      if (c) return `<div class="ln quote h"><span class="md-bullet">${m[1]}</span><span class="md-punct">${c[1]}</span>${sourceInline(c[2])}</div>`;
      return `<div class="ln quote"><span class="md-bullet">${m[1]}</span>${sourceInline(m[2])}</div>`;
    }
    return `<div class="ln">${sourceInline(line)}</div>`;
  });
  return DOMPurify.sanitize(out.join(""), { ADD_ATTR: ["data-wikilink"] });
}

/** Resolves <img data-attach> placeholders to local files. */
export async function resolveImages(
  root: HTMLElement,
  resolve: (names: string[]) => Promise<(string | null)[]>,
  toUrl: (path: string) => string,
): Promise<void> {
  const imgs = Array.from(root.querySelectorAll<HTMLImageElement>("img[data-attach]:not([src])"));
  if (!imgs.length) return;
  const paths = await resolve(imgs.map((i) => i.dataset.attach!));
  imgs.forEach((img, i) => {
    const p = paths[i];
    if (p) {
      img.src = toUrl(p);
      img.loading = "lazy";
    } else img.classList.add("missing");
  });
}
