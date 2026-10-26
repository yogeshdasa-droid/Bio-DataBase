import { useEffect, useMemo, useState } from "react";
import { BookOpen, GitMerge, ListTree, SkipForward, Trash2, Users, UserX } from "lucide-react";
import { api, PersonListItem, ReviewDoc } from "../api";
import { formatSize, Lang, T } from "../i18n";
import { Modal } from "./ui";
import { foldName } from "./Sidebar";

const LEADING = new Set(["sri", "srila", "srimati", "srimad", "sripad", "sripada", "om", "visnupad", "jagad", "guru", "his", "divine", "grace"]);
const TRAILING = new Set(["gosvami", "gosvamin", "thakur", "prabhu", "mahasay", "mahasaya"]);

/**
 * Matching key of a name, strict enough not to join different people:
 * honorifics in front and one title at the end go ("Sri Jiva Gosvami" =
 * "Jiva Goswami"), final "a"s too (Thakura = Thakur); "dasa", "acarya",
 * "puri"… stay, they tell people apart.
 */
function dupKey(name: string): string {
  const words = foldName(name)
    .split(" ")
    .filter(Boolean)
    .map((w) => (w.length > 3 && w.endsWith("a") ? w.slice(0, -1) : w));
  while (words.length > 1 && LEADING.has(words[0])) words.shift();
  if (words.length > 1 && TRAILING.has(words[words.length - 1])) words.pop();
  const key = words.join(" ");
  return key.length >= 4 ? key : "";
}

/**
 * Groups of records that may be the same person: the same name (see
 * `dupKey`), also through their other names. Groups the user already called
 * different people are left out.
 */
export function duplicateGroups(persons: PersonListItem[], distinct: [number, number][]): PersonListItem[][] {
  const parent = new Map<number, number>();
  const find = (x: number): number => {
    const p = parent.get(x) ?? x;
    if (p === x) return x;
    const r = find(p);
    parent.set(x, r);
    return r;
  };
  const byKey = new Map<string, number>();
  for (const p of persons) {
    for (const n of [p.name, ...p.aka.split(",")]) {
      const k = dupKey(n);
      if (!k) continue;
      const other = byKey.get(k);
      if (other === undefined) byKey.set(k, p.id);
      else parent.set(find(p.id), find(other));
    }
  }
  const groups = new Map<number, PersonListItem[]>();
  for (const p of persons) {
    const r = find(p.id);
    groups.set(r, [...(groups.get(r) ?? []), p]);
  }
  const apart = new Set(distinct.map(([a, b]) => `${a}-${b}`));
  return [...groups.values()]
    .filter((g) => g.length > 1 && g.some((a, i) => g.slice(i + 1).some((b) => !apart.has(`${Math.min(a.id, b.id)}-${Math.max(a.id, b.id)}`))))
    .sort((a, b) => b.reduce((s, p) => s + p.docs, 0) - a.reduce((s, p) => s + p.docs, 0));
}

