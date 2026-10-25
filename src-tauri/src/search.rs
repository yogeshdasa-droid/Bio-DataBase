//! Query parsing and all read-side queries used by the UI.

use crate::fold::fold_text;
use crate::text::{self, Term};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- query parsing

/// Folded English / Spanish function words (see `fold_word`: "the" stays "the").
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "but", "by", "for", "from", "had", "has", "have", "he",
    "her", "his", "i", "in", "is", "it", "its", "me", "my", "not", "of", "on", "or", "our", "so", "that", "te",
    "the", "their", "them", "they", "tis", "to", "us", "vas", "ve", "vere", "vhat", "vhen", "vho", "vill", "vith", "vhic", "vould", "could", "sould", "also",
    "you", "your", "this", "these", "those", "there", "then", "than", "into", "if", "all", "can", "do", "does",
    "al", "como", "con", "de", "del", "el", "en", "es", "esta", "este", "ha", "la", "las", "lo", "los", "mas",
    "me", "mi", "no", "o", "para", "pero", "por", "que", "se", "si", "sin", "su", "sus", "un", "una", "uno", "y", "ya",
];

fn is_stopword(t: &str) -> bool {
    STOPWORDS.contains(&t)
}

struct Clause {
    tokens: Vec<String>,
    prefix: bool,
}

pub struct Parsed {
    /// FTS5 MATCH expression, `None` when there is nothing positive to search.
    pub fts: Option<String>,
    pub terms: Vec<Term>,
}

/// User syntax: words (all required), "exact phrase", word* (prefix),
/// -word / -"phrase" (exclude), OR between alternatives.
pub fn parse_query(q: &str, prefix_all: bool) -> Parsed {
    // Typographic / Spanish quotes (pasted from Word, the web, Obsidian…) are
    // phrase quotes too.
    let chars: Vec<char> = q
        .chars()
        .map(|c| if matches!(c, '“' | '”' | '„' | '‟' | '«' | '»' | '″' | '＂') { '"' } else { c })
        .collect();
    let mut i = 0;
    // groups of OR-ed clauses; groups are AND-ed
    let mut groups: Vec<Vec<Clause>> = Vec::new();
    let mut negs: Vec<Clause> = Vec::new();
    let mut pending_or = false;

    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let neg = chars[i] == '-' && chars.get(i + 1).is_some_and(|c| !c.is_whitespace());
        if neg {
            i += 1;
        }
        let (raw, quoted) = if chars[i] == '"' {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && chars[j] != '"' {
                j += 1;
            }
            let s: String = chars[start..j].iter().collect();
            i = (j + 1).min(chars.len());
            (s, true)
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            (chars[start..i].iter().collect::<String>(), false)
        };
        let mut prefix = prefix_all;
        if i < chars.len() && chars[i] == '*' {
            prefix = true;
            i += 1;
        }
        if !quoted && (raw == "OR" || raw == "|") && !groups.is_empty() {
            pending_or = true;
            continue;
        }
        if raw.ends_with('*') {
            prefix = true;
        }
        let folded = fold_text(&raw);
        let tokens: Vec<String> = folded.split(' ').filter(|t| !t.is_empty()).map(String::from).collect();
        if tokens.is_empty() {
            continue;
        }
        let clause = Clause { tokens, prefix };
        if neg {
            negs.push(clause);
        } else if pending_or {
            groups.last_mut().unwrap().push(clause);
            pending_or = false;
        } else {
            groups.push(vec![clause]);
        }
    }

    let render = |c: &Clause| format!("\"{}\"{}", c.tokens.join(" "), if c.prefix { "*" } else { "" });
    // A quoted phrase stays one term so it is highlighted only where its
    // words are together, exactly like the FTS phrase query matches.
    let mut terms: Vec<Term> = groups
        .iter()
        .flatten()
        .map(|c| Term { words: c.tokens.clone(), prefix: c.prefix })
        .collect();
    // Words like "of" / "de" still have to match, but highlighting them
    // everywhere is noise when the query has meaningful words too.
    let noise = |t: &Term| t.words.len() == 1 && !t.prefix && is_stopword(&t.words[0]);
    if terms.iter().any(|t| !noise(t)) {
        terms.retain(|t| !noise(t));
    }
    if groups.is_empty() {
        return Parsed { fts: None, terms };
    }
    let mut expr = groups
        .iter()
        .map(|g| format!("({})", g.iter().map(render).collect::<Vec<_>>().join(" OR ")))
        .collect::<Vec<_>>()
        .join(" AND ");
    if !negs.is_empty() {
        expr = format!("({expr}) NOT ({})", negs.iter().map(render).collect::<Vec<_>>().join(" OR "));
    }
    Parsed { fts: Some(expr), terms }
}
// ---------------------------------------------------------------- search

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchParams {
    pub query: String,
    #[serde(default)]
    pub titles_only: bool,
    #[serde(default)]
    pub prefix: bool,
    #[serde(default)]
    pub sort: String, // relevance | title | person
    #[serde(default)]
    pub collections: Vec<String>,
    pub dir: Option<String>,
    pub doc_id: Option<i64>,
    pub person_id: Option<i64>,
    #[serde(default)]
    pub offset: i64,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub section_id: i64,
    pub doc_id: i64,
    pub ordinal: i64,
    pub sections: i64,
    pub heading: Option<String>,
    pub title: String,
    pub collection: String,
    pub dir: String,
    pub person_id: Option<i64>,
    pub person: Option<String>,
    pub snippet: String,
    pub hits: usize,
}

