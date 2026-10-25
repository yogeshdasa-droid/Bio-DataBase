import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, FilePen, FolderInput, GitMerge, Search, Trash2, Upload, UserPlus, UserRound, X } from "lucide-react";
import { api, DocInfo, EMPTY_PERSON, Person, PersonFields, PersonListItem, SectionText } from "../api";
import { T } from "../i18n";
import { Modal } from "./ui";
import { foldName } from "./Sidebar";

function useCtrlS(enabled: boolean, save: () => void) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === "s") {
        e.preventDefault();
        if (enabled) save();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
}

function filterPersons(persons: PersonListItem[], q: string, limit = 40): PersonListItem[] {
  const words = foldName(q).split(" ").filter(Boolean);
  if (!words.length) return [];
  return persons.filter((p) => words.every((w) => ` ${foldName(`${p.name} ${p.aka}`)}`.includes(` ${w}`))).slice(0, limit);
}

// ---------------------------------------------------------------- person picker

/** The person a text belongs to: an existing record, a new one, or none. */
export type PersonChoice = { id: number | null; newName?: string };

function PersonPicker({ t, persons, value, onChange }: { t: T; persons: PersonListItem[]; value: PersonChoice; onChange: (v: PersonChoice) => void }) {
  const current = value.id ? persons.find((p) => p.id === value.id)?.name ?? "" : value.newName ?? "";
  const [q, setQ] = useState(current);
  const [open, setOpen] = useState(false);
  const matches = useMemo(() => filterPersons(persons, q, 12), [persons, q]);
  const exact = matches.some((m) => foldName(m.name) === foldName(q));
  return (
    <div className="picker">
      <input
        value={q}
        placeholder={t.personPlaceholder}
        onFocus={() => setOpen(true)}
        onBlur={() => window.setTimeout(() => setOpen(false), 150)}
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
          if (!e.target.value.trim()) onChange({ id: null });
        }}
      />
      {value.id || value.newName ? (
        <button
          className="icon-btn picker-clear"
          title={t.noPerson}
          onClick={() => {
            setQ("");
            onChange({ id: null });
          }}
        >
          <X size={14} />
        </button>
      ) : null}
      {open && q.trim() && (
        <div className="picker-list">
          {matches.map((m) => (
            <button
              key={m.id}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                setQ(m.name);
                setOpen(false);
                onChange({ id: m.id });
              }}
            >
              <UserRound size={13} /> {m.name}
              {m.aka && <em>{m.aka}</em>}
            </button>
          ))}
          {!exact && (
            <button
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                setOpen(false);
                onChange({ id: null, newName: q.trim() });
              }}
            >
              <UserPlus size={13} /> {t.personNew(q.trim())}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

async function resolveChoice(c: PersonChoice): Promise<number | null> {
  if (c.id) return c.id;
  if (c.newName) return api.newPerson({ ...EMPTY_PERSON, name: c.newName });
  return null;
}

// ---------------------------------------------------------------- edit text

export function EditDialog({
  t,
  doc,
  section,
  persons,
  onClose,
  onSaved,
}: {
  t: T;
  doc: DocInfo;
  section: SectionText;
  persons: PersonListItem[];
  onClose: () => void;
  onSaved: (docId: number, ordinal: number) => void;
}) {
  const [title, setTitle] = useState(doc.title);
  const [collection, setCollection] = useState(doc.collection);
  const [person, setPerson] = useState<PersonChoice>({ id: doc.personId });
  const [text, setText] = useState(section.text);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const metaDirty = title !== doc.title || collection !== doc.collection || person.id !== doc.personId || !!person.newName;
  const dirty = metaDirty || text !== section.text;

  const close = () => {
    if (!dirty || window.confirm(t.unsaved)) onClose();
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      if (text !== section.text) await api.saveSection(doc.id, section.ordinal, text);
      if (metaDirty) {
        const personId = await resolveChoice(person);
        await api.updateDoc(doc.id, { title, date: doc.date ?? "", collection, personId });
      }
      onSaved(doc.id, section.ordinal);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };
  useCtrlS(dirty && !busy, save);

  return (
    <Modal
      wide
      title={t.editTitle}
      icon={<FilePen size={18} />}
      onClose={close}
      footer={
        <>
          <span className="muted small grow">
            <AlertTriangle size={13} /> {t.editWarning}
          </span>
          <button className="btn ghost" onClick={close}>
            {t.cancel}
          </button>
          <button className="btn" disabled={!dirty || busy} onClick={save}>
            {busy ? <span className="spinner light" /> : null} {t.save} <kbd className="kbd on-accent">Ctrl S</kbd>
          </button>
        </>
      }
    >
      <div className="form-grid three">
        <label>
          <span>{t.titleLabel}</span>
          <input value={title} onChange={(e) => setTitle(e.target.value)} />
        </label>
        <label>
          <span>{t.personLabel}</span>
          <PersonPicker t={t} persons={persons} value={person} onChange={setPerson} />
        </label>
        <label>
          <span>{t.collectionLabel}</span>
          <input value={collection} onChange={(e) => setCollection(e.target.value)} />
        </label>
      </div>
      <label className="text-label">
        <span>
          {t.text}
          {doc.sections > 1 && <em> · {t.editSection(section.ordinal + 1, doc.sections)}</em>}
        </span>
        <textarea className="editor" value={text} spellCheck={false} onChange={(e) => setText(e.target.value)} autoFocus={!doc.size} />
      </label>
      {error && <div className="error-box">{error}</div>}
    </Modal>
  );
}

