import type { PersonListItem } from "./api";
import { foldWord } from "./fold";
import { foldName } from "./components/Sidebar";

/** Folded names (and other names) of the records → record id. */
export interface NameIndex {
  ids: Map<string, number>;
  maxWords: number;
}

/**
 * Only names of two words or more are linked: single words ("Rupa",
 * "Yamuna", "Agata") are too often something else.
 */
export function buildNameIndex(persons: PersonListItem[]): NameIndex {
  const ids = new Map<string, number>();
  let maxWords = 0;
  // when two records share a name, the one with more texts wins
  for (const p of [...persons].sort((a, b) => b.docs - a.docs)) {
    for (const name of [p.name, ...p.aka.split(",")]) {
      const key = foldName(name);
      const n = key.split(" ").length;
      if (!key || n < 2 || ids.has(key)) continue;
      ids.set(key, p.id);
      maxWords = Math.max(maxWords, n);
    }
  }
  return { ids, maxWords: Math.min(maxWords, 7) };
}

const WORD = /[\p{L}\p{M}\p{N}'’]+/gu;
const GAP = /^[ \t -]+$/;

/**
 * Turns the names of other records written in the text into links
 * (<a data-person="id">), the first time each one appears.
 */
export function linkNames(root: HTMLElement, index: NameIndex | null, exclude: number | null): void {
  if (!index || !index.ids.size) return;
  const seen = new Set<number>(exclude ? [exclude] : []);
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => (n.parentElement?.closest("a, pre, code, .md-code") ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
  });
  const nodes: Text[] = [];
  while (walker.nextNode()) nodes.push(walker.currentNode as Text);

  for (const node of nodes) {
    const text = node.data;
    const words = Array.from(text.matchAll(WORD), (m) => ({ s: m.index!, e: m.index! + m[0].length, f: foldWord(m[0]) }));
    const found: { s: number; e: number; id: number }[] = [];
    for (let i = 0; i < words.length; i++) {
      for (let n = Math.min(index.maxWords, words.length - i); n >= 2; n--) {
        let ok = true;
        for (let k = i; k < i + n - 1 && ok; k++) ok = GAP.test(text.slice(words[k].e, words[k + 1].s));
        if (!ok) continue;
        const id = index.ids.get(words.slice(i, i + n).map((w) => w.f).join(" "));
        if (id === undefined) continue;
        if (!seen.has(id)) {
          seen.add(id);
          found.push({ s: words[i].s, e: words[i + n - 1].e, id });
        }
        i += n - 1;
        break;
      }
    }
    if (!found.length) continue;
    const frag = document.createDocumentFragment();
    let at = 0;
    for (const f of found) {
      frag.append(text.slice(at, f.s));
      const a = document.createElement("a");
      a.className = "person-link";
      a.dataset.person = String(f.id);
      a.textContent = text.slice(f.s, f.e);
      frag.append(a);
      at = f.e;
    }
    frag.append(text.slice(at));
    node.replaceWith(frag);
  }
}
