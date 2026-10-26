//! Copies files and folders INTO the database: text, cover images and the
//! original PDFs / Word files. Afterwards the source files are not needed.
//!
//! Each text is attached to the record (person) of the saint it is about:
//! - Calibre books ("Jiva Goswami - v.2") go to "Jiva Goswami";
//! - notes inside one-letter folders (Biografias/A/Abhirama Thakura.md) are
//!   biographies of the person they are named after;
//! - with `person_per_file` every imported file is.
//! Exact copies of a text already in the database are skipped.

use crate::extract::{self, is_importable, keeps_original};
use crate::store::{self, find_person, person_key, NewDoc};
use crate::text::{self, Section};
use rayon::prelude::*;
use regex::Regex;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use walkdir::WalkDir;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub path: String,
    /// collection name; empty = the folder's own name
    #[serde(default)]
    pub collection: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub sources: Vec<Source>,
    #[serde(default)]
    pub person_per_file: bool,
    /// attach everything to this person
    #[serde(default)]
    pub person_id: Option<i64>,
}

#[derive(Clone, Serialize)]
pub struct Progress {
    pub phase: &'static str, // "scan" | "import" | "fts" | "done"
    pub done: usize,
    pub total: usize,
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub added: usize,
    pub duplicates: usize,
    pub persons: usize,
    pub failed: Vec<String>,
    pub millis: u128,
}

struct Item {
    path: PathBuf,
    collection: String,
    dir: String,
    title: String,
    person: Option<String>,
    /// the name comes from a book title ("Rupa - v.1"): it may be short
    book: bool,
    cover: Option<PathBuf>,
    tags: Vec<String>,
}

struct Parsed {
    item: Item,
    kind: String,
    hash: String,
    tags: String,
    sections: Vec<Section>,
    files: Vec<(String, String, String, Vec<u8>)>,
}

// ---------------------------------------------------------------- scanning

static RE_VOLUME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\s*-\s*(v\.\s*\d*|vol\.?\s*\d+|eng)\b.*$").unwrap());
static RE_OPF_TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<dc:title[^>]*>(.*?)</dc:title>").unwrap());
static RE_OPF_SUBJECT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<dc:subject[^>]*>(.*?)</dc:subject>").unwrap());

/// "Jiva Goswami - v.2" → "Jiva Goswami"; numbered compilations
/// ("3 - Vaishnava Saints") are nobody's biography.
fn person_from_book_title(title: &str) -> Option<String> {
    if title.trim_start().starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let name = RE_VOLUME.replace(title, "").trim().to_string();
    (!name.is_empty()).then_some(name)
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn join_dir(root: &str, rel: &[String]) -> String {
    std::iter::once(root.to_string()).chain(rel.iter().cloned()).collect::<Vec<_>>().join("/")
}

/// A Calibre book folder: metadata.opf + the book file + cover.jpg.
fn calibre_item(book_dir: &Path, collection: &str, dir: String) -> Option<Item> {
    let opf = std::fs::read(book_dir.join("metadata.opf")).ok()?;
    let opf = text::decode(&opf);
    let mut files: Vec<PathBuf> = std::fs::read_dir(book_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_importable(p))
        .collect();
    // the PDF is the book when there are several formats
    files.sort_by_key(|p| match extract::ext(p).as_str() {
        "pdf" => 0,
        "docx" => 1,
        _ => 2,
    });
    let path = files.into_iter().next()?;
    let title = RE_OPF_TITLE
        .captures(&opf)
        .map(|c| extract::decode_entities(c[1].trim()))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| text::clean_title(&stem(book_dir)));
    let tags = RE_OPF_SUBJECT.captures_iter(&opf).map(|c| extract::decode_entities(c[1].trim())).collect();
    let cover = book_dir.join("cover.jpg");
    Some(Item {
        path,
        collection: collection.to_string(),
        dir,
        person: person_from_book_title(&title),
        book: true,
        title,
        cover: cover.is_file().then_some(cover),
        tags,
    })
}

