import { memo, useEffect, useMemo, useState } from "react";
import { BookMarked, ChevronRight, FileText, Folder, FolderOpen, History, Library as LibIcon, Search, Star, Trash2, Upload, UserPlus, Users, X } from "lucide-react";
import { api, Bookmark, DocListItem, hueOf, PersonListItem } from "../api";
import { foldWord } from "../fold";
import { Lang, T } from "../i18n";

export type SideTab = "persons" | "library" | "bookmarks" | "history";

export const foldName = (s: string) =>
  s
    .split(/[\s,.;:()\-–—]+/)
    .filter(Boolean)
    .map(foldWord)
    .join(" ");

// ---------------------------------------------------------------- library tree

interface Node {
  key: string;
  name: string;
  dir: string;
  folders: Node[];
  docs: DocListItem[];
  count: number;
}

function buildTree(docs: DocListItem[]): Node[] {
  const root: Node = { key: "", name: "", dir: "", folders: [], docs: [], count: 0 };
  const index = new Map<string, Node>([["", root]]);
  const get = (dir: string): Node => {
    let n = index.get(dir);
    if (n) return n;
    const cut = dir.lastIndexOf("/");
    const parent = get(cut < 0 ? "" : dir.slice(0, cut));
    n = { key: dir, name: dir.slice(cut + 1), dir, folders: [], docs: [], count: 0 };
    parent.folders.push(n);
    index.set(dir, n);
    return n;
  };
  for (const d of docs) get(d.dir).docs.push(d);
  const finish = (n: Node): number => {
    n.folders.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }));
    n.count = n.docs.length + n.folders.reduce((s, c) => s + finish(c), 0);
    return n.count;
  };
  finish(root);
  return root.folders;
}

interface TreeProps {
  nodes: Node[];
  docs?: DocListItem[];
  depth: number;
  open: Set<string>;
  toggle: (k: string) => void;
  activeDir: string | null;
  activeDocId: number | null;
  onFolder: (dir: string) => void;
  onDoc: (id: number) => void;
  lang: Lang;
}

const Tree = memo(function Tree(p: TreeProps) {
  const { nodes, docs, depth, open, toggle, activeDir, activeDocId, onFolder, onDoc, lang } = p;
  return (
    <>
      {nodes.map((n) => {
        const isOpen = open.has(n.key);
        const hue = depth === 0 ? hueOf(n.name) : null;
        return (
          <div key={n.key}>
            <div
              data-ctx="folder"
              data-dir={n.dir}
              className={`tree-row ${depth === 0 ? "lib" : "folder"} ${activeDir === n.dir ? "active" : ""}`}
              style={{ paddingLeft: 6 + depth * 14, ...(hue !== null ? ({ "--h": hue } as React.CSSProperties) : {}) }}
              onClick={() => {
                toggle(n.key);
                onFolder(n.dir);
              }}
            >
              <ChevronRight size={14} className={`caret ${isOpen ? "open" : ""}`} />
              {depth === 0 ? <LibIcon size={16} /> : isOpen ? <FolderOpen size={16} /> : <Folder size={16} />}
              <span className="name">{n.name}</span>
              <span className="count">{n.count.toLocaleString(lang)}</span>
            </div>
            <div className={`tree-children ${isOpen ? "open" : ""}`}>
              {isOpen && <Tree {...p} nodes={n.folders} docs={n.docs} depth={depth + 1} />}
            </div>
          </div>
        );
      })}
      {docs?.map((d) => (
        <div
          key={d.id}
          data-ctx="doc"
          data-id={d.id}
          className={`tree-row doc ${activeDocId === d.id ? "active" : ""}`}
          style={{ paddingLeft: 6 + depth * 14 + 14 }}
          onClick={() => onDoc(d.id)}
          title={d.title}
        >
          <FileText size={14} />
          <span className="name">{d.title}</span>
        </div>
      ))}
    </>
  );
});

// ---------------------------------------------------------------- persons