// ---------------------------------------------------------------- recycle bin

export function TrashDialog({ t, doc, onClose, onDone }: { t: T; doc: DocInfo; onClose: () => void; onDone: () => void }) {
  const [error, setError] = useState<string | null>(null);
  return (
    <Modal
      title={t.moveToTrash}
      icon={<Trash2 size={18} />}
      onClose={onClose}
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            {t.cancel}
          </button>
          <button
            className="btn"
            autoFocus
            onClick={() =>
              api
                .trashDoc(doc.id)
                .then(onDone)
                .catch((e) => setError(String(e)))
            }
          >
            <Trash2 size={15} /> {t.moveToTrash}
          </button>
        </>
      }
    >
      <p>{t.trashQuestion(doc.title)}</p>
      <p className="muted small">{t.trashDesc}</p>
      {error && <div className="error-box">{error}</div>}
    </Modal>
  );
}

// ---------------------------------------------------------------- import

export function ImportDialog({
  t,
  paths,
  person,
  onClose,
  onStarted,
}: {
  t: T;
  paths: string[];
  /** add everything to this record */
  person?: { id: number; name: string };
  onClose: () => void;
  onStarted: () => void;
}) {
  const [kinds, setKinds] = useState<("dir" | "text" | "other")[] | null>(null);
  const [collection, setCollection] = useState("");
  const [perFile, setPerFile] = useState(false);
  const [toPerson, setToPerson] = useState(!!person);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.pathKinds(paths).then(setKinds).catch((e) => setError(String(e)));
  }, [paths]);

  const counts = useMemo(() => {
    const k = kinds ?? [];
    return { files: k.filter((x) => x === "text").length, dirs: k.filter((x) => x === "dir").length };
  }, [kinds]);
  const usable = paths.filter((_, i) => kinds && kinds[i] !== "other");

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.importSources({
        sources: usable.map((path) => ({ path, collection })),
        personPerFile: perFile && !toPerson,
        personId: toPerson && person ? person.id : null,
      });
      onStarted();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const nothing = kinds !== null && usable.length === 0;
  return (
    <Modal
      title={t.importTitle}
      icon={<Upload size={18} />}
      onClose={onClose}
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            {t.cancel}
          </button>
          <button className="btn" disabled={busy || nothing || kinds === null} onClick={run}>
            {busy && <span className="spinner light" />} {t.importButton}
          </button>
        </>
      }
    >
      <div className="import-summary">
        <FolderInput size={28} />
        <div>
          <strong>{t.importSummary(counts.files, counts.dirs) || "…"}</strong>
          <div className="paths">
            {paths.slice(0, 6).map((p) => (
              <div key={p} title={p}>
                {p}
              </div>
            ))}
            {paths.length > 6 && <div>… +{paths.length - 6}</div>}
          </div>
        </div>
      </div>
      {nothing ? (
        <div className="error-box">{t.nothingToImport}</div>
      ) : (
        <>
          <div className="form-grid">
            <label>
              <span>{t.collectionName}</span>
              <input value={collection} placeholder={t.collectionHint} onChange={(e) => setCollection(e.target.value)} />
            </label>
          </div>
          {person && (
            <label className="check block">
              <input type="checkbox" checked={toPerson} onChange={(e) => setToPerson(e.target.checked)} />
              <span>
                <UserRound size={14} /> {t.importIntoPerson(person.name)}
              </span>
            </label>
          )}
          {!toPerson && (
            <label className="check block">
              <input type="checkbox" checked={perFile} onChange={(e) => setPerFile(e.target.checked)} />
              <span>
                <UserPlus size={14} /> {t.personPerFile}
                <em className="muted small">{t.personPerFileHint}</em>
              </span>
            </label>
          )}
        </>
      )}
      {error && <div className="error-box">{error}</div>}
    </Modal>
  );
}

