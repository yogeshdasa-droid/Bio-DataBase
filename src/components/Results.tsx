import { useEffect, useRef } from "react";
import { FileText, Folder, ListTree, SearchX, UserRound, X } from "lucide-react";
import { Hit, hueOf, SearchResult } from "../api";
import { highlightParts, Term } from "../fold";
import { Lang, T } from "../i18n";
import { AnimatedNumber, CollectionChip } from "./ui";

export interface Filters {
  collections: string[];
  scope: { dir: string; label: string } | null;
  doc: { id: number; title: string } | null;
  person: { id: number; name: string } | null;
  /** only texts of records in the Guru Paramparā */
  paramparaOnly: boolean;
  /** only these kinds: "md" (notes), "pdf", "doc" (Word) */
  kinds: string[];
}

export const EMPTY_FILTERS: Filters = { collections: [], scope: null, doc: null, person: null, paramparaOnly: false, kinds: [] };

export const hasFilter = (f: Filters) =>
  f.collections.length > 0 || !!f.scope || !!f.doc || !!f.person || f.paramparaOnly || f.kinds.length > 0;

interface Props {
  t: T;
  lang: Lang;
  result: SearchResult | null;
  hits: Hit[];
  loading: boolean;
  selected: number;
  onSelect: (i: number) => void;
  onPerson: (id: number) => void;
  onLoadMore: () => void;
  filters: Filters;
  onFilters: (f: Filters) => void;
  hasQuery: boolean;
  generation: number;
}

function Highlighted({ text, terms }: { text: string; terms: Term[] }) {
  return (
    <>
      {highlightParts(text, terms).map((p, i) => (p.hit ? <mark key={i}>{p.s}</mark> : <span key={i}>{p.s}</span>))}
    </>
  );
}

function Skeleton() {
  return (
    <div className="hit skeleton">
      <div className="sk sk-title" />
      <div className="sk sk-meta" />
      <div className="sk sk-line" />
      <div className="sk sk-line short" />
    </div>
  );
}

function ScopeChip({ icon, label, onClear }: { icon: React.ReactNode; label: string; onClear: () => void }) {
  return (
    <span className="scope" title={label}>
      {icon} <span>{label}</span>
      <button onClick={onClear}>
        <X size={12} />
      </button>
    </span>
  );
}

