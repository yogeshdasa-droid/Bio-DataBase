import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { BookOpen, FilePlus2, FileText, GitMerge, ImagePlus, MoreHorizontal, Pencil, Search, TextSearch, Trash2, UserRound, X } from "lucide-react";
import { api, fileUrl, Person, PersonListItem, SectionText } from "../api";
import { formatSize, Lang, T } from "../i18n";
import { CollectionChip } from "./ui";
import { ReaderStyle, SectionView } from "./Reader";
import type { NameIndex } from "../personLinks";
import type { Term } from "../fold";
import { foldName } from "./Sidebar";

/** Titles that come and go after a name: "Rupa" = "Rupa Gosvami". */
const TITLES = new Set(
  ["sri", "srila", "srimati", "gosvami", "goswami", "thakura", "thakur", "dasa", "das", "prabhu", "babaji", "pandita", "pandit", "acarya", "acharya", "maharaja", "swami", "svami", "devi", "mata", "thakurani", "kaviraja", "mahasaya", "bhattacarya", "misra", "puri", "bharati"].map(foldName),
);

/** Name without titles and final "a"s: the core used to suggest merges. */
export function coreName(name: string): string {
  return foldName(name)
    .split(" ")
    .filter((w) => w && !TITLES.has(w))
    .map((w) => (w.length > 3 && w.endsWith("a") ? w.slice(0, -1) : w))
    .join(" ");
}

interface Props {
  id: number;
  reload: number;
  t: T;
  lang: Lang;
  persons: PersonListItem[];
  onPerson: (id: number) => void;
  onOpenDoc: (docId: number) => void;
  onEdit: (p: Person) => void;
  onMerge: (p: Person, suggested: number[]) => void;
  onDelete: (p: Person) => void;
  onAddText: (p: Person) => void;
  onFindMentions: (p: Person) => void;
  onSearchIn: (p: Person) => void;
  onEditDoc: (docId: number) => void;
  onPhotoChanged: () => void;
  onToast: (msg: string) => void;
  readerStyle: ReaderStyle;
  fontSize: number;
  names: NameIndex | null;
}

const NO_TERMS: Term[] = [];

/** How much of a single text is shown inside the record (the rest in the reader). */
const INLINE_SECTIONS = 4;

