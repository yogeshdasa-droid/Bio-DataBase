import { useEffect, useMemo, useState } from "react";
import { Clock3, ListTree, UserRound } from "lucide-react";
import { api, fileUrl, LifeSpan } from "../api";
import { Lang, T } from "../i18n";

/** The records in order of time, grouped by century, each with a bar for its life. */
export function Timeline({ t, lang, reload, onPerson }: { t: T; lang: Lang; reload: number; onPerson: (id: number) => void }) {
  const [list, setList] = useState<LifeSpan[] | null>(null);
  const [onlyTree, setOnlyTree] = useState(false);
  useEffect(() => {
    api.timeline().then(setList, () => setList([]));
  }, [reload]);

  const shown = useMemo(() => (list ?? []).filter((l) => !onlyTree || l.parampara), [list, onlyTree]);
  // the scale: from the first year to the last, in whole centuries
  const years = shown.flatMap((l) => [l.born, l.died].filter((y): y is number => y !== null));
  const min = Math.floor(Math.min(...years, 2000) / 100) * 100;
  const max = Math.ceil(Math.max(...years, 1500) / 100) * 100;
  const pos = (y: number) => ((y - min) / (max - min)) * 100;
  const groups = useMemo(() => {
    const g = new Map<number, LifeSpan[]>();
    for (const l of shown) {
      const y = l.born ?? (l.died ?? 0) - 60;
      const c = Math.floor((y - 1) / 100) + 1;
      g.set(c, [...(g.get(c) ?? []), l]);
    }
    return [...g];
  }, [shown]);

  return (
    <div className="reader">
      <div className="reader-scroll">
        <div className="timeline">
          <header className="view-head">
            <h1>
              <Clock3 size={22} /> {t.timeline}
            </h1>
            <p className="muted small">{t.timelineHint}</p>
            <label className={`toggle ${onlyTree ? "on" : ""}`}>
              <input type="checkbox" checked={onlyTree} onChange={(e) => setOnlyTree(e.target.checked)} />
              <span className="switch" />
              <ListTree size={14} /> {t.onlyParampara}
            </label>
          </header>
          {list === null && <span className="spinner" />}
          {groups.map(([c, people]) => (
            <section key={c}>
              <h2>{t.century(c)}</h2>
              {people.map((l) => {
                const from = l.born ?? l.died!;
                const to = l.died ?? l.born!;
                return (
                  <button key={l.id} className={`tl-row${l.parampara ? " tree" : ""}`} onClick={() => onPerson(l.id)}>
                    <span className="tl-name">
                      {l.hasPhoto ? <img src={fileUrl("photo", l.id)} alt="" /> : <UserRound size={14} />}
                      {l.name}
                    </span>
                    <span className="tl-years">
                      {l.born ?? "?"}–{l.died ?? "?"}
                    </span>
                    <span className="tl-track">
                      <span
                        className={`tl-bar${l.born === null || l.died === null ? " partial" : ""}`}
                        style={{ left: `${pos(from)}%`, width: `${Math.max(0.6, pos(to) - pos(from))}%` }}
                      />
                    </span>
                  </button>
                );
              })}
            </section>
          ))}
          {list !== null && (
            <div className="tl-scale">
              {Array.from({ length: (max - min) / 100 + 1 }, (_, i) => min + i * 100).map((y) => (
                <span key={y} style={{ left: `${pos(y)}%` }}>
                  {y.toLocaleString(lang, { useGrouping: false })}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