// ---------------------------------------------------------------- person record

export function PersonDialog({
  t,
  person,
  onClose,
  onSaved,
}: {
  t: T;
  /** null = new record */
  person: Person | null;
  onClose: () => void;
  onSaved: (id: number) => void;
}) {
  const initial: PersonFields = person ?? EMPTY_PERSON;
  const [f, setF] = useState<PersonFields>({ ...EMPTY_PERSON, ...initial });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const keys = Object.keys(EMPTY_PERSON) as (keyof PersonFields)[];
  const dirty = keys.some((k) => f[k] !== (initial[k] ?? ""));
  const set = (k: keyof PersonFields) => (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => setF({ ...f, [k]: e.target.value });

  const close = () => {
    if (!dirty || window.confirm(t.unsaved)) onClose();
  };
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      if (person) {
        await api.updatePerson(person.id, f);
        onSaved(person.id);
      } else onSaved(await api.newPerson(f));
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };
  useCtrlS(dirty && !busy && !!f.name.trim(), save);

  const field = (k: keyof PersonFields, label: string, hint?: string) => (
    <label>
      <span>{label}</span>
      <input value={f[k]} placeholder={hint} onChange={set(k)} />
    </label>
  );

  return (
    <Modal
      wide
      title={person ? t.editPerson : t.newPerson.replace("…", "")}
      icon={<UserRound size={18} />}
      onClose={close}
      footer={
        <>
          <span className="grow" />
          <button className="btn ghost" onClick={close}>
            {t.cancel}
          </button>
          <button className="btn" disabled={!dirty || busy || !f.name.trim()} onClick={save}>
            {busy ? <span className="spinner light" /> : null} {t.save} <kbd className="kbd on-accent">Ctrl S</kbd>
          </button>
        </>
      }
    >
      <div className="form-grid">
        <label>
          <span>{t.name}</span>
          <input value={f.name} onChange={set("name")} autoFocus={!person} />
        </label>
        {field("aka", t.aka, t.akaHint)}
        {field("appearance", t.appearance)}
        {field("disappearance", t.disappearance)}
        {field("place", t.place)}
        {field("guru", t.guru, t.listHint)}
      </div>
      <div className="form-grid one">
        {field("disciples", t.disciples, t.listHint)}
        {field("works", t.works, t.listHint)}
        <label>
          <span>{t.notes}</span>
          <textarea className="notes-editor" value={f.notes} onChange={set("notes")} />
        </label>
      </div>
      {error && <div className="error-box">{error}</div>}
    </Modal>
  );
}

// ---------------------------------------------------------------- merge

export function MergeDialog({
  t,
  person,
  persons,
  suggested,
  onClose,
  onDone,
}: {
  t: T;
  person: Person;
  persons: PersonListItem[];
  suggested: number[];
  onClose: () => void;
  onDone: () => void;
}) {
  const [q, setQ] = useState("");
  // Suggestions are listed, never ticked: two similar names can be two people.
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const others = persons.filter((p) => p.id !== person.id);
  const shown = useMemo(() => {
    const found = q.trim() ? filterPersons(others, q, 60) : others.filter((p) => suggested.includes(p.id));
    const pickedRows = others.filter((p) => picked.has(p.id) && !found.includes(p));
    return [...pickedRows, ...found];
  }, [q, others, picked, suggested]);
  const toggle = (id: number) =>
    setPicked((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const run = async () => {
    setBusy(true);
    try {
      await api.mergePersons(person.id, Array.from(picked));
      onDone();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t.mergeTitle(person.name)}
      icon={<GitMerge size={18} />}
      onClose={onClose}
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            {t.cancel}
          </button>
          <button className="btn" disabled={busy || picked.size === 0} onClick={run}>
            {busy && <span className="spinner light" />} <GitMerge size={15} /> {t.mergeButton(picked.size)}
          </button>
        </>
      }
    >
      <p className="muted small">{t.mergeHint}</p>
      <div className="person-filter boxed">
        <Search size={14} />
        <input autoFocus value={q} placeholder={t.mergeSearch} onChange={(e) => setQ(e.target.value)} />
      </div>
      <div className="merge-list">
        {shown.map((p) => (
          <label key={p.id} className="check block">
            <input type="checkbox" checked={picked.has(p.id)} onChange={() => toggle(p.id)} />
            <span>
              {p.name} <em className="muted small">{t.documents(p.docs)}</em>
              {p.aka && <em className="muted small">{p.aka}</em>}
            </span>
          </label>
        ))}
      </div>
      {error && <div className="error-box">{error}</div>}
    </Modal>
  );
}