/** The assistant: one group at a time, merge / different people / skip. */
export function DuplicatesDialog({
  t,
  persons,
  onClose,
  onChanged,
}: {
  t: T;
  persons: PersonListItem[];
  onClose: () => void;
  onChanged: () => void;
}) {
  const [distinct, setDistinct] = useState<[number, number][] | null>(null);
  const [skipped, setSkipped] = useState<Set<number>>(new Set());
  const [keep, setKeep] = useState<number | null>(null);
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const loadDistinct = () => api.listDistinct().then(setDistinct, (e) => setError(String(e)));
  useEffect(() => void loadDistinct(), []);

  const groups = useMemo(() => (distinct ? duplicateGroups(persons, distinct) : []), [persons, distinct]);
  // skipped groups are known by their smallest id
  const left = groups.filter((g) => !skipped.has(Math.min(...g.map((p) => p.id))));
  const group = left[0];
  const index = groups.length - left.length + 1;

  // each new group proposes the fullest record to stay; nothing else is
  // ticked: two similar names can be two people
  const groupKey = group?.map((p) => p.id).join(",");
  useEffect(() => {
    if (!group) return;
    const best = [...group].sort((a, b) => Number(!!b.tree) - Number(!!a.tree) || b.docs - a.docs)[0];
    setKeep(best.id);
    setPicked(new Set([best.id]));
    setError(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [groupKey]);

  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await f();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const others = group ? group.filter((p) => p.id !== keep && picked.has(p.id)).map((p) => p.id) : [];
  const merge = () =>
    run(async () => {
      await api.mergePersons(keep!, others);
      // what was not ticked is a different person
      for (const p of group) if (!picked.has(p.id)) await api.markDistinct([keep!, p.id]);
      await loadDistinct();
      onChanged();
    });
  const apart = () =>
    run(async () => {
      await api.markDistinct(group.map((p) => p.id));
      await loadDistinct();
    });

  return (
    <Modal
      wide
      title={group ? t.dupDialogTitle(index, groups.length) : t.dupTitle}
      icon={<Users size={18} />}
      onClose={onClose}
      footer={
        group ? (
          <>
            <button className="btn ghost" disabled={busy} onClick={apart}>
              <UserX size={15} /> {t.dupDistinct}
            </button>
            <button className="btn ghost" disabled={busy} onClick={() => setSkipped((s) => new Set(s).add(Math.min(...group.map((p) => p.id))))}>
              <SkipForward size={15} /> {t.dupSkip}
            </button>
            <span className="grow" />
            <button className="btn" disabled={busy || !keep || others.length === 0} onClick={merge}>
              {busy ? <span className="spinner light" /> : <GitMerge size={15} />} {t.mergeButton(others.length)}
            </button>
          </>
        ) : (
          <button className="btn" onClick={onClose}>
            {t.close}
          </button>
        )
      }
    >
      {distinct === null ? (
        <span className="spinner" />
      ) : !group ? (
        <p className="muted">{t.dupDone}</p>
      ) : (
        <>
          <p className="muted small">{t.dupHint}</p>
          <table className="dup-table">
            <tbody>
              {group.map((p) => (
                <tr key={p.id} className={picked.has(p.id) ? "" : "off"}>
                  <td>
                    <input
                      type="checkbox"
                      checked={picked.has(p.id)}
                      onChange={() =>
                        setPicked((s) => {
                          const n = new Set(s);
                          if (n.has(p.id)) n.delete(p.id);
                          else n.add(p.id);
                          return n;
                        })
                      }
                    />
                  </td>
                  <td className="dup-name">
                    <strong>{p.name}</strong>
                    {p.tree && <ListTree size={13} className="dup-tree" aria-label={t.parampara} />}
                    {p.aka && <em>{p.aka}</em>}
                  </td>
                  <td className="muted small">{t.documents(p.docs)}</td>
                  <td>
                    <label className="dup-keep">
                      <input type="radio" name="keep" checked={keep === p.id} disabled={!picked.has(p.id)} onChange={() => setKeep(p.id)} /> {t.dupKeep}
                    </label>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      {error && <div className="error-box">{error}</div>}
    </Modal>
  );
}

/** The "Review" tab in Settings: repeated records, doubtful titles, empty texts. */
export function ReviewPane({
  t,
  lang,
  persons,
  onDuplicates,
  onOpenDoc,
  onChanged,
  onToast,
}: {
  t: T;
  lang: Lang;
  persons: PersonListItem[];
  onDuplicates: () => void;
  onOpenDoc: (id: number) => void;
  onChanged: () => void;
  onToast: (msg: string) => void;
}) {
  const [data, setData] = useState<{ empty: ReviewDoc[]; oddTitles: ReviewDoc[] } | null>(null);
  const [distinct, setDistinct] = useState<[number, number][]>([]);
  const load = () => {
    api.reviewDocs().then(setData, (e) => onToast(String(e)));
    api.listDistinct().then(setDistinct, () => {});
  };
  useEffect(load, []); // eslint-disable-line react-hooks/exhaustive-deps
  const dups = useMemo(() => duplicateGroups(persons, distinct).length, [persons, distinct]);

  const rename = async (d: ReviewDoc, title: string) => {
    if (!title.trim() || title === d.title) return;
    try {
      const { doc } = await api.getDoc(d.id);
      await api.updateDoc(d.id, { title, date: doc.date ?? "", collection: doc.collection, personId: doc.personId });
      onToast(t.renamed);
      onChanged();
      load();
    } catch (e) {
      onToast(String(e));
    }
  };
  const trash = (d: ReviewDoc) =>
    api.trashDoc(d.id).then(
      () => (onToast(t.trashedDone), onChanged(), load()),
      (e) => onToast(String(e)),
    );
  const where = (d: ReviewDoc) => [d.collection, d.person, d.kind.toUpperCase(), formatSize(d.size, lang)].filter(Boolean).join(" · ");

  return (
    <>
      <h3>{t.dupTitle}</h3>
      <p className="muted small">{t.dupCount(dups)}</p>
      <button className="btn" disabled={!dups} onClick={onDuplicates}>
        <Users size={15} /> {t.dupOpen}
      </button>

      <h3>{t.oddTitlesTitle}</h3>
      <p className="muted small">{t.oddTitlesHint}</p>
      {data?.oddTitles.length === 0 && <p className="muted">{t.nothingHere}</p>}
      <div className="lib-list">
        {data?.oddTitles.map((d) => (
          <div key={d.id} className="lib-item">
            <div className="lib-info">
              <input
                className="title-edit"
                defaultValue={d.title}
                onKeyDown={(e) => e.key === "Enter" && rename(d, (e.target as HTMLInputElement).value)}
                onBlur={(e) => rename(d, e.target.value)}
              />
              <code>{where(d)}</code>
            </div>
            <button className="icon-btn" title={t.open} onClick={() => onOpenDoc(d.id)}>
              <BookOpen size={15} />
            </button>
          </div>
        ))}
      </div>

      <h3>{t.emptyDocsTitle}</h3>
      <p className="muted small">{t.emptyDocsHint}</p>
      {data?.empty.length === 0 && <p className="muted">{t.nothingHere}</p>}
      <div className="lib-list">
        {data?.empty.map((d) => (
          <div key={d.id} className="lib-item">
            <div className="lib-info">
              <strong>{d.title}</strong>
              <code>{where(d)}</code>
            </div>
            <button className="icon-btn" title={t.open} onClick={() => onOpenDoc(d.id)}>
              <BookOpen size={15} />
            </button>
            <button className="icon-btn danger" title={t.moveToTrash} onClick={() => trash(d)}>
              <Trash2 size={15} />
            </button>
          </div>
        ))}
      </div>
    </>
  );
}
