//! The database IS the library: persons (biographical records), documents,
//! their text, cover images and original files all live here. Only the FTS
//! search tables are derived data and can be rebuilt from the rest.

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

/// Format of the data tables. A newer value means a newer program wrote the
/// file: it is refused instead of risking the user's data.
/// 2: + person_photos (created empty on open; nothing else changes).
pub const DATA_VERSION: u32 = 2;
/// Bump when the folding rules or the FTS layout change: the search tables
/// are rebuilt from `sections` / `docs` / `persons` (no data is lost).
pub const FTS_VERSION: &str = "1";

const DATA_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- one record per saint / devotee
CREATE TABLE IF NOT EXISTS persons(
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    key TEXT NOT NULL,              -- folded name used to match documents
    aka TEXT NOT NULL DEFAULT '',   -- other names, comma separated
    appearance TEXT NOT NULL DEFAULT '',
    disappearance TEXT NOT NULL DEFAULT '',
    place TEXT NOT NULL DEFAULT '',
    guru TEXT NOT NULL DEFAULT '',
    disciples TEXT NOT NULL DEFAULT '',
    works TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    created TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    modified TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX IF NOT EXISTS persons_key ON persons(key);

CREATE TABLE IF NOT EXISTS docs(
    id INTEGER PRIMARY KEY,
    person_id INTEGER,              -- NULL for compilations
    collection TEXT NOT NULL,       -- where it came from (first folder of `dir`)
    dir TEXT NOT NULL,              -- virtual folder, '/' separated
    title TEXT NOT NULL,
    stem TEXT NOT NULL,             -- lowercase title, for [[wikilinks]]
    date TEXT,
    tags TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL,             -- md, txt, pdf, docx, doc, html, new
    source TEXT NOT NULL DEFAULT '',-- original path at import time (information only)
    hash TEXT NOT NULL DEFAULT '',  -- of the original bytes: skips exact duplicates
    sections INTEGER NOT NULL DEFAULT 0,
    size INTEGER NOT NULL DEFAULT 0,-- bytes of text
    deleted TEXT,                   -- set = in the recycle bin
    created TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    modified TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX IF NOT EXISTS docs_person ON docs(person_id);
CREATE INDEX IF NOT EXISTS docs_dir ON docs(dir);
CREATE INDEX IF NOT EXISTS docs_collection ON docs(collection);
CREATE INDEX IF NOT EXISTS docs_stem ON docs(stem);
CREATE INDEX IF NOT EXISTS docs_hash ON docs(hash);

CREATE TABLE IF NOT EXISTS sections(
    id INTEGER PRIMARY KEY,
    doc_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL,
    heading TEXT,
    text TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS sections_doc ON sections(doc_id, ordinal);

-- original PDFs / Word files and cover images, stored inside the database
CREATE TABLE IF NOT EXISTS files(
    id INTEGER PRIMARY KEY,
    doc_id INTEGER NOT NULL,
    role TEXT NOT NULL,             -- original | cover
    name TEXT NOT NULL,
    mime TEXT NOT NULL,
    data BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS files_doc ON files(doc_id, role);

CREATE TABLE IF NOT EXISTS bookmarks(
    id INTEGER PRIMARY KEY,
    doc_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL,
    title TEXT NOT NULL,
    heading TEXT,
    note TEXT NOT NULL DEFAULT '',
    created TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

-- picture chosen by the user for a record (format 2)
CREATE TABLE IF NOT EXISTS person_photos(
    person_id INTEGER PRIMARY KEY,
    mime TEXT NOT NULL,
    data BLOB NOT NULL
);
"#;

const FTS_SCHEMA: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS sections_fts USING fts5(
    heading, body,
    content='', contentless_delete=1,
    tokenize='unicode61 remove_diacritics 0', prefix='2 3'
);
CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
    title,
    content='', contentless_delete=1,
    tokenize='unicode61 remove_diacritics 0', prefix='2 3'
);
CREATE VIRTUAL TABLE IF NOT EXISTS persons_fts USING fts5(
    name, fields,
    content='', contentless_delete=1,
    tokenize='unicode61 remove_diacritics 0', prefix='2 3'
);
"#;

const FTS_TABLES: &[&str] = &["sections_fts", "docs_fts", "persons_fts"];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    // only takes effect on a new file: lets "empty the recycle bin" shrink it
    conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "cache_size", -64000)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.busy_timeout(std::time::Duration::from_secs(30))?;
    Ok(conn)
}

/// Prepares the database. Returns `true` when the search tables are new or
/// outdated and must be filled again with `store::rebuild_fts`.
pub fn init(conn: &Connection) -> Result<bool, String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);")
        .map_err(|e| e.to_string())?;
    let stored: Option<u32> = get_meta(conn, "data").map_err(|e| e.to_string())?.and_then(|v| v.parse().ok());
    if let Some(s) = stored.filter(|s| *s > DATA_VERSION) {
        return Err(format!(
            "Esta base de datos es de una versión más nueva de Bio-Data (formato {s}, este programa usa {DATA_VERSION}).\n\
             Abre la versión más reciente del programa."
        ));
    }
    init_schema(conn).map_err(|e| e.to_string())
}

fn init_schema(conn: &Connection) -> rusqlite::Result<bool> {
    conn.execute_batch(DATA_SCHEMA)?;
    set_meta(conn, "data", &DATA_VERSION.to_string())?;
    let stale = get_meta(conn, "fts")?.as_deref() != Some(FTS_VERSION);
    if stale {
        for t in FTS_TABLES {
            conn.execute_batch(&format!("DROP TABLE IF EXISTS {t};"))?;
        }
    }
    conn.execute_batch(FTS_SCHEMA)?;
    let empty = conn.query_row("SELECT NOT EXISTS(SELECT 1 FROM docs)", [], |r| r.get::<_, bool>(0))?;
    if stale && empty {
        set_meta(conn, "fts", FTS_VERSION)?;
        return Ok(false);
    }
    Ok(stale)
}

pub fn get_meta(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}