fn scan(sources: &[Source], person_per_file: bool) -> Vec<Item> {
    let mut items = Vec::new();
    for src in sources {
        let root = Path::new(&src.path);
        let own_name = file_name(root);
        if root.is_file() {
            if !is_importable(root) {
                continue;
            }
            let parent = root.parent().map(file_name).unwrap_or_default();
            let collection = if src.collection.trim().is_empty() { parent } else { src.collection.trim().to_string() };
            let title = text::clean_title(&stem(root));
            items.push(Item {
                path: root.to_path_buf(),
                dir: collection.clone(),
                collection,
                person: person_per_file.then(|| title.clone()),
                book: false,
                title,
                cover: None,
                tags: vec![],
            });
            continue;
        }
        if !root.is_dir() {
            continue;
        }
        let collection = if src.collection.trim().is_empty() { own_name } else { src.collection.trim().to_string() };
        let mut walker = WalkDir::new(root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'));
        while let Some(Ok(e)) = walker.next() {
            let p = e.path();
            let rel: Vec<String> = p
                .strip_prefix(root)
                .unwrap_or(p)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            if e.file_type().is_dir() {
                if p.join("metadata.opf").is_file() {
                    let parent_rel = &rel[..rel.len().saturating_sub(1)];
                    if let Some(it) = calibre_item(p, &collection, join_dir(&collection, parent_rel)) {
                        items.push(it);
                    }
                    walker.skip_current_dir();
                }
                continue;
            }
            // "~$name.doc" are Word's lock files
            if !is_importable(p) || file_name(p).starts_with("~$") {
                continue;
            }
            let dirs = &rel[..rel.len() - 1];
            let title = text::clean_title(&stem(p));
            // Biografias/A/Abhirama Thakura.md
            let in_letter_folder = dirs.last().is_some_and(|d| d.chars().count() == 1);
            items.push(Item {
                path: p.to_path_buf(),
                collection: collection.clone(),
                dir: join_dir(&collection, dirs),
                person: (person_per_file || in_letter_folder).then(|| title.clone()),
                book: false,
                title,
                cover: None,
                tags: vec![],
            });
        }
    }
    items
}

// ---------------------------------------------------------------- parsing

/// FNV-1a: a stable fingerprint (the same in every version of the program).
fn fingerprint(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Obsidian notes: the YAML header is not part of the text.
fn strip_front_matter(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            return rest[end + 4..].trim_start_matches(['-', '\n']);
        }
    }
    s
}

/// "GAUR-GAN", "HNC02", "VASANT~1": file names from MS-DOS days say little.
fn is_dos_name(stem: &str) -> bool {
    stem.chars().count() <= 8 && !stem.chars().any(|c| c.is_lowercase())
}

/// First line of the text, as a title, when it looks like one.
/// The author's line ("by Sri …") is skipped.
fn first_line_title(text: &str) -> Option<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).take(4).find_map(|line| {
        let t = text::tidy_doc_title(&text::plain(line));
        let t = t.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
        let n = t.chars().count();
        let letters = t.chars().filter(|c| c.is_alphabetic()).count();
        let byline = t.to_lowercase().starts_with("by ");
        ((6..=90).contains(&n) && letters * 10 >= n * 7 && !byline).then_some(t)
    })
}

fn parse(mut item: Item) -> Result<Parsed, String> {
    let bytes = std::fs::read(&item.path).map_err(|e| format!("{}: {e}", item.path.display()))?;
    let raw = extract::read_document(&item.path, &bytes).ok_or_else(|| format!("{}: formato no legible", item.path.display()))?;
    let content = strip_front_matter(&raw).trim().to_string();
    if item.person.is_none() && is_dos_name(&stem(&item.path)) {
        if let Some(t) = first_line_title(&content) {
            item.title = t;
        }
    }
    // Same words = same text, whatever the file looks like.
    let normalized: String = content.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let hash = if normalized.len() > 40 { fingerprint(normalized.as_bytes()) } else { fingerprint(&bytes) };

    let mut files = Vec::new();
    let name = file_name(&item.path);
    if keeps_original(&item.path) {
        files.push(("original".to_string(), name.clone(), extract::mime_of(&name).to_string(), bytes));
    }
    if let Some(c) = &item.cover {
        if let Ok(b) = std::fs::read(c) {
            files.push(("cover".to_string(), file_name(c), extract::mime_of(&file_name(c)).to_string(), b));
        }
    }
    let mut tags = item.tags.clone();
    for t in text::extract_tags(&content) {
        if !tags.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
            tags.push(t);
        }
    }
    let mut sections = text::split_sections(&content);
    if sections.is_empty() {
        sections.push(Section { heading: None, text: String::new() });
    }
    Ok(Parsed {
        kind: extract::ext(&item.path),
        item,
        hash,
        tags: tags.join(","),
        sections,
        files,
    })
}

// ---------------------------------------------------------------- persons

