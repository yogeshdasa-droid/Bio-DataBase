import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  Download,
  ExternalLink,
  FileDown,
  MoreHorizontal,
  Pencil,
  Star,
  TextSearch,
  Trash2,
  UserRound,
} from "lucide-react";
import { api, DocInfo, fileUrl, SectionText, TocEntry } from "../api";
import { highlightDom, Term } from "../fold";
import { formatDate, Lang, T } from "../i18n";
import { renderMarkdown } from "../markdown";
import { CollectionChip } from "./ui";

export interface ReaderTarget {
  docId: number;
  ordinal: number;
  terms: Term[];
  /** changes on every open so re-opening the same place scrolls again */
  nonce: number;
}

interface Props {
  target: ReaderTarget;
  t: T;
  lang: Lang;
  fontSize: number;
  serif: boolean;
  onFontSize: (n: number) => void;
  onOpenDoc: (docId: number, ordinal: number) => void;
  onPerson: (id: number) => void;
  onLoaded: (doc: DocInfo) => void;
  onSearchInDoc: (doc: DocInfo) => void;
  onBookmark: (docId: number, ordinal: number) => void;
  onEdit: (doc: DocInfo, section: SectionText) => void;
  onTrash: (doc: DocInfo) => void;
  onSaveOriginal: (doc: DocInfo) => void;
  onExport: (doc: DocInfo) => void;
  onToast: (msg: string) => void;
}

const WINDOW_BEFORE = 1;
const WINDOW_AFTER = 2;
const LOAD_STEP = 3;

function SectionView({ s, terms, multi }: { s: SectionText; terms: Term[]; multi: boolean }) {
  const ref = useRef<HTMLElement>(null);
  const html = useMemo(() => renderMarkdown(s.text), [s.text]);
  useLayoutEffect(() => {
    const el = ref.current!;
    el.innerHTML = html;
    highlightDom(el, terms);
    // Obsidian image embeds point at files that are not in the database.
    el.querySelectorAll("img[data-attach]").forEach((i) => i.classList.add("missing"));
  }, [html, terms]);
  return <section ref={ref} className={multi ? "sec multi" : "sec"} data-ord={s.ordinal} />;
}

