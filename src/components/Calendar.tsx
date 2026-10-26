import { useEffect, useState } from "react";
import { CalendarDays, Clock, Moon, Sparkles, UserRound } from "lucide-react";
import { api, CalDay, CalEvent, fileUrl } from "../api";
import { Lang, T } from "../i18n";

/** Days looked at ahead (today included). */
const AHEAD = 30;

function dayName(date: string, lang: Lang, t: T, i: number): string {
  if (i === 0) return t.today;
  if (i === 1) return t.tomorrow;
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(lang === "es" ? "es-ES" : "en-GB", { weekday: "short", day: "numeric", month: "short" });
}

/** Today's lunar day and the events of today and the coming days. */
export function Calendar({ t, lang, reload, onPerson }: { t: T; lang: Lang; reload: number; onPerson: (id: number) => void }) {
  const [days, setDays] = useState<CalDay[] | null>(null);
  useEffect(() => {
    api.upcoming(new Date(), AHEAD).then(setDays, () => setDays([]));
  }, [reload]);
  if (!days?.length) return null;
  const [today, ...rest] = days;
  // the coming days: people and Ekādaśīs (the rest only today, to keep it short)
  const worth = (e: CalEvent) => e.personId !== null || e.kind === "ekadasi";
  const coming = rest.map((d, i) => ({ d, i: i + 1 })).filter(({ d }) => d.events.some(worth));

  const chip = (e: CalEvent, k: number) => {
    const label =
      e.personId === null ? e.name : e.kind === "appearance" ? t.appearanceOf(e.name) : e.kind === "disappearance" ? t.disappearanceOf(e.name) : e.name;
    const icon =
      e.personId !== null ? (
        e.hasPhoto ? (
          <img src={fileUrl("photo", e.personId)} alt="" />
        ) : (
          <UserRound size={14} />
        )
      ) : e.kind === "parana" ? (
        <Clock size={13} />
      ) : (
        <Sparkles size={13} />
      );
    const title = [e.note, e.computed ? t.computedHint : ""].filter(Boolean).join("\n") || undefined;
    const cls = `cal-event ${e.kind}${e.computed ? " computed" : ""}${e.personId === null ? " plain" : ""}`;
    return e.personId !== null ? (
      <button key={k} className={cls} title={title} onClick={() => onPerson(e.personId!)}>
        {icon}
        <span>{label}</span>
      </button>
    ) : (
      <span key={k} className={cls} title={title}>
        {icon}
        <span>{label}</span>
      </span>
    );
  };

  return (
    <section className="calendar">
      <h2 title={t.calendarHint}>
        <CalendarDays size={14} /> {t.calendarTitle}
      </h2>
      <div className="cal-today">
        <div className="cal-day">
          <strong>{t.today}</strong>
          {today.labels.map((l) => (
            <span key={l} className="lunar">
              <Moon size={12} /> {l}
            </span>
          ))}
        </div>
        <div className="cal-events">{today.events.length ? today.events.map(chip) : <span className="muted small">{t.nothingToday}</span>}</div>
      </div>
      {coming.length > 0 && (
        <div className="cal-list">
          <h3>{t.nextDays}</h3>
          {coming.map(({ d, i }) => (
            <div key={d.date} className="cal-row">
              <div className="cal-day">
                <strong>{dayName(d.date, lang, t, i)}</strong>
                <span className="lunar">{d.labels[0]}</span>
              </div>
              <div className="cal-events">{d.events.filter(worth).map(chip)}</div>
            </div>
          ))}
        </div>
      )}
      <p className="muted small">{t.calendarHint}</p>
    </section>
  );
}
