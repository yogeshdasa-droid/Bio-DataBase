import { useEffect, useMemo, useState } from "react";
import { BookOpen, FilePlus2, FileText, GitMerge, MoreHorizontal, Pencil, Search, TextSearch, Trash2, UserRound } from "lucide-react";
import { api, fileUrl, Person, PersonListItem } from "../api";
import { formatSize, Lang, T } from "../i18n";
import { CollectionChip } from "./ui";
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
  onToast: (msg: string) => void;
}

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

  if (!person || person.id !== p.id) return <div className="reader" />;

  const cover = person.docs.find((d) => d.hasCover);
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
            <div className="person-cover">
              {cover ? <img src={fileUrl("cover", cover.id)} alt="" /> : <UserRound size={44} />}
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
