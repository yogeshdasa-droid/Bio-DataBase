import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { DatabaseBackup, FolderInput, HelpCircle, RefreshCw, Settings as Gear, Trash2, Undo2 } from "lucide-react";
import { api, DefaultSource, Status, Trashed } from "../api";
import { Lang, T } from "../i18n";
import { Kbd, Modal } from "./ui";
import type { ReaderStyle } from "./Reader";

export type Theme = "system" | "light" | "dark";
export type SettingsTab = "general" | "sources" | "trash" | "help";

interface Props {
  t: T;
  lang: Lang;
  status: Status | null;
  theme: Theme;
  readerStyle: ReaderStyle;
  initialTab: SettingsTab;
  onLang: (l: Lang) => void;
  onTheme: (t: Theme) => void;
  onReaderStyle: (s: ReaderStyle) => void;
  onClose: () => void;
  onToast: (msg: string) => void;
  onChanged: () => void;
  onImportStarted: () => void;
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

  const backup = guard(async () => {
    const day = new Date().toISOString().slice(0, 10);
    const path = await save({ defaultPath: `Bio-Data ${day}.db`, filters: [{ name: "Bio-Data", extensions: ["db"] }] });
    if (!path) return;
    await api.backup(path);
    p.onToast(t.backupDone);
  });

  const reimport = (list: DefaultSource[]) =>
    guard(async () => {
      await api.importSources({ sources: list.filter((s) => s.exists), personPerFile: false, personId: null });
      p.onImportStarted();
    });

  const tabs: [SettingsTab, typeof Gear, string][] = [
    ["general", Gear, t.general],
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
              <h3>{t.readerFont}</h3>
              <div className="seg">
                {(["editor", "serif", "sans"] as ReaderStyle[]).map((st) => (
                  <button key={st} className={`font-${st} ${p.readerStyle === st ? "on" : ""}`} onClick={() => p.onReaderStyle(st)}>
                    {st === "editor" ? t.editorStyle : st === "serif" ? t.serif : t.sans} · Kṛṣṇa uvāca
                  </button>
                ))}
              </div>
              <h3>{t.database}</h3>
              {status && (
                <p className="muted small">
                  {t.dbInfo(status.docs, status.persons, status.filesMb)}
                  <br />
                  <code>{status.dbPath}</code> · {(status.dbSize / 1048576).toFixed(0)} MB
                </p>
              )}
              <div className="btn-row">
                <button className="btn" onClick={backup}>
                  <DatabaseBackup size={15} /> {t.backup}
                </button>
                <button className="btn ghost" disabled={status?.busy} onClick={guard(async () => (await api.rebuildSearch(), p.onImportStarted()))} title={t.rebuildHint}>
                  <RefreshCw size={15} /> {t.rebuild}
                </button>
              </div>
              <p className="muted small">{t.backupHint}</p>
            </>
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
