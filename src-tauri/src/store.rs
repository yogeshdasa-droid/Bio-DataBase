//! Write-side operations on the database: documents, their text, persons
//! (biographical records), the recycle bin and the search tables.

use crate::fold::fold_text;
use crate::text::{self, Section};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

type Res<T> = Result<T, String>;

fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}

// ---------------------------------------------------------------- person keys

/// Honorifics that come and go in front of a name.
const HONORIFICS: &[&str] = &["sri", "srila", "srimati", "srimad", "sripad", "sripada", "om", "visnupada", "jagad", "guru"];

/// Matching key of a name: folded, without leading honorifics, and with the
/// final "a" dropped from each word (Thakur = Thakura, das = dasa).
pub fn person_key(name: &str) -> String {
    let folded = fold_text(name);
    let mut words: Vec<&str> = folded.split(' ').filter(|w| !w.is_empty()).collect();
    while words.len() > 1 && HONORIFICS.contains(&words[0]) {
        words.remove(0);
    }
    words
        .iter()
        .map(|w| if w.len() > 3 { w.strip_suffix('a').unwrap_or(w) } else { w })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------- search tables

fn doc_fts_text(tx: &Connection, id: i64) -> rusqlite::Result<Option<String>> {
    tx.query_row(
        "SELECT d.title, d.dir, COALESCE(p.name, ''), COALESCE(p.aka, '')
         FROM docs d LEFT JOIN persons p ON p.id = d.person_id WHERE d.id = ?1 AND d.deleted IS NULL",
        [id],
        |r| {
            let (t, dir, p, aka): (String, String, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
            Ok(fold_text(&format!("{t} {} {p} {aka}", dir.replace('/', " "))))
        },
    )
    .optional()
}

/// Re-writes the title entry of a document (title, folder and person names).
pub fn fts_doc(tx: &Connection, id: i64) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM docs_fts WHERE rowid = ?1", [id])?;
    if let Some(s) = doc_fts_text(tx, id)? {
        tx.prepare_cached("INSERT INTO docs_fts(rowid, title) VALUES(?1, ?2)")?.execute(params![id, s])?;
    }
    Ok(())
}

fn section_fold(title_fold: &str, ordinal: i64, heading: Option<&str>, text: &str) -> (String, String) {
    let mut h = heading.map(fold_text).unwrap_or_default();
    if ordinal == 0 {
        h = format!("{title_fold} {h}");
    }
    (h, fold_text(text))
}

fn fts_sections_remove(tx: &Connection, doc_id: i64) -> rusqlite::Result<()> {
    let ids: Vec<i64> = tx
        .prepare_cached("SELECT id FROM sections WHERE doc_id = ?1")?
        .query_map([doc_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut del = tx.prepare_cached("DELETE FROM sections_fts WHERE rowid = ?1")?;
    for id in ids {
        del.execute([id])?;
    }
    Ok(())
}

fn fts_sections_add(tx: &Connection, doc_id: i64) -> rusqlite::Result<()> {
    let title: String = tx.query_row("SELECT title FROM docs WHERE id = ?1", [doc_id], |r| r.get(0))?;
    let title_fold = fold_text(&title);
    let rows: Vec<(i64, i64, Option<String>, String)> = tx
        .prepare_cached("SELECT id, ordinal, heading, text FROM sections WHERE doc_id = ?1")?
        .query_map([doc_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    let mut ins = tx.prepare_cached("INSERT INTO sections_fts(rowid, heading, body) VALUES(?1, ?2, ?3)")?;
    for (id, ord, heading, body) in rows {
        let (h, b) = section_fold(&title_fold, ord, heading.as_deref(), &body);
        ins.execute(params![id, h, b])?;
    }
    Ok(())
}

pub fn fts_person(tx: &Connection, id: i64) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM persons_fts WHERE rowid = ?1", [id])?;
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT name || ' ' || aka, appearance || ' ' || disappearance || ' ' || place || ' ' || guru || ' ' ||
                    disciples || ' ' || works || ' ' || notes
             FROM persons WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((name, fields)) = row {
        tx.prepare_cached("INSERT INTO persons_fts(rowid, name, fields) VALUES(?1, ?2, ?3)")?
            .execute(params![id, fold_text(&name), fold_text(&fields)])?;
    }
    Ok(())
}

/// Fills the search tables again from the stored data.
pub fn rebuild_fts(conn: &mut Connection, progress: impl Fn(usize, usize)) -> Res<()> {
    let tx = conn.transaction().map_err(e)?;
    tx.execute_batch(
        "INSERT INTO sections_fts(sections_fts) VALUES('delete-all');
         INSERT INTO docs_fts(docs_fts) VALUES('delete-all');
         INSERT INTO persons_fts(persons_fts) VALUES('delete-all');",
    )
    .map_err(e)?;
    let ids: Vec<i64> = tx
        .prepare("SELECT id FROM docs WHERE deleted IS NULL")
        .map_err(e)?
        .query_map([], |r| r.get(0))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;
    let total = ids.len();
    for (i, id) in ids.iter().enumerate() {
        fts_doc(&tx, *id).map_err(e)?;
        fts_sections_add(&tx, *id).map_err(e)?;
        if i % 200 == 0 {
            progress(i, total);
        }
    }
    let pids: Vec<i64> = tx
        .prepare("SELECT id FROM persons")
        .map_err(e)?
        .query_map([], |r| r.get(0))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;
    for id in pids {
        fts_person(&tx, id).map_err(e)?;
    }
    crate::db::set_meta(&tx, "fts", crate::db::FTS_VERSION).map_err(e)?;
    tx.commit().map_err(e)?;
    let _ = conn.execute_batch(
        "INSERT INTO sections_fts(sections_fts) VALUES('optimize');
         INSERT INTO docs_fts(docs_fts) VALUES('optimize');",
    );
    progress(total, total);
    Ok(())
}

// ---------------------------------------------------------------- documents

pub struct NewDoc {
    pub person_id: Option<i64>,
    pub collection: String,
    pub dir: String,
    pub title: String,
    pub date: Option<String>,
    pub tags: String,
    pub kind: String,
    pub source: String,
    pub hash: String,
    pub sections: Vec<Section>,
    /// (role, name, mime, bytes)
    pub files: Vec<(String, String, String, Vec<u8>)>,
}

fn write_sections(tx: &Connection, doc_id: i64, sections: &[Section]) -> rusqlite::Result<()> {
    fts_sections_remove(tx, doc_id)?;
    tx.execute("DELETE FROM sections WHERE doc_id = ?1", [doc_id])?;
    let mut ins = tx.prepare_cached("INSERT INTO sections(doc_id, ordinal, heading, text) VALUES(?1, ?2, ?3, ?4)")?;
    let mut size = 0;
    for (i, s) in sections.iter().enumerate() {
        ins.execute(params![doc_id, i as i64, s.heading, s.text])?;
        size += s.text.len();
    }
    tx.execute(
        "UPDATE docs SET sections = ?1, size = ?2 WHERE id = ?3",
        params![sections.len() as i64, size as i64, doc_id],
    )?;
    fts_sections_add(tx, doc_id)
}

pub fn insert_doc(tx: &Connection, d: &NewDoc) -> rusqlite::Result<i64> {
    tx.prepare_cached(
        "INSERT INTO docs(person_id, collection, dir, title, stem, date, tags, kind, source, hash)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?
    .execute(params![
        d.person_id, d.collection, d.dir, d.title, d.title.to_lowercase(), d.date, d.tags, d.kind, d.source, d.hash
    ])?;
    let id = tx.last_insert_rowid();
    let mut ins = tx.prepare_cached("INSERT INTO files(doc_id, role, name, mime, data) VALUES(?1, ?2, ?3, ?4, ?5)")?;
    for (role, name, mime, data) in &d.files {
        ins.execute(params![id, role, name, mime, data])?;
    }
    write_sections(tx, id, &d.sections)?;
    fts_doc(tx, id)?;
    Ok(id)
}

/// A new, empty document (optionally the biography of `person_id`).
pub fn create_doc(conn: &mut Connection, person_id: Option<i64>, title: &str, collection: &str) -> Res<i64> {
    let title = title.trim();
    if title.is_empty() {
        return Err("El título no puede estar vacío".into());
    }
    let collection = collection.trim();
    let collection = if collection.is_empty() { "Mis textos" } else { collection };
    let tx = conn.transaction().map_err(e)?;
    let id = insert_doc(
        &tx,
        &NewDoc {
            person_id,
            collection: collection.into(),
            dir: collection.into(),
            title: title.into(),
            date: None,
            tags: String::new(),
            kind: "new".into(),
            source: String::new(),
            hash: String::new(),
            sections: vec![Section { heading: None, text: String::new() }],
            files: vec![],
        },
    )
    .map_err(e)?;
    tx.commit().map_err(e)?;
    Ok(id)
}

/// Replaces the text of one section. Large new text is split again so the
/// reader and the results keep pointing at small pieces.
pub fn save_section(conn: &mut Connection, doc_id: i64, ordinal: i64, new_text: &str) -> Res<()> {
    let new_t = new_text.replace("\r\n", "\n");
    let tx = conn.transaction().map_err(e)?;
    let mut all: Vec<Section> = tx
        .prepare("SELECT heading, text FROM sections WHERE doc_id = ?1 ORDER BY ordinal")
        .map_err(e)?
        .query_map([doc_id], |r| Ok(Section { heading: r.get(0)?, text: r.get(1)? }))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;
    let i = ordinal as usize;
    if i >= all.len() {
        return Err("Sección no encontrada".into());
    }
    if all.len() == 1 {
        all = text::split_sections(&new_t);
    } else {
        let mut pieces = text::split_sections(&new_t);
        if pieces.len() == 1 {
            // keep the heading the contents list shows for this section
            pieces[0].heading = all[i].heading.take();
        }
        all.splice(i..=i, pieces);
    }
    write_sections(&tx, doc_id, &all).map_err(e)?;
    tx.execute("UPDATE docs SET modified = datetime('now', 'localtime') WHERE id = ?1", [doc_id]).map_err(e)?;
    tx.commit().map_err(e)
}

fn valid_date(d: &str) -> bool {
    let parts: Vec<&str> = d.split('-').collect();
    let num = |s: &str, len: usize| s.len() == len && s.chars().all(|c| c.is_ascii_digit());
    match parts.as_slice() {
        [y] => num(y, 4),
        [y, m] => num(y, 4) && num(m, 2),
        [y, m, dd] => num(y, 4) && num(m, 2) && num(dd, 2),
        _ => false,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocMeta {
    pub title: String,
    pub date: String,
    pub collection: String,
    pub person_id: Option<i64>,
}

pub fn update_doc(conn: &mut Connection, doc_id: i64, m: &DocMeta) -> Res<()> {
    let title = m.title.trim();
    let date = m.date.trim();
    if title.is_empty() {
        return Err("El título no puede estar vacío".into());
    }
    if !date.is_empty() && !valid_date(date) {
        return Err("Fecha no válida: usa AAAA, AAAA-MM o AAAA-MM-DD".into());
    }
    let tx = conn.transaction().map_err(e)?;
    let (old_col, old_dir): (String, String) = tx
        .query_row("SELECT collection, dir FROM docs WHERE id = ?1", [doc_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(e)?;
    let col = m.collection.trim();
    let (col, dir) = if col.is_empty() || col == old_col {
        (old_col, old_dir)
    } else {
        (col.to_string(), col.to_string())
    };
    tx.execute(
        "UPDATE docs SET title = ?1, stem = ?2, date = ?3, person_id = ?4, collection = ?5, dir = ?6,
                modified = datetime('now', 'localtime') WHERE id = ?7",
        params![title, title.to_lowercase(), (!date.is_empty()).then_some(date), m.person_id, col, dir, doc_id],
    )
    .map_err(e)?;
    fts_doc(&tx, doc_id).map_err(e)?;
    // the first section is found by the title too
    fts_sections_remove(&tx, doc_id).map_err(e)?;
    fts_sections_add(&tx, doc_id).map_err(e)?;
    tx.commit().map_err(e)
}

/// Renames a collection (or a folder inside it) for all its documents.
pub fn rename_folder(conn: &mut Connection, dir: &str, new_name: &str) -> Res<()> {
    let new_name = new_name.trim().replace('/', "-");
    if new_name.is_empty() {
        return Err("El nombre no puede estar vacío".into());
    }
    let new_dir = match dir.rsplit_once('/') {
        Some((parent, _)) => format!("{parent}/{new_name}"),
        None => new_name.clone(),
    };
    let tx = conn.transaction().map_err(e)?;
    let rows: Vec<(i64, String)> = tx
        .prepare("SELECT id, dir FROM docs WHERE dir = ?1 OR substr(dir, 1, length(?1) + 1) = ?1 || '/'")
        .map_err(e)?
        .query_map([dir], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;
    for (id, d) in rows {
        let nd = format!("{new_dir}{}", &d[dir.len()..]);
        let col = nd.split('/').next().unwrap_or(&nd).to_string();
        tx.execute("UPDATE docs SET dir = ?1, collection = ?2 WHERE id = ?3", params![nd, col, id]).map_err(e)?;
        fts_doc(&tx, id).map_err(e)?;
    }
    tx.commit().map_err(e)
}

// ---------------------------------------------------------------- recycle bin

pub fn trash_doc(conn: &mut Connection, doc_id: i64) -> Res<()> {
    let tx = conn.transaction().map_err(e)?;
    fts_sections_remove(&tx, doc_id).map_err(e)?;
    tx.execute("DELETE FROM docs_fts WHERE rowid = ?1", [doc_id]).map_err(e)?;
    tx.execute("UPDATE docs SET deleted = datetime('now', 'localtime') WHERE id = ?1", [doc_id]).map_err(e)?;
    tx.commit().map_err(e)
}

pub fn restore_doc(conn: &mut Connection, doc_id: i64) -> Res<()> {
    let tx = conn.transaction().map_err(e)?;
    tx.execute("UPDATE docs SET deleted = NULL WHERE id = ?1", [doc_id]).map_err(e)?;
    // its person may have been deleted meanwhile
    tx.execute(
        "UPDATE docs SET person_id = NULL WHERE id = ?1 AND person_id NOT IN (SELECT id FROM persons)",
        [doc_id],
    )
    .map_err(e)?;
    fts_doc(&tx, doc_id).map_err(e)?;
    fts_sections_add(&tx, doc_id).map_err(e)?;
    tx.commit().map_err(e)
}

/// Deletes for good (only from the recycle bin).
pub fn purge_docs(conn: &mut Connection, ids: &[i64]) -> Res<()> {
    let tx = conn.transaction().map_err(e)?;
    for id in ids {
        for t in ["sections WHERE doc_id", "files WHERE doc_id", "bookmarks WHERE doc_id", "docs WHERE id"] {
            tx.execute(&format!("DELETE FROM {t} = ?1 AND (SELECT deleted FROM docs WHERE id = ?1) IS NOT NULL"), [id])
                .map_err(e)?;
        }
    }
    tx.commit().map_err(e)?;
    let _ = conn.execute_batch("PRAGMA incremental_vacuum;");
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trashed {
    pub id: i64,
    pub title: String,
    pub collection: String,
    pub person: Option<String>,
    pub deleted: String,
}

pub fn list_trash(conn: &Connection) -> Res<Vec<Trashed>> {
    let mut st = conn
        .prepare(
            "SELECT d.id, d.title, d.collection, p.name, d.deleted FROM docs d
             LEFT JOIN persons p ON p.id = d.person_id WHERE d.deleted IS NOT NULL ORDER BY d.deleted DESC",
        )
        .map_err(e)?;
    let rows = st
        .query_map([], |r| {
            Ok(Trashed { id: r.get(0)?, title: r.get(1)?, collection: r.get(2)?, person: r.get(3)?, deleted: r.get(4)? })
        })
        .map_err(e)?;
    rows.collect::<Result<_, _>>().map_err(e)
}

// ---------------------------------------------------------------- persons

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersonFields {
    pub name: String,
    pub aka: String,
    pub appearance: String,
    pub disappearance: String,
    pub place: String,
    pub guru: String,
    pub disciples: String,
    pub works: String,
    pub notes: String,
}

pub fn create_person(tx: &Connection, name: &str) -> rusqlite::Result<i64> {
    tx.prepare_cached("INSERT INTO persons(name, key) VALUES(?1, ?2)")?
        .execute(params![name, person_key(name)])?;
    let id = tx.last_insert_rowid();
    fts_person(tx, id)?;
    Ok(id)
}

pub fn new_person(conn: &mut Connection, f: &PersonFields) -> Res<i64> {
    let tx = conn.transaction().map_err(e)?;
    let id = create_person(&tx, f.name.trim()).map_err(e)?;
    write_person(&tx, id, f)?;
    tx.commit().map_err(e)?;
    Ok(id)
}

fn write_person(tx: &Connection, id: i64, f: &PersonFields) -> Res<()> {
    let name = f.name.trim();
    if name.is_empty() {
        return Err("El nombre no puede estar vacío".into());
    }
    tx.execute(
        "UPDATE persons SET name = ?1, key = ?2, aka = ?3, appearance = ?4, disappearance = ?5, place = ?6,
                guru = ?7, disciples = ?8, works = ?9, notes = ?10, modified = datetime('now', 'localtime')
         WHERE id = ?11",
        params![
            name, person_key(name), f.aka.trim(), f.appearance.trim(), f.disappearance.trim(), f.place.trim(),
            f.guru.trim(), f.disciples.trim(), f.works.trim(), f.notes.trim(), id
        ],
    )
    .map_err(e)?;
    fts_person(tx, id).map_err(e)?;
    // names are part of the documents' title entries
    let docs: Vec<i64> = tx
        .prepare("SELECT id FROM docs WHERE person_id = ?1")
        .map_err(e)?
        .query_map([id], |r| r.get(0))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;
    for d in docs {
        fts_doc(tx, d).map_err(e)?;
    }
    Ok(())
}

pub fn update_person(conn: &mut Connection, id: i64, f: &PersonFields) -> Res<()> {
    let tx = conn.transaction().map_err(e)?;
    write_person(&tx, id, f)?;
    tx.commit().map_err(e)
}

/// Deletes a record. Its documents stay, without a person.
pub fn delete_person(conn: &mut Connection, id: i64) -> Res<()> {
    let tx = conn.transaction().map_err(e)?;
    let docs: Vec<i64> = tx
        .prepare("SELECT id FROM docs WHERE person_id = ?1")
        .map_err(e)?
        .query_map([id], |r| r.get(0))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;
    tx.execute("UPDATE docs SET person_id = NULL WHERE person_id = ?1", [id]).map_err(e)?;
    tx.execute("DELETE FROM person_photos WHERE person_id = ?1", [id]).map_err(e)?;
    tree_remove(&tx, id).map_err(e)?;
    tx.execute("DELETE FROM persons WHERE id = ?1", [id]).map_err(e)?;
    tx.execute("DELETE FROM persons_fts WHERE rowid = ?1", [id]).map_err(e)?;
    for d in docs {
        fts_doc(&tx, d).map_err(e)?;
    }
    tx.commit().map_err(e)
}

fn get_fields(tx: &Connection, id: i64) -> rusqlite::Result<PersonFields> {
    tx.query_row(
        "SELECT name, aka, appearance, disappearance, place, guru, disciples, works, notes FROM persons WHERE id = ?1",
        [id],
        |r| {
            Ok(PersonFields {
                name: r.get(0)?,
                aka: r.get(1)?,
                appearance: r.get(2)?,
                disappearance: r.get(3)?,
                place: r.get(4)?,
                guru: r.get(5)?,
                disciples: r.get(6)?,
                works: r.get(7)?,
                notes: r.get(8)?,
            })
        },
    )
}

fn join_list(a: &str, b: &str) -> String {
    let mut items: Vec<String> = a.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    for s in b.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if !items.iter().any(|x| x.eq_ignore_ascii_case(s)) {
            items.push(s.to_string());
        }
    }
    items.join(", ")
}

fn join_text(a: &str, b: &str) -> String {
    match (a.trim().is_empty(), b.trim().is_empty()) {
        (_, true) => a.to_string(),
        (true, false) => b.to_string(),
        _ if a.contains(b.trim()) => a.to_string(),
        _ => format!("{a}\n\n{b}"),
    }
}

/// Joins `from` into `into`: documents move over, empty fields are filled
/// and the other name is kept as an alternative name.
/// Removes the Obsidian callout marks ("> [!example] NAME", "> ") from the
/// imported notes (once; new imports are cleaned while importing).
/// Returns how many texts changed.
pub fn clean_callouts(conn: &mut Connection) -> Res<usize> {
    let tx = conn.transaction().map_err(e)?;
    let ids: Vec<i64> = tx
        .prepare("SELECT id FROM docs WHERE kind = 'md'")
        .map_err(e)?
        .query_map([], |r| r.get(0))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;
    let mut changed = 0;
    for id in ids {
        let old: Vec<Section> = tx
            .prepare_cached("SELECT heading, text FROM sections WHERE doc_id = ?1 ORDER BY ordinal")
            .map_err(e)?
            .query_map([id], |r| Ok(Section { heading: r.get(0)?, text: r.get(1)? }))
            .map_err(e)?
            .collect::<Result<_, _>>()
            .map_err(e)?;
        if !old.iter().any(|s| s.text.contains('>')) {
            continue;
        }
        let new: Vec<Section> =
            old.iter().map(|s| Section { heading: s.heading.clone(), text: text::strip_callouts(&s.text) }).collect();
        if new.iter().zip(&old).all(|(n, o)| n.text == o.text) {
            continue;
        }
        write_sections(&tx, id, &new).map_err(e)?;
        changed += 1;
    }
    tx.commit().map_err(e)?;
    Ok(changed)
}

/// Picture of a record, read from an image file and kept in the database.
pub fn set_person_photo(conn: &Connection, id: i64, path: &std::path::Path) -> Res<()> {
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => return Err(format!("Formato de imagen no admitido: .{ext}")),
    };
    let data = std::fs::read(path).map_err(e)?;
    if data.len() > 25 * 1024 * 1024 {
        return Err("La imagen es demasiado grande (máx. 25 MB).".into());
    }
    conn.execute(
        "INSERT OR REPLACE INTO person_photos(person_id, mime, data) VALUES (?1, ?2, ?3)",
        params![id, mime, data],
    )
    .map_err(e)?;
    conn.execute("UPDATE persons SET modified = datetime('now', 'localtime') WHERE id = ?1", [id]).map_err(e)?;
    Ok(())
}

pub fn remove_person_photo(conn: &Connection, id: i64) -> Res<()> {
    conn.execute("DELETE FROM person_photos WHERE person_id = ?1", [id]).map_err(e)?;
    conn.execute("UPDATE persons SET modified = datetime('now', 'localtime') WHERE id = ?1", [id]).map_err(e)?;
    let _ = conn.execute_batch("PRAGMA incremental_vacuum;");
    Ok(())
}

pub fn merge_persons(conn: &mut Connection, into: i64, from: &[i64]) -> Res<()> {
    let tx = conn.transaction().map_err(e)?;
    let mut f = get_fields(&tx, into).map_err(e)?;
    for &other in from.iter().filter(|&&o| o != into) {
        let o = get_fields(&tx, other).map_err(e)?;
        if person_key(&o.name) != person_key(&f.name) {
            f.aka = join_list(&f.aka, &o.name);
        }
        f.aka = join_list(&f.aka, &o.aka);
        f.guru = join_list(&f.guru, &o.guru);
        f.disciples = join_list(&f.disciples, &o.disciples);
        f.works = join_list(&f.works, &o.works);
        for (mine, theirs) in [
            (&mut f.appearance, &o.appearance),
            (&mut f.disappearance, &o.disappearance),
            (&mut f.place, &o.place),
        ] {
            if mine.trim().is_empty() {
                *mine = theirs.clone();
            }
        }
        f.notes = join_text(&f.notes, &o.notes);
        tx.execute("UPDATE docs SET person_id = ?1 WHERE person_id = ?2", params![into, other]).map_err(e)?;
        // the picture of a merged record is kept only if this one has none
        tx.execute("UPDATE OR IGNORE person_photos SET person_id = ?1 WHERE person_id = ?2", params![into, other])
            .map_err(e)?;
        tx.execute("DELETE FROM person_photos WHERE person_id = ?1", [other]).map_err(e)?;
        // in the Guru Paramparā the record that stays takes the place of the
        // merged one if it has none; otherwise the merged one just leaves it
        if tree_parent(&tx, into).map_err(e)?.is_none() {
            tx.execute("UPDATE person_tree SET person_id = ?1 WHERE person_id = ?2", params![into, other]).map_err(e)?;
            tx.execute("UPDATE person_tree SET parent_id = ?1 WHERE parent_id = ?2", params![into, other]).map_err(e)?;
        } else {
            tree_remove(&tx, other).map_err(e)?;
        }
        tx.execute("DELETE FROM persons WHERE id = ?1", [other]).map_err(e)?;
        tx.execute("DELETE FROM persons_fts WHERE rowid = ?1", [other]).map_err(e)?;
    }
    write_person(&tx, into, &f)?;
    tx.commit().map_err(e)
}

// ---------------------------------------------------------------- guru paramparā

/// Where a record goes in the Guru Paramparā: under `parent` (None = first
/// level), before the disciple `before` (None = last).
#[derive(Deserialize, Debug, Clone, Copy)]
pub struct TreePlace {
    pub parent: Option<i64>,
    pub before: Option<i64>,
}

fn tree_parent(tx: &Connection, id: i64) -> rusqlite::Result<Option<Option<i64>>> {
    tx.query_row("SELECT parent_id FROM person_tree WHERE person_id = ?1", [id], |r| r.get(0))
        .optional()
}

fn tree_children(tx: &Connection, parent: Option<i64>) -> rusqlite::Result<Vec<i64>> {
    let mut st = tx.prepare_cached("SELECT person_id FROM person_tree WHERE parent_id IS ?1 ORDER BY pos, person_id")?;
    let rows = st.query_map([parent], |r| r.get(0))?;
    rows.collect()
}

fn tree_write(tx: &Connection, parent: Option<i64>, ids: &[i64]) -> rusqlite::Result<()> {
    for (pos, id) in ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO person_tree(person_id, parent_id, pos) VALUES(?1, ?2, ?3)
             ON CONFLICT(person_id) DO UPDATE SET parent_id = excluded.parent_id, pos = excluded.pos",
            params![id, parent, pos as i64],
        )?;
    }
    Ok(())
}

/// Takes a record out of the tree; its disciples take its place under its guru.
fn tree_remove(tx: &Connection, id: i64) -> rusqlite::Result<()> {
    let Some(parent) = tree_parent(tx, id)? else { return Ok(()) };
    let kids = tree_children(tx, Some(id))?;
    let siblings: Vec<i64> = tree_children(tx, parent)?
        .into_iter()
        .flat_map(|s| if s == id { kids.clone() } else { vec![s] })
        .collect();
    tx.execute("DELETE FROM person_tree WHERE person_id = ?1", [id])?;
    tree_write(tx, parent, &siblings)
}

/// Puts a record (with its disciples) in the Guru Paramparā at `place`, or
/// takes it out (`None`: it goes back to "Other Vaiṣṇavas").
pub fn move_person(conn: &mut Connection, id: i64, place: Option<TreePlace>) -> Res<()> {
    let tx = conn.transaction().map_err(e)?;
    match place {
        None => tree_remove(&tx, id).map_err(e)?,
        Some(TreePlace { parent, before }) => {
            // a guru cannot become the disciple of his own disciple
            let mut up = parent;
            while let Some(p) = up {
                if p == id {
                    return Err("No se puede poner una ficha dentro de sus propios discípulos.".into());
                }
                up = tree_parent(&tx, p).map_err(e)?.flatten();
            }
            if let Some(p) = parent {
                if tree_parent(&tx, p).map_err(e)?.is_none() {
                    return Err("El guru no está en la Guru Paramparā.".into());
                }
            }
            let mut siblings: Vec<i64> = tree_children(&tx, parent).map_err(e)?.into_iter().filter(|&s| s != id).collect();
            let at = before.and_then(|b| siblings.iter().position(|&s| s == b)).unwrap_or(siblings.len());
            siblings.insert(at, id);
            tree_write(&tx, parent, &siblings).map_err(e)?;
        }
    }
    tx.commit().map_err(e)
}

/// The usual line of the Gauḍīya Guru Paramparā, starting from Śrī Caitanya
/// Mahāprabhu: (name, guru). Each name lists the spellings used in the
/// records; the first one found is taken. Names not found are skipped.
const PARAMPARA: &[(&[&str], &str)] = &[
    (&["Caitanya Mahaprabhu", "Lord Caitanya (Krsna Caitanya)"], ""),
    (&["Nityananda", "Lord Nityananda", "Nityananda Prabhu,Srila"], "Caitanya Mahaprabhu"),
    (&["Jahnava Mata", "Jahnava Devi"], "Nityananda"),
    (&["Virabhadra Gosvami", "Virabhadra"], "Nityananda"),
    (&["Advaita Acarya Prabhu", "Advaita Acarya, Sri"], "Caitanya Mahaprabhu"),
    (&["Gadadhara Pandit Gosvami"], "Caitanya Mahaprabhu"),
    (&["Srivasa Pandita"], "Caitanya Mahaprabhu"),
    (&["Svarupa Damodara", "Swarupa Damodara Goswami"], "Caitanya Mahaprabhu"),
    (&["Ramananda Raya", "Ramananda Rai"], "Caitanya Mahaprabhu"),
    (&["Haridasa Thakur"], "Caitanya Mahaprabhu"),
    (&["Rupa Gosvami", "Rupa Goswami,Srila", "Rupa"], "Caitanya Mahaprabhu"),
    (&["Sanatana Gosvami", "Sanatana"], "Caitanya Mahaprabhu"),
    (&["Raghunatha Bhatta Gosvami"], "Caitanya Mahaprabhu"),
    (&["Raghunatha Dasa Gosvami", "Raghunatha Dasa"], "Rupa Gosvami"),
    (&["Jiva Gosvami"], "Rupa Gosvami"),
    (&["Lokanatha Gosvami"], "Caitanya Mahaprabhu"),
    (&["Krsnadasa Kaviraj Gosvami"], "Raghunatha Dasa Gosvami"),
    (&["Srinivasa Acarya", "Srinivasa Acarya Thakur"], "Jiva Gosvami"),
    (&["Syamananda Prabhu"], "Jiva Gosvami"),
    (&["Narottama Dasa", "Narottama Thakur", "Narottama Dasa Adhikari"], "Krsnadasa Kaviraj Gosvami"),
    (&["Visvanatha Cakravarti", "Visvanatha Cakravarti Thakura"], "Narottama Dasa"),
    (&["Baladeva Vidyabhusana"], "Visvanatha Cakravarti"),
    (&["Jagannatha dasa babaji", "Jagannatha Dasa Babaji Maharaja"], "Baladeva Vidyabhusana"),
    (&["Bhaktivinoda Thakura", "Bhakti Vinod Thakur", "Kedarnath Datta (Bhaktivinoda Thakur)"], "Jagannatha dasa babaji"),
    (&["Gaura Kishora dasa babaji", "Gaurakisora Dasa Babaji", "Gaura Kishora babaji"], "Bhaktivinoda Thakura"),
    (&["Srila Bhakti Siddhanta Sarasvati Gosvami"], "Gaura Kishora dasa babaji"),
    (&["A.C. Bhaktivedanta Swami Prabhupada"], "Srila Bhakti Siddhanta Sarasvati Gosvami"),
    (&["Bhakti Rakshak Shridhar", "Rakshaka Sridhara Maharaja"], "Srila Bhakti Siddhanta Sarasvati Gosvami"),
    (&["Bhakti Pragyana Keshava", "Prajnana Kesava Maharaja"], "Srila Bhakti Siddhanta Sarasvati Gosvami"),
    (&["Bhakti Vedanta Narayana", "Narayana Gosvami Maharaja,Srila Bhaktivedanta"], "Bhakti Pragyana Keshava"),
    (&["Bhakti Vedanta Vamana"], "Bhakti Pragyana Keshava"),
    (&["Bhakti Pramod Puri"], "Srila Bhakti Siddhanta Sarasvati Gosvami"),
    (&["Bhakti Dayita Madhava"], "Srila Bhakti Siddhanta Sarasvati Gosvami"),
    (&["Bhakti Hridoy Bon"], "Srila Bhakti Siddhanta Sarasvati Gosvami"),
];

/// Fills the Guru Paramparā once with the usual line (only if it is empty).
pub fn seed_parampara(conn: &mut Connection) -> Res<usize> {
    let tx = conn.transaction().map_err(e)?;
    let used: i64 = tx.query_row("SELECT COUNT(*) FROM person_tree", [], |r| r.get(0)).map_err(e)?;
    if used > 0 {
        return Ok(0);
    }
    let mut found: Vec<(&str, i64)> = Vec::new();
    for (names, guru) in PARAMPARA {
        let id = names.iter().find_map(|n| {
            tx.query_row("SELECT id FROM persons WHERE name = ?1 COLLATE NOCASE ORDER BY id LIMIT 1", [n], |r| r.get::<_, i64>(0))
                .ok()
        });
        let Some(id) = id else { continue };
        if found.iter().any(|&(_, f)| f == id) {
            continue;
        }
        // the guru may be missing: then the line goes on from his own guru
        let mut g = *guru;
        let parent = loop {
            if g.is_empty() {
                break None;
            }
            if let Some(&(_, pid)) = found.iter().find(|(n, _)| *n == g) {
                break Some(pid);
            }
            g = PARAMPARA.iter().find(|(ns, _)| ns[0] == g).map(|(_, up)| *up).unwrap_or("");
        };
        let mut kids = tree_children(&tx, parent).map_err(e)?;
        kids.push(id);
        tree_write(&tx, parent, &kids).map_err(e)?;
        found.push((names[0], id));
    }
    tx.commit().map_err(e)?;
    Ok(found.len())
}

/// Finds the person whose name (or alternative name) has the same key.
pub fn find_person(tx: &Connection, key: &str) -> rusqlite::Result<Option<i64>> {
    if key.is_empty() {
        return Ok(None);
    }
    if let Some(id) = tx
        .prepare_cached("SELECT id FROM persons WHERE key = ?1 ORDER BY id LIMIT 1")?
        .query_row([key], |r| r.get(0))
        .optional()?
    {
        return Ok(Some(id));
    }
    // alternative names
    let mut st = tx.prepare_cached("SELECT id, aka FROM persons WHERE aka <> ''")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (id, aka) = row?;
        if aka.split(',').any(|a| person_key(a) == key) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parampara() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let tx = conn.transaction().unwrap();
        let [cai, rupa, jiva, sanatana, x] = ["Caitanya Mahaprabhu", "Rupa", "Jiva Gosvami", "Sanatana Gosvami", "Zz Prueba"]
            .map(|n| create_person(&tx, n).unwrap());
        tx.commit().unwrap();
        assert_eq!(seed_parampara(&mut conn).unwrap(), 4);
        assert_eq!(seed_parampara(&mut conn).unwrap(), 0, "only once");
        let kids = |c: &Connection, p: Option<i64>| tree_children(c, p).unwrap();
        assert_eq!(kids(&conn, None), vec![cai]);
        assert_eq!(kids(&conn, Some(cai)), vec![rupa, sanatana]);
        assert_eq!(kids(&conn, Some(rupa)), vec![jiva]);
        let list = crate::search::list_persons(&conn).unwrap();
        assert_eq!(list.iter().filter(|p| p.tree.is_some()).count(), 4);
        assert_eq!(list.iter().find(|p| p.id == jiva).unwrap().tree.as_ref().unwrap().parent, Some(rupa));
        // reorder, add, and refuse a guru under his own disciple
        move_person(&mut conn, sanatana, Some(TreePlace { parent: Some(cai), before: Some(rupa) })).unwrap();
        assert_eq!(kids(&conn, Some(cai)), vec![sanatana, rupa]);
        move_person(&mut conn, x, Some(TreePlace { parent: Some(jiva), before: None })).unwrap();
        assert!(move_person(&mut conn, rupa, Some(TreePlace { parent: Some(x), before: None })).is_err());
        // taking Rupa out: his disciples take his place
        move_person(&mut conn, rupa, None).unwrap();
        assert_eq!(kids(&conn, Some(cai)), vec![sanatana, jiva]);
        assert_eq!(kids(&conn, Some(jiva)), vec![x]);
        // merging Jiva (in the tree) into Rupa (not in it): Rupa takes his place
        merge_persons(&mut conn, rupa, &[jiva]).unwrap();
        assert_eq!(kids(&conn, Some(cai)), vec![sanatana, rupa]);
        assert_eq!(kids(&conn, Some(rupa)), vec![x]);
        // merging a guru into his disciple: no loops
        merge_persons(&mut conn, x, &[rupa]).unwrap();
        assert_eq!(kids(&conn, Some(cai)), vec![sanatana, x]);
        delete_person(&mut conn, cai).unwrap();
        assert_eq!(kids(&conn, None), vec![sanatana, x]);
    }

    #[test]
    fn keys() {
        assert_eq!(person_key("Abhirama Thakura"), person_key("Abhirama Thakur"));
        assert_eq!(person_key("Sri Jiva Goswami"), person_key("Jiva Gosvami"));
        assert_eq!(person_key("Lochana dasa"), person_key("Locana Das"));
        assert_ne!(person_key("Rupa Gosvami"), person_key("Rupa"));
    }

    #[test]
    fn photos() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let tx = conn.transaction().unwrap();
        let a = create_person(&tx, "Rupa Gosvami").unwrap();
        let b = create_person(&tx, "Sri Rupa").unwrap();
        tx.commit().unwrap();
        let dir = std::env::temp_dir().join(format!("bio-photo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let img = dir.join("rupa.PNG");
        std::fs::write(&img, b"\x89PNG fake").unwrap();
        assert!(set_person_photo(&conn, a, &dir.join("rupa.txt")).is_err(), "not an image");
        set_person_photo(&conn, b, &img).unwrap();
        assert_eq!(crate::search::get_photo(&conn, b).unwrap().unwrap().0, "image/png");
        // merging moves the picture to a record that has none
        merge_persons(&mut conn, a, &[b]).unwrap();
        assert!(crate::search::get_photo(&conn, a).unwrap().is_some());
        assert!(crate::search::get_person(&conn, a).unwrap().has_photo);
        remove_person_photo(&conn, a).unwrap();
        assert!(crate::search::get_photo(&conn, a).unwrap().is_none());
        set_person_photo(&conn, a, &img).unwrap();
        delete_person(&mut conn, a).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM person_photos", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "deleted with the record");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn edit_and_merge() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let tx = conn.transaction().unwrap();
        let a = create_person(&tx, "Jiva Gosvami").unwrap();
        let b = create_person(&tx, "Sri Jiva").unwrap();
        tx.commit().unwrap();
        let d = create_doc(&mut conn, Some(b), "Jiva - v.1", "Calibre").unwrap();
        save_section(&mut conn, d, 0, "Śrī Jīva Gosvāmī wrote the Ṣaṭ-sandarbha.").unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sections_fts WHERE sections_fts MATCH 'sandarbha'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);

        merge_persons(&mut conn, a, &[b]).unwrap();
        let (pid, aka): (i64, String) = conn
            .query_row("SELECT d.person_id, p.aka FROM docs d JOIN persons p ON p.id = d.person_id", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((pid, aka.as_str()), (a, "Sri Jiva"));
        assert_eq!(find_person(&conn, &person_key("Jiva")).unwrap(), Some(a), "found by alternative name");

        trash_doc(&mut conn, d).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM docs_fts WHERE docs_fts MATCH 'jiva'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        restore_doc(&mut conn, d).unwrap();
        trash_doc(&mut conn, d).unwrap();
        purge_docs(&mut conn, &[d]).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM sections", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
}