#[derive(Serialize)]
pub struct Facet {
    pub name: String,
    pub count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub total: i64,
    pub hits: Vec<Hit>,
    pub facets: Vec<Facet>,
    pub terms: Vec<Term>,
    pub millis: u128,
    pub error: Option<String>,
}

struct Filters {
    sql: String,
    args: Vec<Value>,
}

fn filters(p: &SearchParams, with_collections: bool) -> Filters {
    let mut sql = String::from(" AND d.deleted IS NULL");
    let mut args: Vec<Value> = Vec::new();
    if with_collections && !p.collections.is_empty() {
        sql.push_str(&format!(" AND d.collection IN ({})", vec!["?"; p.collections.len()].join(",")));
        args.extend(p.collections.iter().map(|c| Value::Text(c.clone())));
    }
    if let Some(dir) = p.dir.as_ref().filter(|d| !d.is_empty()) {
        sql.push_str(" AND (d.dir = ? OR d.dir LIKE ? ESCAPE '\\')");
        let esc = dir.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        args.push(Value::Text(dir.clone()));
        args.push(Value::Text(format!("{esc}/%")));
    }
    if let Some(id) = p.doc_id {
        sql.push_str(" AND d.id = ?");
        args.push(Value::Integer(id));
    }
    if let Some(id) = p.person_id {
        sql.push_str(" AND d.person_id = ?");
        args.push(Value::Integer(id));
    }
    Filters { sql, args }
}

fn order_by(sort: &str, rank: &str) -> String {
    match sort {
        "title" => "d.title COLLATE NOCASE, s.ordinal".into(),
        "person" => "p.name IS NULL, p.name COLLATE NOCASE, d.title COLLATE NOCASE, s.ordinal".into(),
        "path" => "d.dir COLLATE NOCASE, d.title COLLATE NOCASE, s.ordinal".into(),
        _ => format!("{rank}, d.id, s.ordinal"),
    }
}

const HIT_COLS: &str =
    "s.id, s.doc_id, s.ordinal, d.sections, s.heading, d.title, d.collection, d.dir, d.person_id, p.name";