function PersonList({
  t,
  lang,
  persons,
  activeId,
  onPerson,
}: {
  t: T;
  lang: Lang;
  persons: PersonListItem[];
  activeId: number | null;
  onPerson: (id: number) => void;
}) {
  const [filter, setFilter] = useState("");
  const [fieldHits, setFieldHits] = useState<number[]>([]);
  const folded = useMemo(() => persons.map((p) => ` ${foldName(`${p.name} ${p.aka}`)}`), [persons]);

  // Names are filtered here; the records' other fields by the database.
  useEffect(() => {
    if (filter.trim().length < 3) return setFieldHits([]);
    const h = window.setTimeout(() => api.searchPersons(filter).then(setFieldHits).catch(() => setFieldHits([])), 200);
    return () => window.clearTimeout(h);
  }, [filter]);

  const shown = useMemo(() => {
    const words = foldName(filter).split(" ").filter(Boolean);
    if (!words.length) return persons;
    const extra = new Set(fieldHits);
    return persons.filter((p, i) => words.every((w) => folded[i].includes(` ${w}`)) || extra.has(p.id));
  }, [filter, persons, folded, fieldHits]);

  let letter = "";
  return (
    <div className="fade-in">
      <div className="person-filter">
        <Search size={14} />
        <input value={filter} placeholder={t.filterPersons} title={t.filterPersonsHint} spellCheck={false} onChange={(e) => setFilter(e.target.value)} />
        {filter && (
          <button className="icon-btn" onClick={() => setFilter("")}>
            <X size={13} />
          </button>
        )}
      </div>
      {shown.length === 0 && <div className="side-empty">{t.noPersons}</div>}
      {shown.map((p) => {
        const first = foldWord(p.name.trim()[0] ?? "#").toUpperCase() || "#";
        const header = first !== letter && !filter ? (letter = first) : null;
        return (
          <div key={p.id}>
            {header && <div className="letter">{header}</div>}
            <div
              data-ctx="person"
              data-id={p.id}
              className={`tree-row person ${activeId === p.id ? "active" : ""}`}
              onClick={() => onPerson(p.id)}
              title={p.aka ? `${p.name} · ${p.aka}` : p.name}
            >
              <span className="name">{p.name}</span>
              {p.docs > 1 && <span className="count">{p.docs.toLocaleString(lang)}</span>}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------- sidebar

interface Props {
  t: T;
  lang: Lang;
  tab: SideTab;
  onTab: (t: SideTab) => void;
  persons: PersonListItem[];
  activePersonId: number | null;
  onPerson: (id: number) => void;
  onNewPerson: () => void;
  docs: DocListItem[];
  activeDir: string | null;
  activeDocId: number | null;
  onFolder: (dir: string) => void;
  onDoc: (id: number, ordinal?: number) => void;
  onImport: () => void;
  bookmarks: Bookmark[];
  onBookmarkNote: (id: number, note: string) => void;
  onBookmarkDelete: (id: number) => void;
  history: string[];
  onHistory: (q: string) => void;
  onClearHistory: () => void;
}

export function Sidebar(p: Props) {
  const { t } = p;
  const tree = useMemo(() => buildTree(p.docs), [p.docs]);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [editing, setEditing] = useState<number | null>(null);
  const toggle = (k: string) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });

  const tabs: [SideTab, typeof LibIcon, string][] = [
    ["persons", Users, t.persons],
    ["library", LibIcon, t.library],
    ["bookmarks", Star, t.bookmarks],
    ["history", History, t.history],
  ];

  return (
    <aside className="sidebar">
      <nav className="tabs four">
        {tabs.map(([k, I, label]) => (
          <button key={k} className={p.tab === k ? "active" : ""} onClick={() => p.onTab(k)} title={label}>
            <I size={15} />
            <span>{label}</span>
          </button>
        ))}
        <span className="tab-ink" style={{ transform: `translateX(${tabs.findIndex((x) => x[0] === p.tab) * 100}%)` }} />
      </nav>

      <div className="side-scroll" key={p.tab}>
        {p.tab === "persons" && <PersonList t={t} lang={p.lang} persons={p.persons} activeId={p.activePersonId} onPerson={p.onPerson} />}

        {p.tab === "library" && (
          <div className="fade-in">
            <Tree
              nodes={tree}
              depth={0}
              open={open}
              toggle={toggle}
              activeDir={p.activeDir}
              activeDocId={p.activeDocId}
              onFolder={p.onFolder}
              onDoc={(id) => p.onDoc(id)}
              lang={p.lang}
            />
          </div>
        )}

        {p.tab === "bookmarks" &&
          (p.bookmarks.length === 0 ? (
            <div className="side-empty">
              <BookMarked size={28} />
              <p>{t.noBookmarks}</p>
            </div>
          ) : (
            <div className="fade-in">
              {p.bookmarks.map((b) => (
                <div key={b.id} data-ctx="bookmark" data-id={b.id} className={`bookmark ${b.docId ? "" : "stale"}`}>
                  <div className="bm-head" onClick={() => b.docId && p.onDoc(b.docId, b.ordinal)}>
                    <div className="bm-title">{b.title}</div>
                    {b.heading && b.heading.toLowerCase() !== b.title.toLowerCase() && <div className="bm-sub">§ {b.heading}</div>}
                  </div>
                  {editing === b.id ? (
                    <textarea
                      autoFocus
                      defaultValue={b.note}
                      placeholder={t.notePlaceholder}
                      onBlur={(e) => {
                        setEditing(null);
                        if (e.target.value !== b.note) p.onBookmarkNote(b.id, e.target.value);
                      }}
                    />
                  ) : (
                    <div className={`bm-note ${b.note ? "" : "placeholder"}`} onClick={() => setEditing(b.id)}>
                      {b.note || t.notePlaceholder}
                    </div>
                  )}
                  <div className="bm-date">{b.created}</div>
                  <button className="icon-btn bm-del" title={t.delete} onClick={() => p.onBookmarkDelete(b.id)}>
                    <Trash2 size={14} />
                  </button>
                </div>
              ))}
            </div>
          ))}

        {p.tab === "history" &&
          (p.history.length === 0 ? (
            <div className="side-empty">
              <History size={28} />
              <p>{t.noHistory}</p>
            </div>
          ) : (
            <div className="fade-in">
              {p.history.map((q) => (
                <div key={q} className="tree-row" onClick={() => p.onHistory(q)}>
                  <Search size={14} />
                  <span className="name">{q}</span>
                </div>
              ))}
              <button className="link-btn" onClick={p.onClearHistory}>
                {t.clearHistory}
              </button>
            </div>
          ))}
      </div>

      {(p.tab === "persons" || p.tab === "library") && (
        <div className="side-actions">
          <button onClick={p.onNewPerson}>
            <UserPlus size={15} /> {t.newPerson}
          </button>
          <button onClick={p.onImport} title={t.importHint}>
            <Upload size={15} /> {t.importFiles}
          </button>
        </div>
      )}
    </aside>
  );
}
