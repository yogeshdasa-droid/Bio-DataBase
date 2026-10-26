import { useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { BookOpen, FileDown, FilePlus2, Printer, FileText, GitMerge, History, ImagePlus, Images, MoreHorizontal, Pencil, Search, TextSearch, Trash2, UserRound, X } from "lucide-react";
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
  onVersions: (p: Person) => void;
  onTag: (tag: string) => void;
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
  const [zoom, setZoom] = useState<number | null>(null);

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

  // folded name → id, to turn names written in the fields into links; the
  // name without titles ("Rupa" = "Rupa Gosvami") when only one record has it
  const byName = useMemo(() => {
    const m = new Map<string, number>();
    const cores = new Map<string, number | null>();
    for (const x of p.persons) {
      for (const n of [x.name, ...x.aka.split(",")].map((s) => s.trim()).filter(Boolean)) {
        m.set(foldName(n), x.id);
        const c = coreName(n);
        if (c) cores.set(c, cores.has(c) && cores.get(c) !== x.id ? null : x.id);
      }
    }
    return (name: string) => m.get(foldName(name)) ?? cores.get(coreName(name)) ?? undefined;
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
  // guru and disciples: the names written in the record, plus those of the Guru Paramparā
  const me = p.persons.find((x) => x.id === person.id);
  const treeGuru = p.persons.filter((x) => me?.tree?.parent != null && x.id === me.tree.parent);
  const treeDisciples = p.persons.filter((x) => x.tree?.parent === person.id).sort((a, b) => a.tree!.pos - b.tree!.pos);
  type Name = { text: string; id?: number; tree?: boolean };
  // "Rupa Gosvami (diksa); studied under…" links to Rupa Gosvami
  const nameKey = (s: string) => s.replace(/\([^)]*\)/g, "").split(/[;:]/)[0].trim();
  const nameList = (value: string, fromTree: PersonListItem[]): Name[] => {
    const out: Name[] = value
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean)
      .map((text) => ({ text, id: byName(nameKey(text)) }));
    // the Guru Paramparā only fills in what the record does not say
    if (!out.length) for (const x of fromTree) out.push({ text: x.name, id: x.id, tree: true });
    return out;
  };
  const names = (list: Name[]) =>
    list.map((n, i) =>
      n.id && n.id !== person.id ? (
        <button key={i} className={`name-link${n.tree ? " from-tree" : ""}`} title={n.tree ? t.fromParampara : undefined} onClick={() => p.onPerson(n.id!)}>
          {n.text}
        </button>
      ) : (
        <span key={i} className="name-plain">
          {n.text}
        </span>
      ),
    );
  const dated = (value: string, lunar: string | null) => (
    <>
      {value}
      {lunar && <span className="lunar">{lunar}</span>}
    </>
  );
  const gurus = nameList(person.guru, treeGuru);
  const disciples = nameList(person.disciples, treeDisciples);

  const facts: [string, React.ReactNode, boolean][] = [
    [t.appearance, dated(person.appearance, person.appearanceDay), !!person.appearance.trim()],
    [t.disappearance, dated(person.disappearance, person.disappearanceDay), !!person.disappearance.trim()],
    [t.place, person.place, !!person.place.trim()],
    [t.guru, names(gurus), gurus.length > 0],
    [t.disciples, names(disciples), disciples.length > 0],
    [t.works, person.works, !!person.works.trim()],
  ];
  const hasFacts = facts.some(([, , show]) => show) || person.notes.trim();
  const tags = person.tags
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

  const addImages = async () => {
    try {
      const picked = await open({ multiple: true, filters: [{ name: t.imageFiles, extensions: ["jpg", "jpeg", "png", "webp", "gif", "bmp", "avif"] }] });
      const paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
      if (!paths.length) return;
      await api.addPersonImages(person.id, paths);
      p.onPhotoChanged();
    } catch (e) {
      p.onToast(String(e));
    }
  };
  const exportMe = async () => {
    try {
      const path = await save({ defaultPath: `${person.name.replace(/[\\/:*?"<>|]/g, "-")}.md`, filters: [{ name: "Markdown", extensions: ["md"] }] });
      if (!path) return;
      await api.exportPerson(person.id, path);
      p.onToast(t.exported);
    } catch (e) {
      p.onToast(String(e));
    }
  };
  const imageAction = (f: () => Promise<unknown>) => () => f().then(p.onPhotoChanged, (e) => p.onToast(String(e)));

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
              {tags.length > 0 && (
                <div className="tags">
                  {tags.map((tg) => (
                    <button key={tg} className="tag clickable" title={t.byTag(tg)} onClick={() => p.onTag(tg)}>
                      #{tg}
                    </button>
                  ))}
                </div>
              )}
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
                      <button onClick={() => (setMenu(false), p.onVersions(person))}>
                        <History size={15} /> {t.versions}
                      </button>
                      <button onClick={() => (setMenu(false), exportMe())}>
                        <FileDown size={15} /> {t.exportPerson}
                      </button>
                      <button onClick={() => (setMenu(false), window.setTimeout(() => window.print(), 50))}>
                        <Printer size={15} /> {t.printPerson}
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
                .filter(([, , show]) => show)
                .map(([label, value]) => (
                  <div key={label}>
                    <dt>{label}</dt>
                    <dd>{value}</dd>
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

          <section>
            <h2>
              <Images size={14} /> {t.gallery} {person.images.length > 0 && <span className="pill">{person.images.length}</span>}
            </h2>
            <div className="gallery">
              {person.images.map((id) => (
                <figure key={id}>
                  <a href={fileUrl("image", id)} target="_blank" rel="noreferrer" onClick={(e) => (e.preventDefault(), setZoom(id))}>
                    <img src={fileUrl("image", id)} alt="" loading="lazy" />
                  </a>
                  <figcaption>
                    <button className="icon-btn" title={t.useAsMain} onClick={imageAction(() => api.imageAsPhoto(id))}>
                      <UserRound size={14} />
                    </button>
                    <button
                      className="icon-btn danger"
                      title={t.removeImage}
                      onClick={() => window.confirm(t.removeImageConfirm) && imageAction(() => api.removePersonImage(id))()}
                    >
                      <Trash2 size={14} />
                    </button>
                  </figcaption>
                </figure>
              ))}
              <button className="gallery-add" onClick={addImages}>
                <ImagePlus size={22} />
                <span>{t.addImages}</span>
              </button>
            </div>
          </section>

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
      {zoom !== null && (
        <div className="zoom" onClick={() => setZoom(null)}>
          <img src={fileUrl("image", zoom)} alt="" />
        </div>
      )}
    </div>
  );
}
