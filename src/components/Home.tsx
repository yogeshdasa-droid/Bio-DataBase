import { useMemo } from "react";
import { Clock, Database, FileText, HardDrive, Star, Upload, UserRound, Users } from "lucide-react";
import { Bookmark, fileUrl, hueOf, PersonListItem, SearchResult, Status } from "../api";
import { Lang, T } from "../i18n";
import { AnimatedNumber } from "./ui";

export interface RecentDoc {
  docId: number;
  title: string;
  collection: string;
}

interface Props {
  t: T;
  lang: Lang;
  status: Status | null;
  facets: SearchResult["facets"];
  persons: PersonListItem[];
  recent: RecentDoc[];
  bookmarks: Bookmark[];
  onCollection: (name: string) => void;
  onPerson: (id: number) => void;
  onRecent: (r: RecentDoc) => void;
  onBookmark: (b: Bookmark) => void;
  onImportDefaults: () => void;
}

export function Home({ t, lang, status, facets, persons, recent, bookmarks, onCollection, onPerson, onRecent, onBookmark, onImportDefaults }: Props) {
  const max = Math.max(1, ...facets.map((f) => f.count));
  const top = useMemo(() => [...persons].sort((a, b) => b.docs - a.docs).slice(0, 12), [persons]);
  const stats = [
    { icon: Users, value: status?.persons ?? 0, label: t.statPersons },
    { icon: FileText, value: status?.docs ?? 0, label: t.statDocs },
    { icon: HardDrive, value: status?.filesMb ?? 0, label: t.statFiles, fmt: (n: number) => `${Math.round(n).toLocaleString(lang)} MB` },
    { icon: Database, value: (status?.dbSize ?? 0) / 1048576, label: t.statDb, fmt: (n: number) => `${Math.round(n).toLocaleString(lang)} MB` },
  ];
  const empty = status !== null && status.docs === 0 && !status.busy;
  return (
    <div className="home">
      <div className="home-inner">
        <header className="hero">
          <div className="hero-icon">
            <Users size={26} />
          </div>
          <div>
            <h1>{t.welcome}</h1>
            <p>{t.tagline}</p>
          </div>
        </header>

        <div className="stats">
          {stats.map((s, i) => (
            <div key={i} className="stat" style={{ animationDelay: `${i * 60}ms` }}>
              <s.icon size={18} />
              <strong>
                <AnimatedNumber value={s.value} format={s.fmt ?? ((n) => Math.round(n).toLocaleString(lang))} />
              </strong>
              <span>{s.label}</span>
            </div>
          ))}
        </div>

        {empty && (
          <div className="empty-state">
            <Upload size={34} />
            <h2>{t.emptyTitle}</h2>
            <p>{t.emptyText}</p>
            <button className="btn" onClick={onImportDefaults}>
              <Upload size={15} /> {t.importDefaults}
            </button>
          </div>
        )}

        {top.length > 0 && top[0].docs > 1 && (
          <section>
            <h2>
              <Users size={14} /> {t.topPersons}
            </h2>
            <div className="person-grid">
              {top.map((p, i) => (
                <button key={p.id} data-ctx="person" data-id={p.id} className="person-card" style={{ animationDelay: `${i * 35}ms` }} onClick={() => onPerson(p.id)}>
                  <span className="pc-cover">{p.coverDoc ? <img src={fileUrl("cover", p.coverDoc)} alt="" loading="lazy" /> : <UserRound size={22} />}</span>
                  <span className="pc-name">{p.name}</span>
                  <span className="pc-count">{t.documents(p.docs)}</span>
                </button>
              ))}
            </div>
          </section>
        )}

        {facets.length > 0 && (
          <section>
            <h2>{t.collections}</h2>
            <div className="collection-grid">
              {facets.map((f, i) => (
                <button
                  key={f.name}
                  className="collection-card"
                  style={{ "--h": hueOf(f.name), animationDelay: `${i * 40}ms` } as React.CSSProperties}
                  onClick={() => onCollection(f.name)}
                >
                  <span className="cc-name">{f.name}</span>
                  <span className="cc-count">{t.documents(f.count)}</span>
                  <span className="cc-bar">
                    <span style={{ width: `${Math.max(4, (f.count / max) * 100)}%` }} />
                  </span>
                </button>
              ))}
            </div>
          </section>
        )}

        <div className="home-cols">
          {recent.length > 0 && (
            <section>
              <h2>
                <Clock size={14} /> {t.recent}
              </h2>
              <div className="mini-list">
                {recent.slice(0, 8).map((r) => (
                  <button key={r.docId} onClick={() => onRecent(r)} style={{ "--h": hueOf(r.collection) } as React.CSSProperties}>
                    <i className="dot" />
                    <span>{r.title}</span>
                    <em>{r.collection}</em>
                  </button>
                ))}
              </div>
            </section>
          )}
          {bookmarks.length > 0 && (
            <section>
              <h2>
                <Star size={14} /> {t.recentBookmarks}
              </h2>
              <div className="mini-list">
                {bookmarks.slice(0, 8).map((b) => (
                  <button key={b.id} onClick={() => onBookmark(b)} disabled={!b.docId}>
                    <Star size={12} />
                    <span>{b.title}</span>
                    {b.note && <em>{b.note}</em>}
                  </button>
                ))}
              </div>
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
