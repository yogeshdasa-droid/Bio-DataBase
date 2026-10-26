import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { ArchiveRestore, CalendarDays, ClipboardCheck, Database, FileDown, DatabaseBackup, FolderInput, FolderOpen, HelpCircle, RefreshCw, Settings as Gear, Trash2, Undo2 } from "lucide-react";
import { api, BackupCopy, BackupSettings, CalendarInfo, DefaultSource, PersonListItem, Status, Trashed } from "../api";
import { ReviewPane } from "./Review";
import { checkUpdate } from "../update";
import { getVersion } from "@tauri-apps/api/app";
import { Lang, T } from "../i18n";
import { Kbd, Modal } from "./ui";
import type { ReaderStyle } from "./Reader";

export type Theme = "system" | "light" | "dark";
export type Skin = "vscode" | "classic";
export type SettingsTab = "general" | "backups" | "review" | "sources" | "trash" | "help";

const when = (secs: number, lang: Lang) =>
  new Date(secs * 1000).toLocaleString(lang === "es" ? "es-ES" : "en-GB", { dateStyle: "medium", timeStyle: "short" });

/** This program's version and a check for a newer one. */
function UpdateCheck({ t, onToast }: { t: T; onToast: (msg: string) => void }) {
  const [version, setVersion] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => void getVersion().then(setVersion), []);
  const check = async () => {
    setBusy(true);
    try {
      const u = await checkUpdate();
      if (!u) onToast(t.updateNone);
      else if (window.confirm(`${t.updateAvailable(u.version)}

${t.updateHint}

${t.updateDownload}?`)) await api.openExternal(u.url);
    } catch (e) {
      onToast(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="btn-row">
      <span className="muted">{t.versionLabel(version)}</span>
      <button className="btn ghost" disabled={busy} onClick={check}>
        {busy ? <span className="spinner" /> : <RefreshCw size={15} />} {t.checkUpdates}
      </button>
    </div>
  );
}

/** The imported Vaiṣṇava calendar (.ics). */
function CalendarSettings({ t, onToast, onChanged }: { t: T; onToast: (msg: string) => void; onChanged: () => void }) {
  const [info, setInfo] = useState<CalendarInfo | null>(null);
  const load = () => api.calendarInfo().then(setInfo, (e) => onToast(String(e)));
  useEffect(() => void load(), []); // eslint-disable-line react-hooks/exhaustive-deps
  const pick = async () => {
    try {
      const path = await open({ multiple: false, filters: [{ name: "iCalendar", extensions: ["ics"] }] });
      if (typeof path !== "string") return;
      onToast(t.calendarDone(await api.importCalendar(path)));
      load();
      onChanged();
    } catch (e) {
      onToast(String(e));
    }
  };
  const clear = async () => {
    if (!window.confirm(t.calendarClearConfirm)) return;
    await api.clearCalendar().catch((e) => onToast(String(e)));
    load();
    onChanged();
  };
  return (
    <>
      <h3>{t.calendarTitle}</h3>
      <p className="muted small">
        {info && info.count > 0 && info.from && info.to ? t.calendarRange(info.count, info.from, info.to) : t.calendarNone} {t.calendarImportHint}
      </p>
      <div className="btn-row">
        <button className="btn ghost" onClick={pick}>
          <CalendarDays size={15} /> {t.calendarImport}
        </button>
        {info && info.count > 0 && (
          <button className="btn ghost" onClick={clear}>
            <Trash2 size={15} /> {t.calendarClear}
          </button>
        )}
      </div>
      {info && info.unmatched.length > 0 && (
        <details className="unmatched">
          <summary className="muted small">{t.calendarUnmatched(info.unmatched.length)}</summary>
          <ul>
            {info.unmatched.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </details>
      )}
    </>
  );
}

/** Automatic copies, the list of saved copies, and going back to one. */
function Backups({ t, lang, onToast }: { t: T; lang: Lang; onToast: (msg: string) => void }) {
  const [settings, setSettings] = useState<BackupSettings | null>(null);
  const [copies, setCopies] = useState<{ dir: string; copies: BackupCopy[] } | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () => {
    api.backupSettings().then(setSettings).catch((e) => onToast(String(e)));
    api.listCopies().then(setCopies).catch((e) => onToast(String(e)));
  };
  useEffect(load, []); // eslint-disable-line react-hooks/exhaustive-deps

  const run = (f: () => Promise<unknown>) => async () => {
    setBusy(true);
    try {
      await f();
    } catch (e) {
      onToast(String(e));
    } finally {
      setBusy(false);
      load();
    }
  };
  const change = (days: number, keep: number) =>
    api.setBackupSettings(days, keep).then(() => setSettings((s) => s && { ...s, days, keep }), (e) => onToast(String(e)));

  const restore = async (path: string, name: string) => {
    if (!window.confirm(t.restoreConfirm(name))) return;
    await api.restoreCopy(path);
    // everything on screen belongs to the old data: start again
    window.alert(t.restoredCopy);
    window.location.reload();
  };
  const saveAs = async () => {
    const day = new Date().toISOString().slice(0, 10);
    const path = await save({ defaultPath: `Gaudiya Vaisnavas DB ${day}.db`, filters: [{ name: "Gaudiya Vaisnavas DB", extensions: ["db"] }] });
    if (!path) return;
    await api.backup(path);
    onToast(t.backupDone);
  };
  const fromFile = async () => {
    const path = await open({ multiple: false, filters: [{ name: "Gaudiya Vaisnavas DB", extensions: ["db"] }] });
    if (typeof path === "string") await restore(path, path.split(/[\\/]/).pop() ?? path);
  };

  return (
    <>
      <h3>{t.autoBackup}</h3>
      {settings && (
        <>
          <div className="seg">
            {t.backupEvery.map(([d, label]) => (
              <button key={d} className={settings.days === d ? "on" : ""} onClick={() => change(d, settings.keep)}>
                {label}
              </button>
            ))}
          </div>
          {settings.days > 0 && (
            <label className="inline-field">
              {t.backupKeep}
              <select value={settings.keep} onChange={(e) => change(settings.days, Number(e.target.value))}>
                {[2, 3, 5, 10].map((n) => (
                  <option key={n} value={n}>
                    {t.backupKeepN(n)}
                  </option>
                ))}
              </select>
            </label>
          )}
          <p className="muted small">
            {settings.last ? t.backupLast(when(settings.last, lang)) : t.backupNone} {t.backupAutoHint}
          </p>
        </>
      )}
      <div className="btn-row">
        <button className="btn" disabled={busy} onClick={run(async () => (await api.backupNow(), onToast(t.backupDone)))}>
          {busy ? <span className="spinner light" /> : <DatabaseBackup size={15} />} {t.backupNow}
        </button>
        <button className="btn ghost" disabled={busy} onClick={run(saveAs)}>
          <DatabaseBackup size={15} /> {t.backupSaveAs}
        </button>
        {copies && (
          <button className="btn ghost" onClick={() => api.openExternal(copies.dir).catch(() => onToast(t.noCopies))}>
            <FolderOpen size={15} /> {t.openCopiesFolder}
          </button>
        )}
      </div>

      <h3>{t.copiesTitle}</h3>
      {copies && copies.copies.length === 0 && <p className="muted">{t.noCopies}</p>}
      <div className="lib-list">
        {copies?.copies.map((c) => (
          <div key={c.path} className="lib-item">
            <Database size={18} />
            <div className="lib-info">
              <strong>{c.name}</strong>
              <code title={c.path}>
                {when(c.modified, lang)} · {(c.size / 1048576).toFixed(0)} MB{c.auto ? ` · ${t.copyAuto}` : ""}
              </code>
            </div>
            <button className="btn ghost" disabled={busy} onClick={run(() => restore(c.path, c.name))}>
              <ArchiveRestore size={15} /> {t.restoreCopy}
            </button>
            <button
              className="icon-btn danger"
              title={t.delete}
              disabled={busy}
              onClick={run(async () => {
                if (window.confirm(t.deleteCopyConfirm(c.name))) await api.deleteCopy(c.path);
              })}
            >
              <Trash2 size={15} />
            </button>
          </div>
        ))}
      </div>
      <button className="btn ghost" disabled={busy} onClick={run(fromFile)}>
        <ArchiveRestore size={15} /> {t.restoreFromFile}
      </button>
      {busy && <p className="muted small">{t.working}</p>}
    </>
  );
}

interface Props {
  t: T;
  lang: Lang;
  status: Status | null;
  theme: Theme;
  skin: Skin;
  readerStyle: ReaderStyle;
  initialTab: SettingsTab;
  onLang: (l: Lang) => void;
  onTheme: (t: Theme) => void;
  onSkin: (s: Skin) => void;
  onReaderStyle: (s: ReaderStyle) => void;
  onClose: () => void;
  onToast: (msg: string) => void;
  onChanged: () => void;
  onImportStarted: () => void;
  persons: PersonListItem[];
  onDuplicates: () => void;
  onOpenDoc: (id: number) => void;
}

export function Settings(p: Props) {
  const { t, lang, status } = p;
  const [tab, setTab] = useState<SettingsTab>(p.initialTab);
  const [trash, setTrash] = useState<Trashed[]>([]);
  const [sources, setSources] = useState<DefaultSource[]>([]);

  useEffect(() => {
    if (tab === "trash") api.listTrash().then(setTrash).catch((e) => p.onToast(String(e)));
    if (tab === "sources") api.defaultSources().then(setSources).catch((e) => p.onToast(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  const guard = (f: () => Promise<unknown>) => () => f().catch((e) => p.onToast(String(e)));

  const reimport = (list: DefaultSource[]) =>
    guard(async () => {
      await api.importSources({ sources: list.filter((s) => s.exists), personPerFile: false, personId: null });
      p.onImportStarted();
    });

  const tabs: [SettingsTab, typeof Gear, string][] = [
    ["general", Gear, t.general],
    ["backups", DatabaseBackup, t.backups],
    ["review", ClipboardCheck, t.review],
    ["sources", FolderInput, t.sourcesTitle],
    ["trash", Trash2, `${t.trash}${status?.trash ? ` (${status.trash})` : ""}`],
    ["help", HelpCircle, t.help],
  ];

  return (
    <Modal wide title={t.settings} icon={<Gear size={18} />} onClose={p.onClose}>
      <div className="settings">
        <nav className="settings-nav">
          {tabs.map(([k, I, label]) => (
            <button key={k} className={tab === k ? "active" : ""} onClick={() => setTab(k)}>
              <I size={16} /> {label}
            </button>
          ))}
        </nav>

        <div className="settings-pane fade-in" key={tab}>
          {tab === "general" && (
            <>
              <h3>{t.language}</h3>
              <div className="seg">
                <button className={lang === "es" ? "on" : ""} onClick={() => p.onLang("es")}>
                  Español
                </button>
                <button className={lang === "en" ? "on" : ""} onClick={() => p.onLang("en")}>
                  English
                </button>
              </div>
              <h3>{t.theme}</h3>
              <div className="seg">
                {(["system", "light", "dark"] as Theme[]).map((th) => (
                  <button key={th} className={p.theme === th ? "on" : ""} onClick={() => p.onTheme(th)}>
                    {th === "system" ? t.themeSystem : th === "light" ? t.themeLight : t.themeDark}
                  </button>
                ))}
              </div>
              <h3>{t.skin}</h3>
              <div className="seg">
                {(["vscode", "classic"] as Skin[]).map((s) => (
                  <button key={s} className={p.skin === s ? "on" : ""} onClick={() => p.onSkin(s)}>
                    {s === "vscode" ? t.skinVscode : t.skinClassic}
                  </button>
                ))}
              </div>
              <h3>{t.readerFont}</h3>
              <div className="seg">
                {(["editor", "serif", "sans"] as ReaderStyle[]).map((st) => (
                  <button key={st} className={`font-${st} ${p.readerStyle === st ? "on" : ""}`} onClick={() => p.onReaderStyle(st)}>
                    {st === "editor" ? t.editorStyle : st === "serif" ? t.serif : t.sans} · Kṛṣṇa uvāca
                  </button>
                ))}
              </div>
              <CalendarSettings t={t} onToast={p.onToast} onChanged={p.onChanged} />
              <h3>{t.database}</h3>
              {status && (
                <p className="muted small">
                  {t.dbInfo(status.docs, status.persons, status.filesMb)}
                  <br />
                  <code>{status.dbPath}</code> · {(status.dbSize / 1048576).toFixed(0)} MB
                </p>
              )}
              <div className="btn-row">
                <button className="btn" onClick={() => setTab("backups")}>
                  <DatabaseBackup size={15} /> {t.backups}
                </button>
                <button className="btn ghost" disabled={status?.busy} onClick={guard(async () => (await api.rebuildSearch(), p.onImportStarted()))} title={t.rebuildHint}>
                  <RefreshCw size={15} /> {t.rebuild}
                </button>
              </div>
              <p className="muted small">{t.backupHint}</p>
              <div className="btn-row">
                <button
                  className="btn ghost"
                  disabled={status?.busy}
                  title={t.exportAllHint}
                  onClick={guard(async () => {
                    const dir = await open({ directory: true });
                    if (typeof dir !== "string") return;
                    await api.exportMarkdown(dir);
                    p.onImportStarted();
                  })}
                >
                  <FileDown size={15} /> {t.exportAll}
                </button>
              </div>
              <p className="muted small">{t.exportAllHint}</p>
            </>
          )}

          {tab === "backups" && <Backups t={t} lang={lang} onToast={p.onToast} />}

          {tab === "review" && (
            <ReviewPane
              t={t}
              lang={lang}
              persons={p.persons}
              onDuplicates={p.onDuplicates}
              onOpenDoc={p.onOpenDoc}
              onChanged={p.onChanged}
              onToast={p.onToast}
            />
          )}

          {tab === "sources" && (
            <>
              <p className="muted small">{t.sourcesHint}</p>
              <div className="lib-list">
                {sources.map((s) => (
                  <div key={s.path} className={`lib-item ${s.exists ? "" : "stale"}`}>
                    <FolderInput size={20} />
                    <div className="lib-info">
                      <strong>{s.collection}</strong>
                      <code title={s.path}>{s.path}</code>
                      {!s.exists && <span className="muted small">{t.missing}</span>}
                    </div>
                    <button className="btn ghost" disabled={!s.exists || status?.busy} onClick={reimport([s])}>
                      <RefreshCw size={15} /> {t.reimport}
                    </button>
                  </div>
                ))}
              </div>
              <button className="btn" disabled={status?.busy || !sources.some((s) => s.exists)} onClick={reimport(sources)}>
                <RefreshCw size={15} /> {t.reimport}
              </button>
            </>
          )}

          {tab === "trash" &&
            (trash.length === 0 ? (
              <p className="muted">{t.noTrash}</p>
            ) : (
              <>
                <div className="lib-list">
                  {trash.map((x) => (
                    <div key={x.id} className="lib-item">
                      <Trash2 size={18} />
                      <div className="lib-info">
                        <strong>{x.title}</strong>
                        <code>
                          {x.collection}
                          {x.person ? ` · ${x.person}` : ""}
                        </code>
                        <span className="muted small">{x.deleted}</span>
                      </div>
                      <button
                        className="btn ghost"
                        onClick={guard(async () => {
                          await api.restoreDoc(x.id);
                          setTrash((list) => list.filter((y) => y !== x));
                          p.onToast(t.restored);
                          p.onChanged();
                        })}
                      >
                        <Undo2 size={15} /> {t.restore}
                      </button>
                      <button
                        className="icon-btn danger"
                        title={t.purge}
                        onClick={guard(async () => {
                          if (!window.confirm(t.purgeConfirm(x.title))) return;
                          await api.purgeDocs([x.id]);
                          setTrash((list) => list.filter((y) => y !== x));
                          p.onChanged();
                        })}
                      >
                        <Trash2 size={15} />
                      </button>
                    </div>
                  ))}
                </div>
                <button
                  className="btn ghost"
                  onClick={guard(async () => {
                    if (!window.confirm(t.emptyTrashConfirm(trash.length))) return;
                    await api.purgeDocs(trash.map((x) => x.id));
                    setTrash([]);
                    p.onChanged();
                  })}
                >
                  <Trash2 size={15} /> {t.emptyTrash}
                </button>
              </>
            ))}

          {tab === "help" && (
            <>
              <UpdateCheck t={t} onToast={p.onToast} />
              <h3>{t.syntax}</h3>
              <table className="syntax">
                <tbody>
                  {t.syntaxRows.map(([q, d]) => (
                    <tr key={q}>
                      <td>
                        <code>{q}</code>
                      </td>
                      <td>{d}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <h3>{t.shortcutsTitle}</h3>
              <table className="syntax">
                <tbody>
                  {t.shortcuts.map(([k, d]) => (
                    <tr key={k}>
                      <td>
                        <Kbd>{k}</Kbd>
                      </td>
                      <td>{d}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
        </div>
      </div>
    </Modal>
  );
}