export function Reader(p: Props) {
  const { target, t, lang, fontSize, serif, onFontSize, onOpenDoc, onLoaded, onToast } = p;
  const [doc, setDoc] = useState<DocInfo | null>(null);
  const [toc, setToc] = useState<TocEntry[]>([]);
  const [sections, setSections] = useState<SectionText[]>([]);
  const [markIdx, setMarkIdx] = useState<{ cur: number; total: number }>({ cur: 0, total: 0 });
  const [currentOrd, setCurrentOrd] = useState(0);
  const [progress, setProgress] = useState(0);
  const [menu, setMenu] = useState(false);
  const [view, setView] = useState<"text" | "original">("text");
  const scrollRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const pendingScroll = useRef<{ ordinal: number } | null>(null);
  const prependAnchor = useRef<number | null>(null);
  const loading = useRef(false);
  const terms = target.terms;

  const lo = sections[0]?.ordinal ?? 0;
  const hi = sections[sections.length - 1]?.ordinal ?? -1;
  const count = doc?.sections ?? 0;

  const jump = useCallback(async (docId: number, ordinal: number) => {
    const from = Math.max(0, ordinal - WINDOW_BEFORE);
    const secs = await api.getSections(docId, from, ordinal + WINDOW_AFTER);
    pendingScroll.current = { ordinal };
    setSections(secs);
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const r = await api.getDoc(target.docId);
      if (cancelled) return;
      setDoc(r.doc);
      setToc(r.toc);
      onLoaded(r.doc);
      // a PDF without text (scanned) opens on its original
      if (r.doc.size < 50 && r.doc.original?.toLowerCase().endsWith(".pdf")) setView("original");
      await jump(target.docId, Math.min(target.ordinal, Math.max(0, r.doc.sections - 1)));
      if (target.terms.length) setView("text");
    })().catch((e) => onToast(String(e)));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target]);

  // One entry per match (a phrase split over several <mark>s counts once).
  const allMarks = () => Array.from(bodyRef.current?.querySelectorAll<HTMLElement>("mark.hl.start") ?? []);

  const focusMark = useCallback((i: number, smooth = true) => {
    const marks = allMarks();
    if (!marks.length) return;
    const idx = ((i % marks.length) + marks.length) % marks.length;
    bodyRef.current?.querySelectorAll("mark.hl.current").forEach((m) => m.classList.remove("current"));
    bodyRef.current?.querySelectorAll(`mark.hl[data-m="${marks[idx].dataset.m}"]`).forEach((m) => m.classList.add("current"));
    marks[idx].scrollIntoView({ block: "center", behavior: smooth ? "smooth" : "auto" });
    setMarkIdx({ cur: idx + 1, total: marks.length });
  }, []);

  useLayoutEffect(() => {
    const scroller = scrollRef.current;
    const body = bodyRef.current;
    if (!scroller || !body) return;
    if (prependAnchor.current !== null) {
      scroller.scrollTop += scroller.scrollHeight - prependAnchor.current;
      prependAnchor.current = null;
    }
    const marks = allMarks();
    setMarkIdx((m) => ({ cur: Math.min(m.cur, marks.length), total: marks.length }));
    const ps = pendingScroll.current;
    if (!ps) return;
    pendingScroll.current = null;
    const secEl = body.querySelector<HTMLElement>(`section[data-ord="${ps.ordinal}"]`);
    const firstMark = secEl?.querySelector<HTMLElement>("mark.hl.start");
    if (firstMark) focusMark(marks.indexOf(firstMark), false);
    else if (secEl) {
      scroller.scrollTop = secEl.offsetTop - 16;
      setMarkIdx({ cur: 0, total: marks.length });
    } else scroller.scrollTop = 0;
    setCurrentOrd(ps.ordinal);
    body.classList.remove("enter");
    void body.offsetWidth;
    body.classList.add("enter");
  }, [sections, focusMark, view]);

  const loadMore = useCallback(
    async (dir: "up" | "down") => {
      if (!doc || loading.current) return;
      if (dir === "down" && hi >= count - 1) return;
      if (dir === "up" && lo <= 0) return;
      loading.current = true;
      try {
        if (dir === "down") {
          const more = await api.getSections(doc.id, hi + 1, hi + LOAD_STEP);
          setSections((s) => [...s, ...more]);
        } else {
          const more = await api.getSections(doc.id, Math.max(0, lo - LOAD_STEP), lo - 1);
          prependAnchor.current = scrollRef.current?.scrollHeight ?? 0;
          setSections((s) => [...more, ...s]);
        }
      } finally {
        loading.current = false;
      }
    },
    [doc, lo, hi, count],
  );

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    if (el.scrollTop + el.clientHeight > el.scrollHeight - 600) loadMore("down");
    else if (el.scrollTop < 300) loadMore("up");
    const secs = bodyRef.current?.querySelectorAll<HTMLElement>("section[data-ord]") ?? [];
    let ord = currentOrd;
    for (const s of Array.from(secs)) {
      if (s.offsetTop - el.scrollTop <= 90) ord = Number(s.dataset.ord);
      else break;
    }
    if (ord !== currentOrd) setCurrentOrd(ord);
    // Reading progress: position within the whole document.
    const within = el.scrollHeight > el.clientHeight ? el.scrollTop / (el.scrollHeight - el.clientHeight) : 1;
    setProgress(count > 1 ? (lo + within * (hi - lo + 1)) / count : within);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F3") {
        e.preventDefault();
        focusMark(markIdx.cur - 1 + (e.shiftKey ? -1 : 1));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [focusMark, markIdx.cur]);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(false);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [menu]);

  const onBodyClick = async (e: React.MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const wl = a.getAttribute("data-wikilink");
    try {
      if (wl) {
        const r = await api.resolveLink(wl);
        if (r.kind === "doc") onOpenDoc(r.id, 0);
        else if (r.kind === "person") p.onPerson(r.id);
        else onToast(`? ${wl}`);
      } else {
        const href = a.getAttribute("href");
        if (href && /^(https?:|mailto:)/.test(href)) await api.openExternal(href);
      }
    } catch (err) {
      onToast(String(err));
    }
  };

  if (!doc) return <div className="reader" />;

  const multi = count > 1;
  const isPdf = !!doc.original?.toLowerCase().endsWith(".pdf");
  const editSection = () => {
    const s = sections.find((x) => x.ordinal === currentOrd) ?? sections[0];
    if (s) p.onEdit(doc, s);
  };
  const guard = (f: () => Promise<unknown>) => () => {
    setMenu(false);
    f().catch((e) => onToast(String(e)));
  };

  return (
    <div className="reader">
      {view === "text" && <div className="read-progress" style={{ transform: `scaleX(${progress})` }} />}
      <header className="reader-head">
        <div className="reader-title">
          <div className="meta">
            <CollectionChip name={doc.collection} />
            {doc.person && doc.personId && (
              <button className="person-chip" onClick={() => p.onPerson(doc.personId!)} title={t.openPerson}>
                <UserRound size={12} /> {doc.person}
              </button>
            )}
            {doc.date && <span>{formatDate(doc.date, lang)}</span>}
            <span className="path" title={doc.source ? `${t.importedFrom}: ${doc.source}` : undefined}>
              {doc.dir.split("/").slice(1).join(" › ")}
            </span>
          </div>
          <h1>{doc.title}</h1>
          {doc.tags.length > 0 && (
            <div className="tags">
              {doc.tags.slice(0, 10).map((tg) => (
                <span key={tg} className="tag">
                  #{tg}
                </span>
              ))}
            </div>
          )}
        </div>
        <div className="reader-tools">
          {isPdf && (
            <div className="seg small" data-on={view === "original" ? 1 : 0}>
              <span className="seg-ink" />
              <button className={view === "text" ? "on" : ""} onClick={() => setView("text")}>
                {t.viewText}
              </button>
              <button className={view === "original" ? "on" : ""} onClick={() => setView("original")}>
                {t.viewOriginal}
              </button>
            </div>
          )}
          {view === "text" && (
            <div className="tool-group">
              <button className="icon-btn" title={t.fontSmaller} onClick={() => onFontSize(Math.max(13, fontSize - 1))}>
                <span className="aa small">A</span>
              </button>
              <button className="icon-btn" title={t.fontLarger} onClick={() => onFontSize(Math.min(28, fontSize + 1))}>
                <span className="aa">A</span>
              </button>
            </div>
          )}
          <button className="icon-btn" title={t.addBookmark} onClick={() => p.onBookmark(doc.id, currentOrd)}>
            <Star size={18} />
          </button>
          <button className="icon-btn" title={t.searchInDoc} onClick={() => p.onSearchInDoc(doc)}>
            <TextSearch size={18} />
          </button>
          <button className="icon-btn" title={t.edit} onClick={editSection}>
            <Pencil size={17} />
          </button>
          <div className="menu-wrap">
            <button
              className={`icon-btn ${menu ? "on" : ""}`}
              title={t.more}
              onClick={(e) => {
                e.stopPropagation();
                setMenu((m) => !m);
              }}
            >
              <MoreHorizontal size={18} />
            </button>
            {menu && (
              <div className="menu" onClick={(e) => e.stopPropagation()}>
                {doc.original && (
                  <>
                    <button onClick={guard(() => api.openOriginal(doc.id))}>
                      <ExternalLink size={15} /> {t.openOriginal}
                    </button>
                    <button onClick={() => (setMenu(false), p.onSaveOriginal(doc))}>
                      <Download size={15} /> {t.saveOriginal}
                    </button>
                  </>
                )}
                <button onClick={() => (setMenu(false), p.onExport(doc))}>
                  <FileDown size={15} /> {t.exportText}
                </button>
                <button onClick={() => (setMenu(false), editSection())}>
                  <Pencil size={15} /> {t.editTitle}
                </button>
                <hr />
                <button className="danger" onClick={() => (setMenu(false), p.onTrash(doc))}>
                  <Trash2 size={15} /> {t.moveToTrash}
                </button>
              </div>
            )}
          </div>
        </div>
        {multi && view === "text" && (
          <div className="toc-row">
            <button className="icon-btn" disabled={currentOrd <= 0} title={t.previous} onClick={() => jump(doc.id, currentOrd - 1)}>
              <ChevronLeft size={18} />
            </button>
            <select value={currentOrd} onChange={(e) => jump(doc.id, Number(e.target.value))} title={t.contents}>
              {toc.map((e) => (
                <option key={e.ordinal} value={e.ordinal}>
                  {e.ordinal + 1}. {e.heading ?? doc.title}
                </option>
              ))}
            </select>
            <button className="icon-btn" disabled={currentOrd >= count - 1} title={t.next} onClick={() => jump(doc.id, currentOrd + 1)}>
              <ChevronRight size={18} />
            </button>
            <span className="pill mono">
              {currentOrd + 1} / {count}
            </span>
          </div>
        )}
      </header>

      {view === "original" ? (
        <iframe className="pdf-frame" src={fileUrl("original", doc.id)} title={doc.original ?? doc.title} />
      ) : (
        <div className="reader-scroll" data-ctx="reader" ref={scrollRef} onScroll={onScroll}>
          <div className={`reader-body ${serif ? "serif" : "sans"}`} style={{ fontSize }} ref={bodyRef} onClick={onBodyClick}>
            {doc.size < 50 && <p className="muted no-text">{doc.original ? t.noTextHint : ""}</p>}
            {lo > 0 && <div className="more-hint">• • •</div>}
            {sections.map((s) => (
              <SectionView key={s.id} s={s} terms={terms} multi={multi} />
            ))}
            {hi < count - 1 && <div className="more-hint">• • •</div>}
          </div>
        </div>
      )}

      {view === "text" && markIdx.total > 0 && (
        <div className="hit-pill">
          <button title={t.prevHit} onClick={() => focusMark(markIdx.cur - 2)}>
            <ChevronUp size={17} />
          </button>
          <span className="mono">
            {markIdx.cur} / {markIdx.total}
          </span>
          <button title={t.nextHit} onClick={() => focusMark(markIdx.cur)}>
            <ChevronDown size={17} />
          </button>
        </div>
      )}
    </div>
  );
}