export function PersonView(p: Props) {
  const { t, lang } = p;
  const [person, setPerson] = useState<Person | null>(null);
  const [menu, setMenu] = useState(false);

  useEffect(() => {
    let cancelled = false;
    api
      .getPerson(p.id)
      .then((r) => !cancelled && setPerson(r))
      .catch((e) => p.onToast(String(e)));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.id, p.reload]);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(false);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [menu]);

  // folded name → id, to turn names written in the fields into links
  const byName = useMemo(() => {
    const m = new Map<string, number>();
    for (const x of p.persons) {
      m.set(foldName(x.name), x.id);
      for (const a of x.aka.split(",")) if (a.trim()) m.set(foldName(a), x.id);
    }
    return m;
  }, [p.persons]);

  const similar = useMemo(() => {
    if (!person) return [];
    const core = coreName(person.name);
    if (!core) return [];
    return p.persons.filter((x) => x.id !== person.id && coreName(x.name) === core).slice(0, 8);
  }, [person, p.persons]);

  // A record with a single text shows it right here, no click needed.
  const single = person && person.id === p.id && person.docs.length === 1 ? person.docs[0] : null;
  const [inline, setInline] = useState<{ docId: number; sections: SectionText[]; total: number } | null>(null);
  useEffect(() => {
    setInline(null);
    if (!single) return;
    let cancelled = false;
    api
      .getDoc(single.id)
      .then(async ({ doc }) => {
        const n = Math.min(doc.sections, INLINE_SECTIONS);
        const sections = n ? await api.getSections(doc.id, 0, n - 1) : [];
        if (!cancelled) setInline({ docId: doc.id, sections, total: doc.sections });
      })
      .catch((e) => p.onToast(String(e)));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [single?.id, p.reload]);

  const onInlineClick = async (e: React.MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const wl = a.getAttribute("data-wikilink");
    try {
      if (a.dataset.person) p.onPerson(Number(a.dataset.person));
      else if (wl) {
        const r = await api.resolveLink(wl);
        if (r.kind === "doc") p.onOpenDoc(r.id);
        else if (r.kind === "person") p.onPerson(r.id);
        else p.onToast(`? ${wl}`);
      } else {
        const href = a.getAttribute("href");
        if (href && /^(https?:|mailto:)/.test(href)) await api.openExternal(href);
      }
    } catch (err) {
      p.onToast(String(err));
    }
  };

  if (!person || person.id !== p.id) return <div className="reader" />;

  const cover = person.docs.find((d) => d.hasCover);
  const choosePhoto = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: t.imageFiles, extensions: ["jpg", "jpeg", "png", "webp", "gif", "bmp", "avif"] }],
      });
      if (typeof path !== "string") return;
      await api.setPersonPhoto(person.id, path);
      p.onPhotoChanged();
    } catch (e) {
      p.onToast(String(e));
    }
  };
  const removePhoto = () => {
    if (!window.confirm(t.removePhotoConfirm)) return;
    api.removePersonPhoto(person.id).then(p.onPhotoChanged, (e) => p.onToast(String(e)));
  };
  const names = (value: string) =>
    value
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean)
      .map((s, i) => {
        const id = byName.get(foldName(s));
        return id && id !== person.id ? (
          <button key={i} className="name-link" onClick={() => p.onPerson(id)}>
            {s}
          </button>
        ) : (
          <span key={i} className="name-plain">
            {s}
          </span>
        );
      });

  const facts: [string, string, boolean][] = [
    [t.appearance, person.appearance, false],
    [t.disappearance, person.disappearance, false],
    [t.place, person.place, false],
    [t.guru, person.guru, true],
    [t.disciples, person.disciples, true],
    [t.works, person.works, false],
  ];
  const hasFacts = facts.some(([, v]) => v.trim()) || person.notes.trim();

  return (
    <div className="reader person-view">
      <div className="reader-scroll">
        <div className="person-inner">
          <header className="person-head">
            <div className="person-cover-wrap">
              <button className="person-cover" title={t.choosePhoto} onClick={choosePhoto}>
                {person.hasPhoto ? (
                  <img src={`${fileUrl("photo", person.id)}?v=${encodeURIComponent(person.modified)}`} alt="" />
                ) : cover ? (
                  <img src={fileUrl("cover", cover.id)} alt="" />
                ) : (
                  <UserRound size={44} />
                )}
                <span className="cover-overlay">
                  <ImagePlus size={20} />
                  {person.hasPhoto ? t.changePhoto : t.choosePhoto}
                </span>
              </button>
              {person.hasPhoto && (
                <button className="icon-btn cover-remove" title={t.removePhoto} onClick={removePhoto}>
                  <X size={14} />
                </button>
              )}
            </div>
            <div className="person-title">
              <h1>{person.name}</h1>
              {person.aka && <div className="aka">{person.aka}</div>}
              <div className="person-actions">
                <button className="btn" onClick={() => p.onEdit(person)}>
                  <Pencil size={15} /> {t.editPerson}
                </button>
                <button className="btn ghost" onClick={() => p.onFindMentions(person)} title={t.findMentions}>
                  <Search size={15} /> {t.findMentions}
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
                      <button onClick={() => (setMenu(false), p.onSearchIn(person))}>
                        <TextSearch size={15} /> {t.searchInPerson}
                      </button>
                      <button onClick={() => (setMenu(false), p.onAddText(person))}>
                        <FilePlus2 size={15} /> {t.addText}
                      </button>
                      <button onClick={() => (setMenu(false), p.onMerge(person, similar.map((s) => s.id)))}>
                        <GitMerge size={15} /> {t.mergePersons}
                      </button>
                      <hr />
                      <button className="danger" onClick={() => (setMenu(false), p.onDelete(person))}>
                        <Trash2 size={15} /> {t.deletePerson}
                      </button>
                    </div>
                  )}
                </div>
              </div>
            </div>
          </header>

          {hasFacts ? (
            <dl className="facts">
              {facts
                .filter(([, v]) => v.trim())
                .map(([label, value, linked]) => (
                  <div key={label}>
                    <dt>{label}</dt>
                    <dd>{linked ? names(value) : value}</dd>
                  </div>
                ))}
              {person.notes.trim() && (
                <div className="wide">
                  <dt>{t.notes}</dt>
                  <dd className="notes">{person.notes}</dd>
                </div>
              )}
            </dl>
          ) : (
            <p className="muted small no-facts" onClick={() => p.onEdit(person)}>
              {t.noDetails}
            </p>
          )}

          {single ? (
            <section>
              <h2>
                <BookOpen size={14} /> {t.texts} <span className="pill">1</span>
              </h2>
              <div className="inline-doc">
                <div className="inline-head" data-ctx="doc" data-id={single.id}>
                  <span className="tc-title">{single.title}</span>
                  <CollectionChip name={single.collection} />
                  <span className="kind">{single.kind === "new" ? "" : single.kind.toUpperCase()}</span>
                  <span className="muted small">{formatSize(single.size, lang)}</span>
                  <span className="inline-actions">
                    <button className="btn ghost" onClick={() => p.onOpenDoc(single.id)}>
                      <BookOpen size={15} /> {t.openInReader}
                    </button>
                    <button className="icon-btn" title={t.edit} onClick={() => p.onEditDoc(single.id)}>
                      <Pencil size={16} />
                    </button>
                  </span>
                </div>
                <div
                  className={`reader-body ${p.readerStyle === "editor" ? "src-view" : p.readerStyle}`}
                  style={{ fontSize: p.fontSize }}
                  data-ctx="doc"
                  data-id={single.id}
                  onClick={onInlineClick}
                >
                  {inline === null ? (
                    <span className="spinner" />
                  ) : single.size < 50 ? (
                    <p className="muted">{single.kind === "pdf" ? t.noTextHint : t.noTexts}</p>
                  ) : (
                    inline.sections.map((s) => (
                      <SectionView
                        key={s.id}
                        s={s}
                        terms={NO_TERMS}
                        multi={inline.total > 1}
                        source={p.readerStyle === "editor"}
                        names={p.names}
                        self={person.id}
                      />
                    ))
                  )}
                  {inline && inline.total > inline.sections.length && (
                    <button className="link-btn" onClick={() => p.onOpenDoc(single.id)}>
                      {t.readMore}
                    </button>
                  )}
                </div>
              </div>
              <button className="link-btn add-text-link" onClick={() => p.onAddText(person)}>
                <FilePlus2 size={14} /> {t.addText}
              </button>
            </section>
          ) : (
          <section>
            <h2>
              <BookOpen size={14} /> {t.texts} <span className="pill">{person.docs.length}</span>
            </h2>
            {person.docs.length === 0 && <p className="muted small">{t.noTexts}</p>}
            <div className="text-cards">
              {person.docs.map((d, i) => (
                <button
                  key={d.id}
                  data-ctx="doc"
                  data-id={d.id}
                  className="text-card"
                  style={{ animationDelay: `${Math.min(i, 12) * 30}ms` }}
                  onClick={() => p.onOpenDoc(d.id)}
                >
                  {d.hasCover ? <img src={fileUrl("cover", d.id)} alt="" loading="lazy" /> : <FileText size={22} />}
                  <span className="tc-title">{d.title}</span>
                  <span className="tc-meta">
                    <CollectionChip name={d.collection} />
                    <span className="kind">{d.kind === "new" ? "" : d.kind.toUpperCase()}</span>
                    <span>{formatSize(d.size, lang)}</span>
                  </span>
                </button>
              ))}
              <button className="text-card add" onClick={() => p.onAddText(person)}>
                <FilePlus2 size={22} />
                <span className="tc-title">{t.addText}</span>
              </button>
            </div>
          </section>
          )}

          {similar.length > 0 && (
            <section>
              <h2>
                <GitMerge size={14} /> {t.similar}
              </h2>
              <p className="muted small">{t.similarHint}</p>
              <div className="similar">
                {similar.map((s) => (
                  <button key={s.id} className="chip clickable" onClick={() => p.onPerson(s.id)}>
                    <UserRound size={12} /> {s.name}
                    <span className="n">{s.docs}</span>
                  </button>
                ))}
                <button className="link-btn" onClick={() => p.onMerge(person, similar.map((s) => s.id))}>
                  {t.mergePersons}
                </button>
              </div>
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