/// Existing record for this name, or a new one. The short name of a book
/// ("Rupa - v.1") also matches a single longer record that starts with it
/// ("Rupa Gosvami"); two notes are always two people, though.
fn person_for(
    tx: &Connection,
    name: &str,
    book: bool,
    cache: &mut HashMap<(String, bool), i64>,
    created: &mut usize,
) -> rusqlite::Result<Option<i64>> {
    let key = person_key(name);
    if key.is_empty() {
        return Ok(None);
    }
    if let Some(id) = cache.get(&(key.clone(), book)) {
        return Ok(Some(*id));
    }
    let mut id = find_person(tx, &key)?;
    if id.is_none() && book {
        let like = format!("{} %", key.replace('%', ""));
        let ids: Vec<i64> = tx
            .prepare_cached("SELECT id FROM persons WHERE key LIKE ?1 LIMIT 2")?
            .query_map([like], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        if ids.len() == 1 {
            id = Some(ids[0]);
        }
    }
    let id = match id {
        Some(id) => id,
        None => {
            *created += 1;
            store::create_person(tx, name.trim())?
        }
    };
    cache.insert((key, book), id);
    Ok(Some(id))
}

// ---------------------------------------------------------------- run

pub fn run(conn: &mut Connection, opts: &Options, progress: impl Fn(Progress)) -> Result<Summary, String> {
    let started = std::time::Instant::now();
    let mut summary = Summary::default();
    progress(Progress { phase: "scan", done: 0, total: 0 });
    let mut items = scan(&opts.sources, opts.person_per_file);
    // People with their own notes first: their full names are the best records.
    items.sort_by_key(|i| (i.person.is_none(), i.cover.is_some()));
    let total = items.len();
    progress(Progress { phase: "import", done: 0, total });

    let mut cache: HashMap<(String, bool), i64> = HashMap::new();
    let mut done = 0;
    let mut iter = items.into_iter().peekable();
    while iter.peek().is_some() {
        // small batches: PDFs are parsed in parallel, results appear as they go
        let batch: Vec<Item> = iter.by_ref().take(48).collect();
        let n = batch.len();
        let parsed: Vec<Result<Parsed, String>> = batch.into_par_iter().map(parse).collect();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for p in parsed {
            let p = match p {
                Ok(p) => p,
                Err(e) => {
                    summary.failed.push(e);
                    continue;
                }
            };
            let dup: Option<i64> = tx
                .prepare_cached("SELECT id FROM docs WHERE hash = ?1 AND deleted IS NULL LIMIT 1")
                .and_then(|mut s| s.query_row([&p.hash], |r| r.get(0)).optional())
                .map_err(|e| e.to_string())?;
            if dup.is_some() {
                summary.duplicates += 1;
                continue;
            }
            let person_id = match (opts.person_id, &p.item.person) {
                (Some(id), _) => Some(id),
                (None, Some(name)) => {
                    person_for(&tx, name, p.item.book, &mut cache, &mut summary.persons).map_err(|e| e.to_string())?
                }
                _ => None,
            };
            store::insert_doc(
                &tx,
                &NewDoc {
                    person_id,
                    collection: p.item.collection.clone(),
                    dir: p.item.dir.clone(),
                    title: p.item.title.clone(),
                    date: None,
                    tags: p.tags,
                    kind: p.kind,
                    source: p.item.path.to_string_lossy().to_string(),
                    hash: p.hash,
                    sections: p.sections,
                    files: p.files,
                },
            )
            .map_err(|e| format!("{}: {e}", p.item.path.display()))?;
            summary.added += 1;
        }
        tx.commit().map_err(|e| e.to_string())?;
        done += n;
        progress(Progress { phase: "import", done, total });
    }
    if summary.added > 0 {
        let _ = conn.execute_batch(
            "INSERT INTO sections_fts(sections_fts) VALUES('optimize');
             INSERT INTO docs_fts(docs_fts) VALUES('optimize');",
        );
    }
    summary.millis = started.elapsed().as_millis();
    progress(Progress { phase: "done", done: total, total });
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_lines() {
        assert_eq!(first_line_title("ÃTHE ACCEPTANCE OF THE HOLY NAME

All glories").as_deref(), Some("THE ACCEPTANCE OF THE HOLY NAME"));
        assert_eq!(first_line_title("Çréla Raghunätha däsa Goswämé’s

text").as_deref(), Some("Śrīla Raghunātha dāsa Goswāmī’s"));
        assert_eq!(first_line_title("Áby Sri Gopijanavallabha dasa
Ð

Glory to Syamananda").as_deref(), Some("Glory to Syamananda"));
    }

    #[test]
    fn book_titles() {
        assert_eq!(person_from_book_title("Jiva Goswami - v.2").as_deref(), Some("Jiva Goswami"));
        assert_eq!(person_from_book_title("Ramanujacharya  - v.1 - Eng").as_deref(), Some("Ramanujacharya"));
        assert_eq!(person_from_book_title("Gopala Bhatta Gosvami").as_deref(), Some("Gopala Bhatta Gosvami"));
        assert_eq!(person_from_book_title("3 - Vaishnava Saints"), None);
        assert_eq!(strip_front_matter("---\ntags: x\n---\n\nHola"), "Hola");
    }

    /// Imports the real folders into a test database:
    /// `BIO_DB=... cargo test --release real_import -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_import() {
        let db_path = std::env::var("BIO_DB").expect("BIO_DB");
        let _ = std::fs::remove_file(&db_path);
        let mut conn = crate::db::open(Path::new(&db_path)).unwrap();
        crate::db::init(&conn).unwrap();
        let sources = [
            (r"G:\Whispers\Data Bases\Obsidian\Biografias", "Obsidian Biografías"),
            (r"G:\Whispers\Data Bases\Vaisnava's Biographies", "Calibre"),
            (r"G:\Whispers\Data Bases\Calibre\Yogi Library\Vaisnava's Biographies", "Calibre"),
            (r"G:\Whispers\Data Bases\Obsidian\Goswami Grantha\BIOGRAPH", "Goswami Grantha"),
        ]
        .iter()
        .map(|(p, c)| Source { path: p.to_string(), collection: c.to_string() })
        .collect();
        let opts = Options { sources, person_per_file: false, person_id: None };
        let s = run(&mut conn, &opts, |p| {
            if p.done % 480 == 0 {
                println!("{} {}/{}", p.phase, p.done, p.total)
            }
        })
        .unwrap();
        println!(
            "added {} duplicates {} persons {} failed {} in {} s",
            s.added, s.duplicates, s.persons, s.failed.len(), s.millis / 1000
        );
        for f in s.failed.iter().take(10) {
            println!("  FAILED {f}");
        }
        let q = |sql: &str| -> Vec<String> {
            let mut st = conn.prepare(sql).unwrap();
            st.query_map([], |r| r.get::<_, String>(0)).unwrap().map(|x| x.unwrap()).collect()
        };
        println!("most documents per person:");
        for l in q("SELECT p.name || ': ' || COUNT(*) || ' — ' || group_concat(d.title, ' | ') FROM docs d JOIN persons p ON p.id = d.person_id GROUP BY p.id ORDER BY COUNT(*) DESC LIMIT 12") {
            println!("  {l}");
        }
        println!("calibre books with their own new record (no note matched):");
        for l in q("SELECT p.name FROM persons p WHERE NOT EXISTS(SELECT 1 FROM docs d WHERE d.person_id = p.id AND d.kind = 'md') LIMIT 40") {
            print!("{l} · ");
        }
        println!("\nold Word files:");
        for l in q("SELECT d.title || ' (' || d.size || ' chars): ' || substr(replace(s.text, char(10), ' '), 1, 160) FROM docs d JOIN sections s ON s.doc_id = d.id AND s.ordinal = 0 WHERE d.kind = 'doc' LIMIT 8") {
            println!("  {l}");
        }
        for l in q("SELECT d.kind || ': ' || COUNT(*) || ' docs, ' || CAST(SUM(d.size) / 1024 AS TEXT) || ' KB text, ' || CAST((SELECT COALESCE(SUM(length(f.data)), 0) / 1048576 FROM files f JOIN docs e ON e.id = f.doc_id WHERE e.kind = d.kind) AS TEXT) || ' MB files' FROM docs d GROUP BY d.kind") {
            println!("  {l}");
        }
        for l in q("SELECT 'empty text: ' || COUNT(*) || ' — ' || COALESCE(group_concat(title, ' | '), '') FROM docs WHERE size < 50") {
            println!("  {l}");
        }
    }

    #[test]
    fn import_folder() {
        let dir = std::env::temp_dir().join(format!("bio-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Bio/J")).unwrap();
        std::fs::create_dir_all(dir.join("Cal/Jiva Goswami - v.1 (12)")).unwrap();
        std::fs::create_dir_all(dir.join("Cal/Jiva Goswami - v.1 (13)")).unwrap();
        std::fs::write(dir.join("Bio/J/Jiva Gosvami.md"), "> [!example] JIVA GOSVAMI\n\n>He wrote the Sat-sandarbha.").unwrap();
        std::fs::write(dir.join("Bio/Compilation.md"), "Many lives of many saints, all in one long text.").unwrap();
        for n in ["12", "13"] {
            let b = dir.join(format!("Cal/Jiva Goswami - v.1 ({n})"));
            std::fs::write(b.join("metadata.opf"), "<dc:title>Jiva Goswami - v.1</dc:title>").unwrap();
            std::fs::write(b.join("book.txt"), "The same volume, twice in Calibre: Jiva lived in Vrindavan.").unwrap();
        }
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let src = |p: &str| Source { path: dir.join(p).to_string_lossy().to_string(), collection: String::new() };
        let opts = Options { sources: vec![src("Bio"), src("Cal")], person_per_file: false, person_id: None };
        let s = run(&mut conn, &opts, |_| {}).unwrap();
        assert_eq!((s.added, s.duplicates, s.persons), (3, 1, 1), "{:?}", s.failed);
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM docs WHERE person_id IS NOT NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "note and book share one record");
        let dir_of: String = conn.query_row("SELECT dir FROM docs WHERE kind = 'txt'", [], |r| r.get(0)).unwrap();
        assert_eq!(dir_of, "Cal");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
