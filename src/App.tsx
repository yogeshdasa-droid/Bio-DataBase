import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  ArrowRight,
  ClipboardPaste,
  Copy,
  Download,
  Eraser,
  FileDown,
  FolderOpen,
  GitMerge,
  Link as LinkIcon,
  List as ListIcon,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  ListTree,
  ListX,
  Pencil,
  Quote,
  Scissors,
  Search,
  Settings as Gear,
  Star,
  Sun,
  TextCursorInput,
  TextSearch,
  Trash2,
  Upload,
  UploadCloud,
  UserPlus,
  UserRound,
  X,
} from "lucide-react";
import { ContextMenu, MenuItem, MenuState } from "./components/ContextMenu";
import {
  api,
  Bookmark,
  DocInfo,
  DocListItem,
  Hit,
  ImportProgress,
  ImportSummary,
  Person,
  PersonListItem,
  SearchResult,
  SectionText,
  Sort,
  Status,
  TreePlace,
} from "./api";
import { dicts, Lang } from "./i18n";
import { buildNameIndex } from "./personLinks";
import { EditDialog, ImportDialog, MergeDialog, PersonDialog, TrashDialog } from "./components/Dialogs";
import { Home, RecentDoc } from "./components/Home";
import { EMPTY_FILTERS, Filters, hasFilter, Results } from "./components/Results";
import { Reader, ReaderStyle, ReaderTarget } from "./components/Reader";
import { PersonView } from "./components/PersonView";
import { Settings, SettingsTab, Skin, Theme } from "./components/Settings";
import { SideTab, Sidebar } from "./components/Sidebar";
import { Kbd, Splitter } from "./components/ui";

const PAGE = 50;

function stored<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem("bio." + key);
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}
function usePersisted<T>(key: string, fallback: T) {
  const [v, setV] = useState<T>(() => stored(key, fallback));
  useEffect(() => {
    try {
      localStorage.setItem("bio." + key, JSON.stringify(v));
    } catch {
      /* storage unavailable */
    }
  }, [key, v]);
  return [v, setV] as const;
}

type Main = { kind: "home" } | { kind: "doc"; target: ReaderTarget } | { kind: "person"; id: number };

type Dialog =
  | { kind: "settings"; tab: SettingsTab }
  | { kind: "edit"; doc: DocInfo; section: SectionText }
  | { kind: "trash"; doc: DocInfo }
  | { kind: "import"; paths: string[]; person?: { id: number; name: string } }
  | { kind: "person"; person: Person | null }
  | { kind: "merge"; person: Person; suggested: number[] }
  | null;