pub fn search(conn: &Connection, p: &SearchParams) -> rusqlite::Result<SearchResult> {
    let started = std::time::Instant::now();
    let parsed = parse_query(&p.query, p.prefix);
    let limit = p.limit.unwrap_or(50).clamp(1, 500);
    let fl = filters(p, true);
    let ff = filters(p, false);

    // (FROM/WHERE clause, leading MATCH argument, rank expression)
    let (from, match_arg, rank): (String, Option<String>, &str) = match (&parsed.fts, p.titles_only) {
        (Some(expr), false) => (
            "sections_fts JOIN sections s ON s.id = sections_fts.rowid JOIN docs d ON d.id = s.doc_id
             LEFT JOIN persons p ON p.id = d.person_id
             WHERE sections_fts MATCH ?"
                .into(),
            Some(expr.clone()),
            "bm25(sections_fts, 6.0, 1.0)",
        ),
        (Some(expr), true) => (
            "docs_fts JOIN docs d ON d.id = docs_fts.rowid JOIN sections s ON s.doc_id = d.id AND s.ordinal = 0
             LEFT JOIN persons p ON p.id = d.person_id
             WHERE docs_fts MATCH ?"
                .into(),
            Some(expr.clone()),
            "bm25(docs_fts)",
        ),
        (None, _) => (
            "docs d JOIN sections s ON s.doc_id = d.id AND s.ordinal = 0 LEFT JOIN persons p ON p.id = d.person_id WHERE 1"
                .into(),
            None,
            "d.dir COLLATE NOCASE, d.title COLLATE NOCASE",
        ),
    };
    let with_match = |extra: &[Value]| -> Vec<Value> {
        let mut v = Vec::new();
        if let Some(m) = &match_arg {
            v.push(Value::Text(m.clone()));
        }
        v.extend_from_slice(extra);
        v
    };

    // Facets (per collection, ignoring the collection filter) give the total too.
    let facet_sql = format!("SELECT d.collection, COUNT(*) FROM {from}{} GROUP BY d.collection ORDER BY 2 DESC", ff.sql);
    let facets: Vec<Facet> = match conn.prepare(&facet_sql).and_then(|mut st| {
        st.query_map(params_from_iter(with_match(&ff.args)), |r| {
            Ok(Facet { name: r.get(0)?, count: r.get(1)? })
        })?
        .collect::<Result<Vec<_>, _>>()
    }) {
        Ok(f) => f,
        Err(e) => {
            return Ok(SearchResult {
                total: 0,
                hits: vec![],
                facets: vec![],
                terms: parsed.terms,
                millis: started.elapsed().as_millis(),
                error: Some(e.to_string()),
            })
        }
    };
    let total = facets
        .iter()
        .filter(|f| p.collections.is_empty() || p.collections.contains(&f.name))
        .map(|f| f.count)
        .sum();

    let sort = if parsed.fts.is_none() && p.sort == "relevance" { "path" } else { p.sort.as_str() };
    let sql = format!(
        "SELECT {HIT_COLS} FROM {from}{} ORDER BY {} LIMIT {limit} OFFSET {}",
        fl.sql,
        order_by(sort, rank),
        p.offset.max(0)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(with_match(&fl.args)), |r| {
            Ok(Hit {
                section_id: r.get(0)?,
                doc_id: r.get(1)?,
                ordinal: r.get(2)?,
                sections: r.get(3)?,
                heading: r.get(4)?,
                title: r.get(5)?,
                collection: r.get(6)?,
                dir: r.get(7)?,
                person_id: r.get(8)?,
                person: r.get(9)?,
                snippet: String::new(),
                hits: 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // Snippets: section text is fetched only for the visible page.
    let mut text_stmt = conn.prepare_cached("SELECT text FROM sections WHERE id = ?1")?;
    let hits = rows
        .into_iter()
        .map(|mut h| {
            let t: String = text_stmt.query_row([h.section_id], |r| r.get(0)).unwrap_or_default();
            let words = if parsed.fts.is_some() && !p.titles_only { 42 } else { 30 };
            let (snip, n) = text::snippet(&t, &parsed.terms, words);
            h.snippet = snip;
            h.hits = n;
            h
        })
        .collect();

    Ok(SearchResult {
        total,
        hits,
        facets,
        terms: parsed.terms,
        millis: started.elapsed().as_millis(),
        error: None,
    })
}

// ---------------------------------------------------------------- documents

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocInfo {
    pub id: i64,
    pub dir: String,
    pub collection: String,
    pub title: String,
    pub date: Option<String>,
    pub tags: Vec<String>,
    pub size: i64,
    pub sections: i64,
    /// md, txt, pdf, docx, doc, html or new
    pub kind: String,
    /// where the file was imported from
    pub source: String,
    pub person_id: Option<i64>,
    pub person: Option<String>,
    /// name of the original PDF / Word file kept in the database
    pub original: Option<String>,
    pub has_cover: bool,
    pub created: String,
    pub modified: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocListItem {
    pub id: i64,
    pub dir: String,
    pub title: String,
    pub person_id: Option<i64>,
}

pub fn list_docs(conn: &Connection) -> rusqlite::Result<Vec<DocListItem>> {
    let mut st = conn.prepare(
        "SELECT id, dir, title, person_id FROM docs WHERE deleted IS NULL ORDER BY dir COLLATE NOCASE, title COLLATE NOCASE",
    )?;
    let rows = st.query_map([], |r| Ok(DocListItem { id: r.get(0)?, dir: r.get(1)?, title: r.get(2)?, person_id: r.get(3)? }))?;
    rows.collect()
}

pub fn get_doc(conn: &Connection, id: i64) -> rusqlite::Result<DocInfo> {
    conn.query_row(
        "SELECT d.id, d.dir, d.collection, d.title, d.date, d.tags, d.size, d.sections, d.kind, d.source,
                d.person_id, p.name, d.created, d.modified,
                (SELECT name FROM files f WHERE f.doc_id = d.id AND f.role = 'original' LIMIT 1),
                EXISTS(SELECT 1 FROM files f WHERE f.doc_id = d.id AND f.role = 'cover')
         FROM docs d LEFT JOIN persons p ON p.id = d.person_id WHERE d.id = ?1",
        [id],
        |r| {
            let tags: String = r.get(5)?;
            Ok(DocInfo {
                id: r.get(0)?,
                dir: r.get(1)?,
                collection: r.get(2)?,
                title: r.get(3)?,
                date: r.get(4)?,
                tags: tags.split(',').filter(|t| !t.is_empty()).map(String::from).collect(),
                size: r.get(6)?,
                sections: r.get(7)?,
                kind: r.get(8)?,
                source: r.get(9)?,
                person_id: r.get(10)?,
                person: r.get(11)?,
                created: r.get(12)?,
                modified: r.get(13)?,
                original: r.get(14)?,
                has_cover: r.get(15)?,
            })
        },
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TocEntry {
    pub ordinal: i64,
    pub heading: Option<String>,
}

pub fn get_toc(conn: &Connection, doc_id: i64) -> rusqlite::Result<Vec<TocEntry>> {
    let mut st = conn.prepare("SELECT ordinal, heading FROM sections WHERE doc_id = ?1 ORDER BY ordinal")?;
    let rows = st.query_map([doc_id], |r| Ok(TocEntry { ordinal: r.get(0)?, heading: r.get(1)? }))?;
    rows.collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionText {
    pub id: i64,
    pub ordinal: i64,
    pub heading: Option<String>,
    pub text: String,
}

pub fn get_sections(conn: &Connection, doc_id: i64, from: i64, to: i64) -> rusqlite::Result<Vec<SectionText>> {
    let mut st = conn.prepare(
        "SELECT id, ordinal, heading, text FROM sections WHERE doc_id = ?1 AND ordinal BETWEEN ?2 AND ?3 ORDER BY ordinal",
    )?;
    let rows = st.query_map(params![doc_id, from, to], |r| {
        Ok(SectionText { id: r.get(0)?, ordinal: r.get(1)?, heading: r.get(2)?, text: r.get(3)? })
    })?;
    rows.collect()
}

/// (mime, file name, bytes) of a stored file.
pub fn get_file(conn: &Connection, doc_id: i64, role: &str) -> rusqlite::Result<Option<(String, String, Vec<u8>)>> {
    conn.query_row(
        "SELECT mime, name, data FROM files WHERE doc_id = ?1 AND role = ?2 ORDER BY id LIMIT 1",
        params![doc_id, role],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
}

pub fn get_photo(conn: &Connection, person_id: i64) -> rusqlite::Result<Option<(String, Vec<u8>)>> {
    conn.query_row("SELECT mime, data FROM person_photos WHERE person_id = ?1", [person_id], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })
    .optional()
}

/// The whole text of a document (for exporting).
pub fn full_text(conn: &Connection, doc_id: i64) -> rusqlite::Result<String> {
    let mut st = conn.prepare("SELECT text FROM sections WHERE doc_id = ?1 ORDER BY ordinal")?;
    let parts: Vec<String> = st.query_map([doc_id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(parts.iter().map(|s| s.trim_end()).collect::<Vec<_>>().join("\n\n"))
}

// ---------------------------------------------------------------- persons

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonListItem {
    pub id: i64,
    pub name: String,
    pub aka: String,
    pub docs: i64,
    /// a document with a cover image
    pub cover_doc: Option<i64>,
    /// a picture chosen for the record (served at /photo/<id>)
    pub has_photo: bool,
}

pub fn list_persons(conn: &Connection) -> rusqlite::Result<Vec<PersonListItem>> {
    let mut st = conn.prepare(
        "SELECT p.id, p.name, p.aka,
                (SELECT COUNT(*) FROM docs d WHERE d.person_id = p.id AND d.deleted IS NULL),
                (SELECT f.doc_id FROM files f JOIN docs d ON d.id = f.doc_id
                 WHERE d.person_id = p.id AND d.deleted IS NULL AND f.role = 'cover' ORDER BY d.title LIMIT 1),
                EXISTS(SELECT 1 FROM person_photos ph WHERE ph.person_id = p.id)
         FROM persons p ORDER BY p.name COLLATE NOCASE",
    )?;
    let rows = st.query_map([], |r| {
        Ok(PersonListItem { id: r.get(0)?, name: r.get(1)?, aka: r.get(2)?, docs: r.get(3)?, cover_doc: r.get(4)?, has_photo: r.get(5)? })
    })?;
    rows.collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonDoc {
    pub id: i64,
    pub title: String,
    pub collection: String,
    pub kind: String,
    pub size: i64,
    pub has_cover: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub id: i64,
    #[serde(flatten)]
    pub fields: crate::store::PersonFields,
    pub created: String,
    pub modified: String,
    pub has_photo: bool,
    pub docs: Vec<PersonDoc>,
}

pub fn get_person(conn: &Connection, id: i64) -> rusqlite::Result<Person> {
    let (fields, created, modified) = conn.query_row(
        "SELECT name, aka, appearance, disappearance, place, guru, disciples, works, notes, created, modified
         FROM persons WHERE id = ?1",
        [id],
        |r| {
            Ok((
                crate::store::PersonFields {
                    name: r.get(0)?,
                    aka: r.get(1)?,
                    appearance: r.get(2)?,
                    disappearance: r.get(3)?,
                    place: r.get(4)?,
                    guru: r.get(5)?,
                    disciples: r.get(6)?,
                    works: r.get(7)?,
                    notes: r.get(8)?,
                },
                r.get(9)?,
                r.get(10)?,
            ))
        },
    )?;
    let mut st = conn.prepare(
        "SELECT d.id, d.title, d.collection, d.kind, d.size,
                EXISTS(SELECT 1 FROM files f WHERE f.doc_id = d.id AND f.role = 'cover')
         FROM docs d WHERE d.person_id = ?1 AND d.deleted IS NULL
         ORDER BY d.kind = 'pdf', d.title COLLATE NOCASE",
    )?;
    let docs = st
        .query_map([id], |r| {
            Ok(PersonDoc {
                id: r.get(0)?,
                title: r.get(1)?,
                collection: r.get(2)?,
                kind: r.get(3)?,
                size: r.get(4)?,
                has_cover: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let has_photo = conn.query_row("SELECT EXISTS(SELECT 1 FROM person_photos WHERE person_id = ?1)", [id], |r| r.get(0))?;
    Ok(Person { id, fields, created, modified, has_photo, docs })
}

/// Records whose name or fields match the query (same syntax as the main search).
pub fn search_persons(conn: &Connection, query: &str) -> rusqlite::Result<Vec<i64>> {
    let parsed = parse_query(query, true);
    let Some(expr) = parsed.fts else { return Ok(vec![]) };
    let mut st = conn.prepare(
        "SELECT rowid FROM persons_fts WHERE persons_fts MATCH ?1 ORDER BY bm25(persons_fts, 8.0, 1.0) LIMIT 500",
    )?;
    let rows = st.query_map([expr], |r| r.get(0))?;
    rows.collect()
}

// ---------------------------------------------------------------- links

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LinkTarget {
    Doc { id: i64 },
    Person { id: i64 },
    None,
}

/// [[wikilinks]]: a document with that title, or else a person with that name.
pub fn resolve_link(conn: &Connection, target: &str) -> rusqlite::Result<LinkTarget> {
    let name = target.split(['#', '^']).next().unwrap_or("").trim();
    let file = name.rsplit('/').next().unwrap_or(name).to_lowercase();
    let stem = file.strip_suffix(".md").or_else(|| file.strip_suffix(".txt")).unwrap_or(&file).to_string();
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM docs WHERE stem = ?1 AND deleted IS NULL ORDER BY person_id IS NULL, id LIMIT 1",
            [&stem],
            |r| r.get(0),
        )
        .optional()?
    {
        return Ok(LinkTarget::Doc { id });
    }
    if let Some(id) = crate::store::find_person(conn, &crate::store::person_key(&stem))? {
        return Ok(LinkTarget::Person { id });
    }
    Ok(LinkTarget::None)
}

// ---------------------------------------------------------------- bookmarks

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: i64,
    pub doc_id: Option<i64>,
    pub ordinal: i64,
    pub title: String,
    pub heading: Option<String>,
    pub note: String,
    pub created: String,
}

pub fn list_bookmarks(conn: &Connection) -> rusqlite::Result<Vec<Bookmark>> {
    let mut st = conn.prepare(
        "SELECT b.id, d.id, b.ordinal, COALESCE(d.title, b.title), b.heading, b.note, b.created
         FROM bookmarks b LEFT JOIN docs d ON d.id = b.doc_id AND d.deleted IS NULL
         ORDER BY b.created DESC, b.id DESC",
    )?;
    let rows = st.query_map([], |r| {
        Ok(Bookmark {
            id: r.get(0)?,
            doc_id: r.get(1)?,
            ordinal: r.get(2)?,
            title: r.get(3)?,
            heading: r.get(4)?,
            note: r.get(5)?,
            created: r.get(6)?,
        })
    })?;
    rows.collect()
}

pub fn add_bookmark(conn: &Connection, doc_id: i64, ordinal: i64, note: &str) -> rusqlite::Result<i64> {
    let title: String = conn.query_row("SELECT title FROM docs WHERE id = ?1", [doc_id], |r| r.get(0))?;
    let heading: Option<String> = conn
        .query_row(
            "SELECT heading FROM sections WHERE doc_id = ?1 AND ordinal = ?2",
            params![doc_id, ordinal],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    conn.execute(
        "INSERT INTO bookmarks(doc_id, ordinal, title, heading, note) VALUES(?1, ?2, ?3, ?4, ?5)",
        params![doc_id, ordinal, title, heading, note],
    )?;
    Ok(conn.last_insert_rowid())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_syntax() {
        let p = parse_query("uvāca krishna", false);
        assert_eq!(p.fts.as_deref(), Some(r#"("uvaca") AND ("krsna")"#));
        let p = parse_query(r#""jīva-dharma" -maya radha OR krsna*"#, false);
        assert_eq!(
            p.fts.as_deref(),
            Some(r#"(("jiva dharma") AND ("radha" OR "krsna"*)) NOT ("maya")"#)
        );
        assert!(parse_query("   ", false).fts.is_none());
        assert_eq!(parse_query("uvac", true).fts.as_deref(), Some(r#"("uvac"*)"#));
        // stopwords still searched, not highlighted (unless they are all there is)
        let p = parse_query("meaning of Mayavadism will", false);
        assert!(p.fts.unwrap().contains("\"of\""));
        assert_eq!(p.terms.iter().map(|t| t.words.join(" ")).collect::<Vec<_>>(), ["meaning", "mayavadism"]);
        assert_eq!(parse_query("of the", false).terms.len(), 2);
        // any kind of quotes makes an exact phrase
        for q in ["“jaiva dharma”", "«jaiva dharma»", "„jaiva dharma“", "\"jaiva dharma\""] {
            assert_eq!(parse_query(q, false).fts.as_deref(), Some(r#"("jaiva dharma")"#), "{q}");
        }
        // quoted phrase = one term (with its stopwords)
        let p = parse_query(r#""the meaning of life" krsna"#, false);
        assert_eq!(p.terms.iter().map(|t| t.words.join(" ")).collect::<Vec<_>>(), ["the meaning of life", "krsna"]);
    }
}