export function Results(p: Props) {
  const { t, lang, result, hits, loading, selected, onSelect, filters, onFilters, hasQuery } = p;
  const listRef = useRef<HTMLDivElement>(null);
  const terms = result?.terms ?? [];

  useEffect(() => {
    listRef.current?.querySelector(`[data-i="${selected}"]`)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [selected]);
  useEffect(() => {
    listRef.current?.scrollTo({ top: 0 });
  }, [p.generation]);

  const toggleCollection = (name: string) => {
    const has = filters.collections.includes(name);
    onFilters({ ...filters, collections: has ? filters.collections.filter((c) => c !== name) : [...filters.collections, name] });
  };
  const scoped = !!filters.scope || !!filters.doc || !!filters.person;

  return (
    <div className="results">
      <div className="filters">
        <div className="facets">
          <button className={`facet ${filters.collections.length === 0 ? "on" : ""}`} onClick={() => onFilters({ ...filters, collections: [] })}>
            {t.allCollections}
          </button>
          {result?.facets.map((f) => (
            <button
              key={f.name}
              className={`facet ${filters.collections.includes(f.name) ? "on" : ""}`}
              style={{ "--h": hueOf(f.name) } as React.CSSProperties}
              onClick={() => toggleCollection(f.name)}
            >
              <i className="dot" />
              {f.name}
              <span className="n">{f.count.toLocaleString(lang)}</span>
            </button>
          ))}
        </div>
        <div className="facets kinds">
          <button
            className={`facet ${filters.paramparaOnly ? "on" : ""}`}
            title={t.onlyParamparaHint}
            onClick={() => onFilters({ ...filters, paramparaOnly: !filters.paramparaOnly })}
          >
            <ListTree size={12} /> {t.parampara}
          </button>
          {(
            [
              ["md", t.kindNotes],
              ["pdf", "PDF"],
              ["doc", "Word"],
            ] as [string, string][]
          ).map(([k, label]) => (
            <button
              key={k}
              className={`facet ${filters.kinds.includes(k) ? "on" : ""}`}
              onClick={() =>
                onFilters({ ...filters, kinds: filters.kinds.includes(k) ? filters.kinds.filter((x) => x !== k) : [...filters.kinds, k] })
              }
            >
              {label}
            </button>
          ))}
        </div>
        {scoped && (
          <div className="filter-row">
            {filters.person && (
              <ScopeChip icon={<UserRound size={13} />} label={filters.person.name} onClear={() => onFilters({ ...filters, person: null })} />
            )}
            {filters.scope && <ScopeChip icon={<Folder size={13} />} label={filters.scope.label} onClear={() => onFilters({ ...filters, scope: null })} />}
            {filters.doc && <ScopeChip icon={<FileText size={13} />} label={filters.doc.title} onClear={() => onFilters({ ...filters, doc: null })} />}
            <button className="link-btn" onClick={() => onFilters(EMPTY_FILTERS)}>
              {t.clearFilters}
            </button>
          </div>
        )}
        <div className="result-count">
          {result && !result.error && (
            <>
              <strong>
                <AnimatedNumber value={result.total} format={(n) => (hasQuery ? t.results(Math.round(n)) : t.documents(Math.round(n)))} />
              </strong>
              <span className="pill">{t.inMs(result.millis)}</span>
            </>
          )}
          <span className={`loading-bar ${loading ? "on" : ""}`} />
        </div>
      </div>

      <div className="result-list" ref={listRef}>
        {result?.error && <div className="error-box">{result.error}</div>}
        {!result && [0, 1, 2, 3, 4].map((i) => <Skeleton key={i} />)}
        {result && !result.error && hits.length === 0 && !loading && (
          <div className="empty-state small">
            <SearchX size={34} />
            <p>{t.noResults}</p>
            <p className="muted small">{t.noResultsHint}</p>
          </div>
        )}
        {hits.map((h, i) => (
          <article
            key={`${p.generation}:${h.sectionId}:${i}`}
            data-i={i}
            data-ctx="hit"
            className={`hit ${i === selected ? "selected" : ""}`}
            style={{ "--h": hueOf(h.collection), animationDelay: `${Math.min(i % 50, 12) * 22}ms` } as React.CSSProperties}
            onClick={() => onSelect(i)}
          >
            <div className="hit-title">
              <Highlighted text={h.title} terms={terms} />
            </div>
            {h.sections > 1 && h.heading && h.heading.toLowerCase() !== h.title.toLowerCase() && (
              <div className="hit-heading">
                § <Highlighted text={h.heading} terms={terms} />
              </div>
            )}
            <div className="hit-meta">
              <CollectionChip name={h.collection} />
              {h.person && h.personId && h.person.toLowerCase() !== h.title.toLowerCase() && (
                <button
                  className="person-chip"
                  title={t.openPerson}
                  onClick={(e) => {
                    e.stopPropagation();
                    p.onPerson(h.personId!);
                  }}
                >
                  <UserRound size={11} /> {h.person}
                </button>
              )}
              {hasQuery && h.hits > 0 && <span className="badge">{t.hitsInSection(h.hits)}</span>}
            </div>
            {h.snippet && <p className="snippet" dangerouslySetInnerHTML={{ __html: h.snippet }} />}
          </article>
        ))}
        {result && hits.length < result.total && (
          <button className="load-more" onClick={p.onLoadMore} disabled={loading}>
            {t.loadMore} · {hits.length.toLocaleString(lang)} / {result.total.toLocaleString(lang)}
          </button>
        )}
      </div>
    </div>
  );
}