export default function App() {
  const [lang, setLang] = usePersisted<Lang>("language", "es");
  const [theme, setTheme] = usePersisted<Theme>("theme", "system");
  const [skin, setSkin] = usePersisted<Skin>("skin", "vscode");
  const [readerStyle, setReaderStyle] = usePersisted<ReaderStyle>("readerStyle", "editor");
  const [fontSize, setFontSize] = usePersisted("fontSize", 18);
  const [titlesOnly, setTitlesOnly] = usePersisted("titlesOnly", false);
  const [prefix, setPrefix] = usePersisted("prefix", false);
  const [sort, setSort] = usePersisted<Sort>("sort", "relevance");
  const [history, setHistory] = usePersisted<string[]>("history", []);
  const [recent, setRecent] = usePersisted<RecentDoc[]>("recent", []);
  const [sideTab, setSideTab] = usePersisted<SideTab>("sideTab", "persons");
  const [sideW, setSideW] = usePersisted("sideW", 280);
  const [resW, setResW] = usePersisted("resW", 430);
  const [sideOpen, setSideOpen] = usePersisted("sideOpen", true);
  const t = dicts[lang];

  const [query, setQuery] = useState("");
  const [filters, setFilters] = useState<Filters>(EMPTY_FILTERS);
  const [result, setResult] = useState<SearchResult | null>(null);
  const [hits, setHits] = useState<Hit[]>([]);
  const [generation, setGeneration] = useState(0);
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState(-1);
  const [main, setMain] = useState<Main>({ kind: "home" });
  const [status, setStatus] = useState<Status | null>(null);
  const [homeFacets, setHomeFacets] = useState<SearchResult["facets"]>([]);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [docs, setDocs] = useState<DocListItem[]>([]);
  const [persons, setPersons] = useState<PersonListItem[]>([]);
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [dragging, setDragging] = useState(false);
  const [toast, setToast] = useState<{ msg: string; id: number } | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [readerDoc, setReaderDoc] = useState<DocInfo | null>(null);
  const [dataTick, setDataTick] = useState(0);
  const [resultsOpen, setResultsOpen] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const reqId = useRef(0);

  const target = main.kind === "doc" ? main.target : null;
  const personId = main.kind === "person" ? main.id : null;
  // names of the records, linked wherever a text mentions them
  const nameIndex = useMemo(() => buildNameIndex(persons), [persons]);

  // ---- back / forward, like a browser. Every change of `main` is a step;
  // a text is remembered at the section that was being read.
  const backStack = useRef<Main[]>([]);
  const fwdStack = useRef<Main[]>([]);
  const prevMain = useRef<Main>(main);
  const travelling = useRef(false);
  const readingOrd = useRef(new Map<number, number>()); // docId → section being read
  const [, setNavTick] = useState(0);
  const samePlace = (a: Main, b: Main) =>
    a.kind === b.kind && (a.kind === "home" || (a.kind === "person" ? a.id === (b as typeof a).id : a.target.docId === (b as typeof a).target.docId));
  const withPosition = (m: Main): Main =>
    m.kind === "doc" ? { ...m, target: { ...m.target, ordinal: readingOrd.current.get(m.target.docId) ?? m.target.ordinal } } : m;
  useEffect(() => {
    const prev = prevMain.current;
    prevMain.current = main;
    if (travelling.current) {
      travelling.current = false;
      return;
    }
    if (samePlace(prev, main)) return;
    backStack.current = [...backStack.current, withPosition(prev)].slice(-60);
    fwdStack.current = [];
    setNavTick((n) => n + 1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [main]);
  const travel = useCallback((dir: "back" | "fwd") => {
    const from = dir === "back" ? backStack : fwdStack;
    const to = dir === "back" ? fwdStack : backStack;
    const m = from.current[from.current.length - 1];
    if (!m) return;
    from.current = from.current.slice(0, -1);
    to.current = [...to.current, withPosition(prevMain.current)];
    travelling.current = true;
    setMain(m.kind === "doc" ? { ...m, target: { ...m.target, nonce: Date.now() } } : m);
    setNavTick((n) => n + 1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
        e.preventDefault();
        travel(e.key === "ArrowLeft" ? "back" : "fwd");
      }
    };
    // the mouse's side buttons
    const onMouse = (e: MouseEvent) => {
      if (e.button === 3 || e.button === 4) {
        e.preventDefault();
        travel(e.button === 3 ? "back" : "fwd");
      }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mouseup", onMouse);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mouseup", onMouse);
    };
  }, [travel]);

  // The results pane follows the search: it opens when there is a query or a
  // filter and closes when both are cleared. The toolbar button / Ctrl+J
  // override it at any time.
  const hasQuery = !!query.trim();
  const wantResults = hasQuery || hasFilter(filters);
  useEffect(() => setResultsOpen(wantResults), [wantResults]);

  const showToast = useCallback((msg: string) => {
    const id = Date.now();
    setToast({ msg, id });
    window.setTimeout(() => setToast((m) => (m?.id === id ? null : m)), 5200);
  }, []);
  const fail = useCallback((e: unknown) => showToast(String(e)), [showToast]);

  // ---- theme
  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
    root.dataset.skin = skin;
    root.lang = lang;
  }, [theme, skin, lang]);
  const [systemDark, setSystemDark] = useState(() => window.matchMedia("(prefers-color-scheme: dark)").matches);
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const on = () => setSystemDark(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  const isDark = theme === "dark" || (theme === "system" && systemDark);

  // ---- splash (index.html): fade out once the first data is in, or after 3 s
  useEffect(() => {
    const hide = () => {
      const s = document.getElementById("splash");
      if (!s || s.classList.contains("hide")) return;
      s.classList.add("hide");
      window.setTimeout(() => s.remove(), 400);
    };
    if (status) hide();
    const h = window.setTimeout(hide, 3000);
    return () => window.clearTimeout(h);
  }, [status]);

  // ---- data refresh
  const [fatal, setFatal] = useState<string | null>(null);
  const refreshStatus = useCallback(
    () =>
      api
        .status()
        .then((s) => (setStatus(s), s))
        .catch((e) => {
          setFatal(String(e));
          document.getElementById("splash")?.remove();
          return null;
        }),
    [],
  );
  const refreshAll = useCallback(() => {
    refreshStatus();
    api.listDocs().then(setDocs).catch(() => {});
    api.listPersons().then(setPersons).catch(() => {});
    api.listBookmarks().then(setBookmarks).catch(() => {});
    api
      .search({ query: "", titlesOnly: false, prefix: false, sort: "relevance", collections: [], dir: null, docId: null, personId: null, offset: 0, limit: 1 })
      .then((r) => setHomeFacets(r.facets))
      .catch(() => {});
    setDataTick((n) => n + 1);
  }, [refreshStatus]);

  useEffect(() => {
    refreshAll();
    let lastRefresh = 0;
    const unsubs = [
      listen<ImportProgress>("import-progress", (e) => {
        setProgress(e.payload);
        // show what is already in while a long import goes on
        if (e.payload.phase === "import" && Date.now() - lastRefresh > 4000) {
          lastRefresh = Date.now();
          refreshAll();
        }
      }),
      listen<ImportSummary>("import-done", (e) => {
        const s = e.payload;
        const d = dicts[stored<Lang>("language", "es")];
        if (s.added + s.duplicates + s.failed.length === 0) showToast(d.searchReady);
        else showToast(d.importDone(s.added, s.duplicates, s.persons, s.millis / 1000) + (s.failed.length ? ` · ${d.importFailed(s.failed.length)}` : ""));
      }),
      listen<string>("import-error", (e) => showToast(`Error: ${e.payload}`)),
      listen("busy-end", () => {
        setProgress(null);
        refreshAll();
      }),
      getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload;
        if (p.type === "enter" || p.type === "over") setDragging(true);
        else if (p.type === "leave") setDragging(false);
        else if (p.type === "drop") {
          setDragging(false);
          if (p.paths.length) setDialog({ kind: "import", paths: p.paths });
        }
      }),
    ];
    return () => unsubs.forEach((u) => u.then((f) => f()));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ---- search (debounced, newest request wins)
  const runSearch = useCallback(
    async (offset: number) => {
      const id = ++reqId.current;
      setLoading(true);
      try {
        const r = await api.search({
          query,
          titlesOnly,
          prefix,
          sort,
          collections: filters.collections,
          dir: filters.scope?.dir ?? null,
          docId: filters.doc?.id ?? null,
          personId: filters.person?.id ?? null,
          offset,
          limit: PAGE,
        });
        if (id !== reqId.current) return;
        setResult(r);
        if (offset === 0) {
          setHits(r.hits);
          setSelected(-1);
          setGeneration((g) => g + 1);
        } else setHits((h) => [...h, ...r.hits]);
      } catch (e) {
        if (id === reqId.current) showToast(String(e));
      } finally {
        if (id === reqId.current) setLoading(false);
      }
    },
    [query, titlesOnly, prefix, sort, filters, showToast],
  );

  useEffect(() => {
    const h = window.setTimeout(() => runSearch(0), query ? 160 : 0);
    return () => window.clearTimeout(h);
  }, [runSearch, dataTick, query]);

  const remember = useCallback(() => {
    const q = query.trim();
    if (q) setHistory((h) => [q, ...h.filter((x) => x !== q)].slice(0, 40));
  }, [query, setHistory]);

  const openHit = useCallback(
    (i: number) => {
      const h = hits[i];
      if (!h) return;
      setSelected(i);
      remember();
      setMain({ kind: "doc", target: { docId: h.docId, ordinal: h.ordinal, terms: query.trim() ? result?.terms ?? [] : [], nonce: Date.now() } });
    },
    [hits, query, result, remember],
  );

  const openDoc = useCallback((docId: number, ordinal = 0) => {
    setMain({ kind: "doc", target: { docId, ordinal, terms: [], nonce: Date.now() } });
  }, []);
  const openPerson = useCallback((id: number) => setMain({ kind: "person", id }), []);

  const onDocLoaded = useCallback(
    (d: DocInfo) => {
      setReaderDoc(d);
      setRecent((r) => [{ docId: d.id, title: d.title, collection: d.collection }, ...r.filter((x) => x.docId !== d.id)].slice(0, 20));
    },
    [setRecent],
  );

  // ---- keyboard
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = ["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName ?? "");
      if ((e.ctrlKey && (e.key === "k" || e.key === "f")) || (e.key === "/" && !typing)) {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      } else if (e.ctrlKey && e.key === "b") {
        e.preventDefault();
        setSideOpen((o) => !o);
      } else if (e.ctrlKey && e.key === "j") {
        e.preventDefault();
        setResultsOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setSideOpen]);

  const onSearchKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      openHit(Math.min(hits.length - 1, selected + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      openHit(Math.max(0, selected - 1));
    } else if (e.key === "Enter") openHit(selected < 0 ? 0 : selected);
    else if (e.key === "Escape") setQuery("");
  };

  // ---- actions
  const onBookmark = async (docId: number, ordinal: number) => {
    try {
      await api.addBookmark(docId, ordinal, "");
      setBookmarks(await api.listBookmarks());
      showToast(t.bookmarkAdded);
    } catch (e) {
      fail(e);
    }
  };

  const pickImport = async (person?: { id: number; name: string }) => {
    const files = await open({
      multiple: true,
      filters: [{ name: "Documentos (md, txt, pdf, docx, doc, html)", extensions: ["md", "markdown", "txt", "pdf", "docx", "doc", "html", "htm"] }],
    });
    const paths = Array.isArray(files) ? files : typeof files === "string" ? [files] : [];
    if (paths.length) setDialog({ kind: "import", paths, person });
  };
  const pickImportFolder = async () => {
    const dir = await open({ directory: true });
    if (typeof dir === "string") setDialog({ kind: "import", paths: [dir] });
  };

  const importDefaults = async () => {
    try {
      const list = await api.defaultSources();
      await api.importSources({ sources: list.filter((s) => s.exists), personPerFile: false, personId: null });
      showToast(t.importStarted);
      refreshStatus();
    } catch (e) {
      fail(e);
    }
  };

  const saveOriginal = async (doc: DocInfo) => {
    if (!doc.original) return;
    const ext = doc.original.split(".").pop() ?? "pdf";
    const path = await save({ defaultPath: doc.original, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
    if (path) api.saveOriginal(doc.id, path).then(() => showToast(t.exported), fail);
  };
  const exportDoc = async (doc: DocInfo) => {
    const path = await save({ defaultPath: `${doc.title.replace(/[\\/:*?"<>|]/g, "-")}.md`, filters: [{ name: "Markdown", extensions: ["md"] }] });
    if (path) api.exportDoc(doc.id, path).then(() => showToast(t.exported), fail);
  };

  const editSectionOf = (docId: number, ordinal: number) =>
    api
      .getDoc(docId)
      .then(async ({ doc }) => {
        const [section] = await api.getSections(docId, ordinal, ordinal);
        if (section) setDialog({ kind: "edit", doc, section });
      })
      .catch(fail);

  const addTextFor = async (p: Person) => {
    try {
      const id = await api.createDoc(p.id, t.newTextTitle(p.name), "Mis textos");
      refreshAll();
      openDoc(id);
      editSectionOf(id, 0);
    } catch (e) {
      fail(e);
    }
  };

  const withPerson = (id: number, f: (p: Person) => unknown) => () => api.getPerson(id).then(f).catch(fail);
  const deletePerson = (p: Person) => {
    if (!window.confirm(t.deletePersonConfirm(p.name))) return;
    api
      .deletePerson(p.id)
      .then(() => {
        showToast(t.personDeleted);
        setMain({ kind: "home" });
        refreshAll();
      })
      .catch(fail);
  };
  const movePerson = (id: number, place: TreePlace) =>
    api
      .movePerson(id, place)
      .then(() => api.listPersons().then(setPersons))
      .catch(fail);
  const findMentions = (p: Person) => {
    setFilters(EMPTY_FILTERS);
    setTitlesOnly(false);
    setQuery(`"${p.name.replace(/"/g, "")}"`);
  };
  const searchInPerson = (p: Person) => {
    setFilters({ ...EMPTY_FILTERS, person: { id: p.id, name: p.name } });
    searchRef.current?.focus();
  };

  // ---- right-click menu (replaces the browser's own menu everywhere)
  const copyText = async (s: string) => {
    try {
      await navigator.clipboard.writeText(s);
      showToast(t.copied);
    } catch (e) {
      fail(e);
    }
  };
  const withDoc = (docId: number, f: (d: DocInfo) => unknown) => () =>
    api
      .getDoc(docId)
      .then((r) => f(r.doc))
      .catch(fail);
  const docItems = (docId: number, ordinal: number, here = false): MenuItem[] => [
    { label: here ? t.bookmarkHere : t.addBookmark, icon: Star, onClick: () => onBookmark(docId, ordinal) },
    { label: here ? t.editThisSection : t.editTitle, icon: Pencil, onClick: () => editSectionOf(docId, ordinal) },
    {
      label: t.searchInDoc,
      icon: TextSearch,
      onClick: withDoc(docId, (d) => {
        setFilters((f) => ({ ...f, doc: { id: d.id, title: d.title } }));
        searchRef.current?.focus();
      }),
    },
    { label: t.openPerson, icon: UserRound, onClick: withDoc(docId, (d) => (d.personId ? openPerson(d.personId) : showToast(t.noPerson))) },
    "sep",
    { label: t.saveOriginal, icon: Download, onClick: withDoc(docId, (d) => (d.original ? saveOriginal(d) : showToast("—"))) },
    { label: t.exportText, icon: FileDown, onClick: withDoc(docId, exportDoc) },
    "sep",
    { label: `${t.moveToTrash}…`, icon: Trash2, danger: true, onClick: withDoc(docId, (doc) => setDialog({ kind: "trash", doc })) },
  ];

  const buildMenu = (e: MouseEvent): MenuState | null => {
    const el = e.target as HTMLElement;
    const at = { x: e.clientX, y: e.clientY };

    const input = el.closest<HTMLInputElement | HTMLTextAreaElement>("input:not([type=checkbox]), textarea");
    if (input) {
      const hasSel = input.selectionStart !== input.selectionEnd;
      const ro = input.readOnly || input.disabled;
      return {
        ...at,
        items: [
          { label: t.cut, icon: Scissors, hint: "Ctrl X", disabled: !hasSel || ro, onClick: () => (input.focus(), document.execCommand("cut")) },
          { label: t.copy, icon: Copy, hint: "Ctrl C", disabled: !hasSel, onClick: () => (input.focus(), document.execCommand("copy")) },
          {
            label: t.paste,
            icon: ClipboardPaste,
            hint: "Ctrl V",
            disabled: ro,
            onClick: async () => {
              input.focus();
              try {
                document.execCommand("insertText", false, await navigator.clipboard.readText());
              } catch (err) {
                fail(err);
              }
            },
          },
          "sep",
          { label: t.selectAll, icon: TextCursorInput, hint: "Ctrl A", onClick: () => (input.focus(), input.select()) },
        ],
      };
    }

    const ctxEl = el.closest<HTMLElement>("[data-ctx]");
    const ctx = ctxEl?.dataset.ctx;
    const items: MenuItem[] = [];
    const sel = (window.getSelection()?.toString() ?? "").replace(/\s+/g, " ").trim();
    if (sel) {
      const short = sel.length > 30 ? sel.slice(0, 30) + "…" : sel;
      const q = /\s/.test(sel) ? `"${sel.replace(/"/g, "")}"` : sel;
      items.push({ label: t.copy, icon: Copy, hint: "Ctrl C", onClick: () => copyText(window.getSelection()?.toString() ?? sel) });
      if (ctx === "reader" && readerDoc) {
        const ref = `— ${readerDoc.person && readerDoc.person !== readerDoc.title ? `${readerDoc.person}, ` : ""}${readerDoc.title}`;
        items.push({ label: t.copyWithRef, icon: Quote, onClick: () => copyText(`«${window.getSelection()?.toString().trim() ?? sel}»\n${ref}`) });
      }
      items.push({
        label: t.searchFor(short),
        icon: Search,
        onClick: () => {
          setFilters((f) => ({ ...f, doc: null }));
          setQuery(q);
        },
      });
      if (ctx === "reader" && readerDoc) {
        const d = readerDoc;
        items.push({
          label: t.searchForInDoc(short),
          icon: TextSearch,
          onClick: () => {
            setFilters((f) => ({ ...f, doc: { id: d.id, title: d.title } }));
            setQuery(q);
          },
        });
      }
      if (ctx !== "reader") return { ...at, items };
      items.push("sep");
    }

    if (ctx === "reader" && readerDoc) {
      const ord = Number(el.closest<HTMLElement>("section[data-ord]")?.dataset.ord ?? 0);
      const link = el.closest("a");
      if (link) items.push({ label: t.openLink, icon: LinkIcon, onClick: () => link.click() }, "sep");
      return { ...at, title: readerDoc.title, items: [...items, ...docItems(readerDoc.id, ord, true)] };
    }
    if (ctx === "hit") {
      const i = Number(ctxEl!.dataset.i);
      const h = hits[i];
      if (h) return { ...at, title: h.title, items: [{ label: t.open, icon: FolderOpen, onClick: () => openHit(i) }, "sep", ...docItems(h.docId, h.ordinal)] };
    }
    if (ctx === "doc") {
      const id = Number(ctxEl!.dataset.id);
      const d = docs.find((x) => x.id === id);
      return { ...at, title: d?.title, items: [{ label: t.open, icon: FolderOpen, onClick: () => openDoc(id) }, "sep", ...docItems(id, 0)] };
    }
    if (ctx === "person") {
      const id = Number(ctxEl!.dataset.id);
      const p = persons.find((x) => x.id === id);
      return {
        ...at,
        title: p?.name,
        items: [
          { label: t.openPerson, icon: UserRound, onClick: () => openPerson(id) },
          { label: t.editPerson, icon: Pencil, onClick: withPerson(id, (person) => setDialog({ kind: "person", person })) },
          { label: t.searchInPerson, icon: TextSearch, onClick: withPerson(id, searchInPerson) },
          { label: t.findMentions, icon: Search, onClick: withPerson(id, findMentions) },
          { label: t.mergePersons, icon: GitMerge, onClick: withPerson(id, (person) => setDialog({ kind: "merge", person, suggested: [] })) },
          { label: t.importFiles, icon: Upload, onClick: () => p && pickImport({ id: p.id, name: p.name }) },
          p?.tree
            ? { label: t.removeFromParampara, icon: ListX, onClick: () => movePerson(id, null) }
            : { label: t.addToParampara, icon: ListTree, onClick: () => movePerson(id, { parent: null, before: null }) },
          "sep",
          { label: t.deletePerson, icon: Trash2, danger: true, onClick: withPerson(id, deletePerson) },
        ],
      };
    }
    if (ctx === "folder") {
      const dir = ctxEl!.dataset.dir ?? "";
      const label = dir.split("/").pop() ?? dir;
      return {
        ...at,
        title: label,
        items: [
          {
            label: t.searchInFolder,
            icon: Search,
            onClick: () => {
              setFilters((f) => ({ ...f, doc: null, scope: { dir, label } }));
              searchRef.current?.focus();
            },
          },
          {
            label: t.renameFolder,
            icon: Pencil,
            onClick: () => {
              const name = window.prompt(t.renamePrompt, label);
              if (name && name.trim() && name !== label) api.renameFolder(dir, name).then(refreshAll, fail);
            },
          },
        ],
      };
    }
    if (ctx === "bookmark") {
      const b = bookmarks.find((x) => x.id === Number(ctxEl!.dataset.id));
      if (b)
        return {
          ...at,
          title: b.title,
          items: [
            { label: t.open, icon: FolderOpen, disabled: !b.docId, onClick: () => b.docId && openDoc(b.docId, b.ordinal) },
            "sep",
            { label: t.deleteBookmark, icon: Trash2, danger: true, onClick: () => api.deleteBookmark(b.id).then(() => api.listBookmarks().then(setBookmarks)) },
          ],
        };
    }
    return {
      ...at,
      items: [
        { label: t.focusSearch, icon: Search, hint: "Ctrl K", onClick: () => searchRef.current?.focus() },
        { label: t.clearSearch, icon: Eraser, disabled: !query && !hasFilter(filters), onClick: () => (setQuery(""), setFilters(EMPTY_FILTERS)) },
        "sep",
        { label: t.newPerson, icon: UserPlus, onClick: () => setDialog({ kind: "person", person: null }) },
        { label: t.importFiles, icon: Upload, onClick: () => pickImport() },
        { label: `${t.importFiles.replace("…", "")} (${t.library.toLowerCase()})…`, icon: FolderOpen, onClick: pickImportFolder },
        "sep",
        { label: t.settings, icon: Gear, onClick: () => setDialog({ kind: "settings", tab: "general" }) },
      ],
    };
  };
  // The listener is attached once; it always calls the latest builder.
  const buildMenuRef = useRef(buildMenu);
  buildMenuRef.current = buildMenu;
  useEffect(() => {
    const onCtx = (e: MouseEvent) => {
      e.preventDefault();
      setMenu(buildMenuRef.current(e));
    };
    window.addEventListener("contextmenu", onCtx);
    return () => window.removeEventListener("contextmenu", onCtx);
  }, []);

  const [personReload, setPersonReload] = useState(0);
  const reloadPerson = () => setPersonReload((n) => n + 1);

  const pct = progress && progress.total ? Math.round((progress.done / progress.total) * 100) : null;
  // In a narrow window the side panes give way so the reader always fits:
  // first the results pane shrinks, then the sidebar.
  const [winW, setWinW] = useState(window.innerWidth);
  useEffect(() => {
    const on = () => setWinW(window.innerWidth);
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  const MAIN_MIN = 380;
  let side = sideOpen ? sideW : 0;
  let res = resultsOpen ? resW : 0;
  const spare = winW - MAIN_MIN - (sideOpen ? 5 : 0) - (resultsOpen ? 5 : 0);
  if (side + res > spare) {
    if (resultsOpen) res = Math.max(260, spare - side);
    if (sideOpen && side + res > spare) side = Math.max(190, spare - res);
  }
  const panesStyle = {
    gridTemplateColumns: `${sideOpen ? `${side}px 5px ` : ""}${resultsOpen ? `${res}px 5px ` : ""}minmax(0, 1fr)`,
  } as React.CSSProperties;

  if (fatal) {
    return (
      <div className="empty-state" style={{ height: "100%", justifyContent: "center" }}>
        <h2>Bio-Data</h2>
        <p style={{ whiteSpace: "pre-wrap", maxWidth: 520 }}>{fatal}</p>
      </div>
    );
  }

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <button className="icon-btn" onClick={() => setSideOpen((o) => !o)} title={sideOpen ? t.collapse : t.expand}>
            {sideOpen ? <PanelLeftClose size={18} /> : <PanelLeftOpen size={18} />}
          </button>
          <button className="logo" onClick={() => setMain({ kind: "home" })} title={t.home}>
            B
          </button>
          <span className="brand-name">Bio-Data</span>
          <button className={`icon-btn ${resultsOpen ? "on" : ""}`} onClick={() => setResultsOpen((o) => !o)} title={t.toggleResults}>
            <ListIcon size={18} />
          </button>
          <span className="nav-btns">
            <button className="icon-btn" disabled={!backStack.current.length} onClick={() => travel("back")} title={t.goBack}>
              <ArrowLeft size={18} />
            </button>
            <button className="icon-btn" disabled={!fwdStack.current.length} onClick={() => travel("fwd")} title={t.goForward}>
              <ArrowRight size={18} />
            </button>
          </span>
        </div>
        <div className="searchbox">
          <Search size={18} />
          <input
            ref={searchRef}
            autoFocus
            value={query}
            spellCheck={false}
            placeholder={t.searchPlaceholder}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onSearchKey}
          />
          {query ? (
            <button className="icon-btn" onClick={() => setQuery("")} title={t.close}>
              <X size={16} />
            </button>
          ) : (
            <span className="search-hint">
              {t.searchHint} <Kbd>Ctrl K</Kbd>
            </span>
          )}
        </div>
        <div className="search-opts">
          <div className="seg small" data-on={titlesOnly ? 1 : 0}>
            <span className="seg-ink" />
            <button className={!titlesOnly ? "on" : ""} onClick={() => setTitlesOnly(false)}>
              {t.modeText}
            </button>
            <button className={titlesOnly ? "on" : ""} onClick={() => setTitlesOnly(true)}>
              {t.modeTitles}
            </button>
          </div>
          <label className={`toggle ${prefix ? "on" : ""}`} title={t.prefixHint}>
            <input type="checkbox" checked={prefix} onChange={(e) => setPrefix(e.target.checked)} />
            <span className="switch" />
            {t.prefix}
          </label>
          <select value={sort} onChange={(e) => setSort(e.target.value as Sort)}>
            <option value="relevance">{t.sortRelevance}</option>
            <option value="person">{t.sortPerson}</option>
            <option value="title">{t.sortTitle}</option>
          </select>
        </div>
        <div className="top-actions">
          <button className="lang-btn" onClick={() => setLang(lang === "es" ? "en" : "es")} title={t.language}>
            {lang.toUpperCase()}
          </button>
          <button className="icon-btn" onClick={() => setTheme(isDark ? "light" : "dark")} title={t.theme}>
            {isDark ? <Sun size={18} /> : <Moon size={18} />}
          </button>
          <button className="icon-btn" onClick={() => setDialog({ kind: "settings", tab: "general" })} title={t.settings}>
            <Gear size={18} />
          </button>
        </div>
      </header>

      <main className="panes" style={panesStyle}>
        {sideOpen && (
          <>
            <Sidebar
              t={t}
              lang={lang}
              tab={sideTab}
              onTab={setSideTab}
              persons={persons}
              activePersonId={personId}
              onPerson={openPerson}
              onNewPerson={() => setDialog({ kind: "person", person: null })}
              onMovePerson={movePerson}
              docs={docs}
              activeDir={filters.scope?.dir ?? null}
              activeDocId={target?.docId ?? null}
              onFolder={(dir) =>
                setFilters((f) => (f.scope?.dir === dir ? { ...f, scope: null } : { ...f, doc: null, scope: { dir, label: dir.split("/").pop() ?? dir } }))
              }
              onDoc={openDoc}
              onImport={() => pickImport()}
              bookmarks={bookmarks}
              onBookmarkNote={(id, note) => api.updateBookmark(id, note).then(() => api.listBookmarks().then(setBookmarks))}
              onBookmarkDelete={(id) => api.deleteBookmark(id).then(() => api.listBookmarks().then(setBookmarks))}
              history={history}
              onHistory={(q) => {
                setQuery(q);
                searchRef.current?.focus();
              }}
              onClearHistory={() => setHistory([])}
            />
            <Splitter onDrag={(dx) => setSideW((w) => Math.min(480, Math.max(200, w + dx)))} onDoubleClick={() => setSideW(280)} />
          </>
        )}
        {resultsOpen && (
          <>
            <Results
              t={t}
              lang={lang}
              result={result}
              hits={hits}
              loading={loading}
              selected={selected}
              onSelect={openHit}
              onPerson={openPerson}
              onLoadMore={() => runSearch(hits.length)}
              filters={filters}
              onFilters={setFilters}
              hasQuery={hasQuery}
              generation={generation}
            />
            <Splitter onDrag={(dx) => setResW((w) => Math.min(760, Math.max(300, w + dx)))} onDoubleClick={() => setResW(430)} />
          </>
        )}
        {main.kind === "doc" && (
          <Reader
            key={main.target.docId}
            target={main.target}
            t={t}
            lang={lang}
            fontSize={fontSize}
            readerStyle={readerStyle}
            names={nameIndex}
            onFontSize={setFontSize}
            onOpenDoc={openDoc}
            onPerson={openPerson}
            onLoaded={onDocLoaded}
            onSection={(docId, ord) => readingOrd.current.set(docId, ord)}
            onSearchInDoc={(doc) => {
              setFilters((f) => ({ ...f, doc: { id: doc.id, title: doc.title } }));
              searchRef.current?.focus();
            }}
            onBookmark={onBookmark}
            onEdit={(doc, section) => setDialog({ kind: "edit", doc, section })}
            onTrash={(doc) => setDialog({ kind: "trash", doc })}
            onSaveOriginal={saveOriginal}
            onExport={exportDoc}
            onToast={showToast}
          />
        )}
        {main.kind === "person" && (
          <PersonView
            id={main.id}
            reload={personReload}
            t={t}
            lang={lang}
            persons={persons}
            onPerson={openPerson}
            onOpenDoc={openDoc}
            onEdit={(person) => setDialog({ kind: "person", person })}
            onMerge={(person, suggested) => setDialog({ kind: "merge", person, suggested })}
            onDelete={deletePerson}
            onAddText={addTextFor}
            onFindMentions={findMentions}
            onSearchIn={searchInPerson}
            onEditDoc={(docId) => editSectionOf(docId, 0)}
            onPhotoChanged={() => {
              reloadPerson();
              refreshAll();
            }}
            onToast={showToast}
            readerStyle={readerStyle}
            fontSize={fontSize}
            names={nameIndex}
          />
        )}
        {main.kind === "home" && (
          <Home
            t={t}
            lang={lang}
            status={status}
            facets={homeFacets}
            persons={persons}
            recent={recent}
            bookmarks={bookmarks}
            onCollection={(name) => setFilters({ ...EMPTY_FILTERS, collections: [name] })}
            onPerson={openPerson}
            onRecent={(r) => openDoc(r.docId)}
            onBookmark={(b) => b.docId && openDoc(b.docId, b.ordinal)}
            onImportDefaults={importDefaults}
          />
        )}
      </main>

      <footer className="statusbar">
        {status && (
          <span>
            {t.personsCount(status.persons)} · {t.documents(status.docs)}
          </span>
        )}
        {progress && (
          <span className="progress">
            <span className="spinner" />
            {progress.phase === "scan"
              ? t.scanning
              : `${progress.phase === "fts" ? t.rebuilding : t.importing} ${progress.done.toLocaleString(lang)} / ${progress.total.toLocaleString(lang)}`}
            {pct !== null && (
              <span className="bar">
                <span style={{ width: `${pct}%` }} />
              </span>
            )}
          </span>
        )}
        {!progress && status?.busy && (
          <span className="progress">
            <span className="spinner" />
            {t.busy}
          </span>
        )}
        <span className="spacer" />
        <span className="muted">{status ? `${Math.round(status.dbSize / 1048576)} MB` : ""}</span>
      </footer>

      {dragging && (
        <div className="drop-overlay">
          <div className="drop-card">
            <UploadCloud size={44} />
            <strong>{t.dropTitle}</strong>
            <span>{t.dropSub}</span>
          </div>
        </div>
      )}

      {dialog?.kind === "settings" && (
        <Settings
          t={t}
          lang={lang}
          status={status}
          theme={theme}
          skin={skin}
          readerStyle={readerStyle}
          initialTab={dialog.tab}
          onLang={setLang}
          onTheme={setTheme}
          onSkin={setSkin}
          onReaderStyle={setReaderStyle}
          onClose={() => setDialog(null)}
          onToast={showToast}
          onChanged={refreshAll}
          onImportStarted={() => {
            showToast(t.importStarted);
            refreshStatus();
          }}
        />
      )}
      {dialog?.kind === "edit" && (
        <EditDialog
          t={t}
          doc={dialog.doc}
          section={dialog.section}
          persons={persons}
          onClose={() => setDialog(null)}
          onSaved={(docId, ordinal) => {
            setDialog(null);
            showToast(t.saved);
            // edited from a record (its single text shown inline): stay there
            if (main.kind === "person") reloadPerson();
            else setMain({ kind: "doc", target: { docId, ordinal, terms: target?.terms ?? [], nonce: Date.now() } });
            refreshAll();
          }}
        />
      )}
      {dialog?.kind === "trash" && (
        <TrashDialog
          t={t}
          doc={dialog.doc}
          onClose={() => setDialog(null)}
          onDone={() => {
            const doc = dialog.doc;
            setDialog(null);
            showToast(t.trashedDone);
            setMain(doc.personId ? { kind: "person", id: doc.personId } : { kind: "home" });
            reloadPerson();
            refreshAll();
          }}
        />
      )}
      {dialog?.kind === "import" && (
        <ImportDialog
          t={t}
          paths={dialog.paths}
          person={dialog.person}
          onClose={() => setDialog(null)}
          onStarted={() => {
            setDialog(null);
            showToast(t.importStarted);
            refreshStatus();
          }}
        />
      )}
      {dialog?.kind === "person" && (
        <PersonDialog
          t={t}
          person={dialog.person}
          onClose={() => setDialog(null)}
          onSaved={(id) => {
            setDialog(null);
            showToast(t.personSaved);
            openPerson(id);
            reloadPerson();
            refreshAll();
          }}
        />
      )}
      {dialog?.kind === "merge" && (
        <MergeDialog
          t={t}
          person={dialog.person}
          persons={persons}
          suggested={dialog.suggested}
          onClose={() => setDialog(null)}
          onDone={() => {
            const id = dialog.person.id;
            setDialog(null);
            showToast(t.merged);
            openPerson(id);
            reloadPerson();
            refreshAll();
          }}
        />
      )}
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}
      {toast && (
        <div className="toast" key={toast.id}>
          {toast.msg}
        </div>
      )}
    </div>
  );
}
