#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backup;
mod calendar;
mod db;
mod export;
mod extract;
mod fold;
mod import;
mod search;
mod store;
mod text;

use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// Imported automatically the first time the program starts (if they exist).
const DEFAULT_SOURCES: &[(&str, &str)] = &[
    (r"G:\Whispers\Data Bases\Obsidian\Biografias", "Obsidian Biografías"),
    (r"G:\Whispers\Data Bases\Vaisnava's Biographies", "Calibre"),
    (r"G:\Whispers\Data Bases\Calibre\Yogi Library\Vaisnava's Biographies", "Calibre"),
    (r"G:\Whispers\Data Bases\Obsidian\Goswami Grantha\BIOGRAPH", "Goswami Grantha"),
];

struct AppState {
    db_path: PathBuf,
    conn: Mutex<Connection>,
    /// an import or a search-table rebuild is running
    busy: Arc<AtomicBool>,
    /// an automatic backup copy is being made
    backing_up: Arc<AtomicBool>,
    /// the database could not be opened (e.g. it belongs to a newer version)
    startup_error: Option<String>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    docs: i64,
    sections: i64,
    persons: i64,
    trash: i64,
    files_mb: f64,
    busy: bool,
    db_path: String,
    db_size: u64,
}

#[tauri::command]
async fn get_status(state: State<'_, AppState>) -> CmdResult<Status> {
    if let Some(e) = &state.startup_error {
        return Err(e.clone());
    }
    let conn = state.conn.lock().unwrap();
    let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0);
    Ok(Status {
        docs: count("SELECT COUNT(*) FROM docs WHERE deleted IS NULL"),
        sections: count("SELECT COUNT(*) FROM sections"),
        persons: count("SELECT COUNT(*) FROM persons"),
        trash: count("SELECT COUNT(*) FROM docs WHERE deleted IS NOT NULL"),
        files_mb: count("SELECT COALESCE(SUM(length(data)), 0) FROM files") as f64 / 1048576.0,
        busy: state.busy.load(Ordering::SeqCst),
        db_path: state.db_path.to_string_lossy().to_string(),
        db_size: std::fs::metadata(&state.db_path).map(|m| m.len()).unwrap_or(0)
            + std::fs::metadata(state.db_path.with_extension("db-wal")).map(|m| m.len()).unwrap_or(0),
    })
}

// ---------------------------------------------------------------- background work

/// Runs `job` on its own connection in a background thread; the UI keeps working.
fn background(app: &AppHandle, job: impl FnOnce(&mut Connection, &AppHandle) -> Result<(), String> + Send + 'static) -> CmdResult<()> {
    let state = app.state::<AppState>();
    if state.startup_error.is_some() {
        return Err("La base de datos no está disponible".into());
    }
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("Ya hay una importación en marcha; espera a que termine.".into());
    }
    let flag = state.busy.clone();
    let db_path = state.db_path.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        let result = db::open(&db_path).map_err(err).and_then(|mut conn| job(&mut conn, &app));
        if let Err(e) = result {
            let _ = app.emit("import-error", e);
        }
        flag.store(false, Ordering::SeqCst);
        let _ = app.emit("busy-end", ());
    });
    Ok(())
}

fn start_import(app: &AppHandle, opts: import::Options) -> CmdResult<()> {
    background(app, move |conn, app| {
        let summary = import::run(conn, &opts, |p| {
            let _ = app.emit("import-progress", p);
        })?;
        // after the first import the records exist: the Guru Paramparā can start
        let _ = seed_parampara_once(conn);
        let _ = app.emit("import-done", summary);
        Ok(())
    })
}

/// Fills the Guru Paramparā with the usual line the first time there are
/// records (never again: the user may have emptied it on purpose).
fn seed_parampara_once(conn: &mut Connection) -> CmdResult<()> {
    if db::get_meta(conn, "parampara_seeded").map_err(err)?.is_some() {
        return Ok(());
    }
    let persons: i64 = conn.query_row("SELECT COUNT(*) FROM persons", [], |r| r.get(0)).map_err(err)?;
    if persons > 0 {
        store::seed_parampara(conn)?;
        db::set_meta(conn, "parampara_seeded", "1").map_err(err)?;
    }
    Ok(())
}

fn start_rebuild(app: &AppHandle) -> CmdResult<()> {
    background(app, |conn, app| {
        store::rebuild_fts(conn, |done, total| {
            let _ = app.emit("import-progress", import::Progress { phase: "fts", done, total });
        })?;
        let _ = app.emit("import-done", import::Summary::default());
        Ok(())
    })
}

#[tauri::command]
async fn import_sources(app: AppHandle, options: import::Options) -> CmdResult<()> {
    start_import(&app, options)
}

#[tauri::command]
async fn rebuild_search(app: AppHandle) -> CmdResult<()> {
    start_rebuild(&app)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DefaultSource {
    path: String,
    collection: String,
    exists: bool,
}

#[tauri::command]
async fn default_sources() -> CmdResult<Vec<DefaultSource>> {
    Ok(DEFAULT_SOURCES
        .iter()
        .map(|(p, c)| DefaultSource { path: p.to_string(), collection: c.to_string(), exists: Path::new(p).is_dir() })
        .collect())
}

// ---------------------------------------------------------------- reading

#[tauri::command]
async fn search(state: State<'_, AppState>, params: search::SearchParams) -> CmdResult<search::SearchResult> {
    let conn = state.conn.lock().unwrap();
    search::search(&conn, &params).map_err(err)
}

#[tauri::command]
async fn list_docs(state: State<'_, AppState>) -> CmdResult<Vec<search::DocListItem>> {
    search::list_docs(&state.conn.lock().unwrap()).map_err(err)
}

#[derive(Serialize)]
struct DocWithToc {
    doc: search::DocInfo,
    toc: Vec<search::TocEntry>,
}

#[tauri::command]
async fn get_doc(state: State<'_, AppState>, id: i64) -> CmdResult<DocWithToc> {
    let conn = state.conn.lock().unwrap();
    Ok(DocWithToc {
        doc: search::get_doc(&conn, id).map_err(err)?,
        toc: search::get_toc(&conn, id).map_err(err)?,
    })
}

#[tauri::command]
async fn get_sections(state: State<'_, AppState>, doc_id: i64, from: i64, to: i64) -> CmdResult<Vec<search::SectionText>> {
    search::get_sections(&state.conn.lock().unwrap(), doc_id, from, to).map_err(err)
}

#[tauri::command]
async fn resolve_link(state: State<'_, AppState>, target: String) -> CmdResult<search::LinkTarget> {
    search::resolve_link(&state.conn.lock().unwrap(), &target).map_err(err)
}

// ---------------------------------------------------------------- persons

#[tauri::command]
async fn list_persons(state: State<'_, AppState>) -> CmdResult<Vec<search::PersonListItem>> {
    search::list_persons(&state.conn.lock().unwrap()).map_err(err)
}

#[tauri::command]
async fn get_person(state: State<'_, AppState>, id: i64) -> CmdResult<search::Person> {
    search::get_person(&state.conn.lock().unwrap(), id).map_err(err)
}

#[tauri::command]
async fn search_persons(state: State<'_, AppState>, query: String) -> CmdResult<Vec<i64>> {
    search::search_persons(&state.conn.lock().unwrap(), &query).map_err(err)
}

#[tauri::command]
async fn new_person(state: State<'_, AppState>, fields: store::PersonFields) -> CmdResult<i64> {
    store::new_person(&mut state.conn.lock().unwrap(), &fields)
}

#[tauri::command]
async fn update_person(state: State<'_, AppState>, id: i64, fields: store::PersonFields) -> CmdResult<()> {
    store::update_person(&mut state.conn.lock().unwrap(), id, &fields)
}

#[tauri::command]
async fn delete_person(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    store::delete_person(&mut state.conn.lock().unwrap(), id)
}

#[tauri::command]
async fn merge_persons(state: State<'_, AppState>, into: i64, from: Vec<i64>) -> CmdResult<()> {
    store::merge_persons(&mut state.conn.lock().unwrap(), into, &from)
}

#[tauri::command]
async fn move_person(state: State<'_, AppState>, id: i64, place: Option<store::TreePlace>) -> CmdResult<()> {
    store::move_person(&mut state.conn.lock().unwrap(), id, place)
}

#[tauri::command]
async fn set_person_photo(state: State<'_, AppState>, id: i64, path: String) -> CmdResult<()> {
    store::set_person_photo(&state.conn.lock().unwrap(), id, std::path::Path::new(&path))
}

#[tauri::command]
async fn remove_person_photo(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    store::remove_person_photo(&state.conn.lock().unwrap(), id)
}

// ---------------------------------------------------------------- editing

#[tauri::command]
async fn create_doc(state: State<'_, AppState>, person_id: Option<i64>, title: String, collection: String) -> CmdResult<i64> {
    store::create_doc(&mut state.conn.lock().unwrap(), person_id, &title, &collection)
}

#[tauri::command]
async fn save_section(state: State<'_, AppState>, doc_id: i64, ordinal: i64, text: String) -> CmdResult<()> {
    store::save_section(&mut state.conn.lock().unwrap(), doc_id, ordinal, &text)
}

#[tauri::command]
async fn update_doc(state: State<'_, AppState>, doc_id: i64, meta: store::DocMeta) -> CmdResult<()> {
    store::update_doc(&mut state.conn.lock().unwrap(), doc_id, &meta)
}

#[tauri::command]
async fn rename_folder(state: State<'_, AppState>, dir: String, name: String) -> CmdResult<()> {
    store::rename_folder(&mut state.conn.lock().unwrap(), &dir, &name)
}

#[tauri::command]
async fn trash_doc(state: State<'_, AppState>, doc_id: i64) -> CmdResult<()> {
    store::trash_doc(&mut state.conn.lock().unwrap(), doc_id)
}

#[tauri::command]
async fn restore_doc(state: State<'_, AppState>, doc_id: i64) -> CmdResult<()> {
    store::restore_doc(&mut state.conn.lock().unwrap(), doc_id)
}

#[tauri::command]
async fn list_trash(state: State<'_, AppState>) -> CmdResult<Vec<store::Trashed>> {
    store::list_trash(&state.conn.lock().unwrap())
}

#[tauri::command]
async fn purge_docs(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<()> {
    store::purge_docs(&mut state.conn.lock().unwrap(), &ids)
}

// ---------------------------------------------------------------- bookmarks

#[tauri::command]
async fn list_bookmarks(state: State<'_, AppState>) -> CmdResult<Vec<search::Bookmark>> {
    search::list_bookmarks(&state.conn.lock().unwrap()).map_err(err)
}

#[tauri::command]
async fn add_bookmark(state: State<'_, AppState>, doc_id: i64, ordinal: i64, note: String) -> CmdResult<i64> {
    search::add_bookmark(&state.conn.lock().unwrap(), doc_id, ordinal, &note).map_err(err)
}

#[tauri::command]
async fn update_bookmark(state: State<'_, AppState>, id: i64, note: String) -> CmdResult<()> {
    let conn = state.conn.lock().unwrap();
    conn.execute("UPDATE bookmarks SET note = ?1 WHERE id = ?2", rusqlite::params![note, id])
        .map(|_| ())
        .map_err(err)
}

#[tauri::command]
async fn delete_bookmark(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let conn = state.conn.lock().unwrap();
    conn.execute("DELETE FROM bookmarks WHERE id = ?1", [id]).map(|_| ()).map_err(err)
}

// ---------------------------------------------------------------- files out

/// Writes the stored original (PDF / Word) to `path`.
#[tauri::command]
async fn save_original(state: State<'_, AppState>, doc_id: i64, path: String) -> CmdResult<()> {
    let file = search::get_file(&state.conn.lock().unwrap(), doc_id, "original").map_err(err)?;
    let (_, _, data) = file.ok_or("Este documento no tiene archivo original guardado")?;
    std::fs::write(&path, data).map_err(err)
}

/// Opens the stored original with its usual program (from a temporary copy).
#[tauri::command]
async fn open_original(app: AppHandle, state: State<'_, AppState>, doc_id: i64) -> CmdResult<()> {
    let file = search::get_file(&state.conn.lock().unwrap(), doc_id, "original").map_err(err)?;
    let (_, name, data) = file.ok_or("Este documento no tiene archivo original guardado")?;
    let dir = std::env::temp_dir().join("Bio-Data");
    std::fs::create_dir_all(&dir).map_err(err)?;
    let target = dir.join(format!("{doc_id}-{name}"));
    std::fs::write(&target, data).map_err(err)?;
    app.opener().open_path(target.to_string_lossy().to_string(), None::<&str>).map_err(err)
}

/// Saves a document's text as a Markdown file.
#[tauri::command]
async fn export_doc(state: State<'_, AppState>, doc_id: i64, path: String) -> CmdResult<()> {
    let conn = state.conn.lock().unwrap();
    let doc = search::get_doc(&conn, doc_id).map_err(err)?;
    let body = search::full_text(&conn, doc_id).map_err(err)?;
    std::fs::write(&path, format!("# {}\n\n{}\n", doc.title, body)).map_err(err)
}

/// A complete copy of the database (everything is inside it). Made on its
/// own connection so the program keeps answering meanwhile.
#[tauri::command]
async fn backup(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let conn = db::open(&state.db_path).map_err(err)?;
    backup::copy_to(&conn, Path::new(&path))
}

// ---------------------------------------------------------------- backup copies

#[tauri::command]
async fn backup_settings(state: State<'_, AppState>) -> CmdResult<backup::Settings> {
    Ok(backup::settings(&state.conn.lock().unwrap()))
}

#[tauri::command]
async fn set_backup_settings(state: State<'_, AppState>, days: u32, keep: u32) -> CmdResult<()> {
    backup::set_settings(&state.conn.lock().unwrap(), days, keep)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Copies {
    dir: String,
    copies: Vec<backup::Copy>,
}

#[tauri::command]
async fn list_copies(state: State<'_, AppState>) -> CmdResult<Copies> {
    Ok(Copies {
        dir: backup::copies_dir(&state.db_path).to_string_lossy().to_string(),
        copies: backup::list_copies(&state.db_path),
    })
}

/// A copy now, into the copies folder.
#[tauri::command]
async fn backup_now(state: State<'_, AppState>) -> CmdResult<String> {
    let conn = db::open(&state.db_path).map_err(err)?;
    let stamp: String = conn
        .query_row("SELECT strftime('%Y-%m-%d %H%M', 'now', 'localtime')", [], |r| r.get(0))
        .map_err(err)?;
    let target = backup::copies_dir(&state.db_path).join(format!("Bio-Data {stamp}.db"));
    backup::copy_to(&conn, &target)?;
    Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
async fn delete_copy(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    backup::delete_copy(&state.db_path, &path)
}

/// Replaces everything with the copy at `path` (what there was is kept as a
/// copy first). The page reloads afterwards.
#[tauri::command]
async fn restore_copy(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let src = PathBuf::from(&path);
    if src == state.db_path {
        return Err("Ese archivo es la base de datos en uso".into());
    }
    backup::check_copy(&src)?;
    if state.backing_up.load(Ordering::SeqCst) {
        return Err("Se está haciendo una copia automática; prueba dentro de un momento.".into());
    }
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("Hay una importación en marcha; espera a que termine.".into());
    }
    let result = (|| {
        let mut guard = state.conn.lock().unwrap();
        let old = std::mem::replace(&mut *guard, Connection::open_in_memory().map_err(err)?);
        let (conn, r) = backup::restore(old, &state.db_path, &src);
        *guard = conn;
        let safety = r?;
        let stale = db::init(&guard)?;
        Ok::<_, String>((safety, stale))
    })();
    state.busy.store(false, Ordering::SeqCst);
    let (safety, stale) = result?;
    if stale {
        start_rebuild(&app)?;
    }
    Ok(safety.to_string_lossy().to_string())
}

// ---------------------------------------------------------------- calendar, labels, pictures

/// The next `count` days from y-m-d (the page's local date), with their lunar
/// day and the appearances / disappearances on each.
#[tauri::command]
async fn upcoming(state: State<'_, AppState>, y: i32, m: u32, d: u32, count: i64) -> CmdResult<Vec<calendar::CalDay>> {
    calendar::upcoming(&state.conn.lock().unwrap(), y, m, d, count.clamp(1, 400)).map_err(err)
}

#[tauri::command]
async fn timeline(state: State<'_, AppState>) -> CmdResult<Vec<calendar::LifeSpan>> {
    calendar::timeline(&state.conn.lock().unwrap()).map_err(err)
}

/// Everything out as Markdown files into `dir` (in the background).
#[tauri::command]
async fn export_markdown(app: AppHandle, dir: String) -> CmdResult<()> {
    background(&app, move |conn, app| {
        let (persons, docs) = export::export_all(conn, Path::new(&dir), |done, total| {
            let _ = app.emit("import-progress", import::Progress { phase: "export", done, total });
        })?;
        let _ = app.emit("export-done", (persons, docs, dir.clone()));
        Ok(())
    })
}

/// One record with all its texts as a single Markdown file.
#[tauri::command]
async fn export_person(state: State<'_, AppState>, id: i64, path: String) -> CmdResult<()> {
    export::export_person(&state.conn.lock().unwrap(), id, Path::new(&path))
}

/// Replaces the imported calendar with an .ics file (the user's Vaiṣṇava calendar).
#[tauri::command]
async fn import_calendar(state: State<'_, AppState>, path: String) -> CmdResult<usize> {
    let bytes = std::fs::read(&path).map_err(err)?;
    calendar::import_ics(&mut state.conn.lock().unwrap(), &text::decode(&bytes))
}

#[tauri::command]
async fn calendar_info(state: State<'_, AppState>) -> CmdResult<calendar::CalendarInfo> {
    calendar::calendar_info(&state.conn.lock().unwrap()).map_err(err)
}

#[tauri::command]
async fn clear_calendar(state: State<'_, AppState>) -> CmdResult<()> {
    state.conn.lock().unwrap().execute("DELETE FROM calendar_events", []).map(|_| ()).map_err(err)
}

/// The lunar day a field's text is read as (for the record form).
#[tauri::command]
async fn lunar_label(text: String) -> CmdResult<Option<String>> {
    Ok(calendar::parse_day(&text).map(calendar::label))
}

#[tauri::command]
async fn list_tags(state: State<'_, AppState>) -> CmdResult<Vec<(String, i64)>> {
    store::list_tags(&state.conn.lock().unwrap())
}

#[tauri::command]
async fn add_person_images(state: State<'_, AppState>, id: i64, paths: Vec<String>) -> CmdResult<usize> {
    store::add_person_images(&mut state.conn.lock().unwrap(), id, &paths)
}

#[tauri::command]
async fn remove_person_image(state: State<'_, AppState>, image_id: i64) -> CmdResult<()> {
    store::remove_person_image(&state.conn.lock().unwrap(), image_id)
}

#[tauri::command]
async fn image_as_photo(state: State<'_, AppState>, image_id: i64) -> CmdResult<()> {
    store::image_as_photo(&mut state.conn.lock().unwrap(), image_id)
}

// ---------------------------------------------------------------- tidying up

#[tauri::command]
async fn review_docs(state: State<'_, AppState>) -> CmdResult<store::Review> {
    store::review(&state.conn.lock().unwrap())
}

#[tauri::command]
async fn mark_distinct(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<()> {
    store::mark_distinct(&mut state.conn.lock().unwrap(), &ids)
}

#[tauri::command]
async fn list_distinct(state: State<'_, AppState>) -> CmdResult<Vec<(i64, i64)>> {
    store::list_distinct(&state.conn.lock().unwrap())
}

// ---------------------------------------------------------------- earlier versions

#[tauri::command]
async fn list_versions(state: State<'_, AppState>, kind: String, target: i64) -> CmdResult<Vec<store::Version>> {
    store::list_versions(&state.conn.lock().unwrap(), &kind, target)
}

#[tauri::command]
async fn restore_version(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    store::restore_version(&mut state.conn.lock().unwrap(), id)
}

#[tauri::command]
async fn open_external(app: AppHandle, target: String) -> CmdResult<()> {
    if ["http://", "https://", "mailto:"].iter().any(|p| target.starts_with(p)) {
        app.opener().open_url(target, None::<&str>).map_err(err)
    } else {
        app.opener().open_path(target, None::<&str>).map_err(err)
    }
}

#[tauri::command]
async fn path_kinds(paths: Vec<String>) -> CmdResult<Vec<&'static str>> {
    Ok(paths
        .iter()
        .map(|p| {
            let p = Path::new(p);
            if p.is_dir() {
                "dir"
            } else if extract::is_importable(p) {
                "text"
            } else {
                "other"
            }
        })
        .collect())
}

/// `bio://localhost/cover/12` and `bio://localhost/original/12` (on Windows
/// `http://bio.localhost/...`): files stored in the database, for <img> and
/// the PDF viewer.
fn serve_file(app: &AppHandle, uri: &str) -> tauri::http::Response<Vec<u8>> {
    let not_found = || tauri::http::Response::builder().status(404).body(Vec::new()).unwrap();
    let path = uri.split("://").nth(1).and_then(|r| r.split_once('/')).map(|(_, p)| p).unwrap_or("");
    let mut parts = path.split(['/', '?']);
    let (Some(role), Some(id)) = (parts.next(), parts.next().and_then(|s| s.parse::<i64>().ok())) else {
        return not_found();
    };
    if !["cover", "original", "photo", "image"].contains(&role) {
        return not_found();
    }
    let state = app.state::<AppState>();
    let Ok(conn) = state.conn.lock() else { return not_found() };
    if role == "photo" || role == "image" {
        // can change at any time: never cached
        let found = if role == "photo" { search::get_photo(&conn, id) } else { search::get_image(&conn, id) };
        return match found {
            Ok(Some((mime, data))) => tauri::http::Response::builder()
                .header("Content-Type", mime)
                .header("Cache-Control", "no-store")
                .body(data)
                .unwrap(),
            _ => not_found(),
        };
    }
    match search::get_file(&conn, id, role) {
        Ok(Some((mime, _, data))) => tauri::http::Response::builder()
            .header("Content-Type", mime)
            .header("Cache-Control", "max-age=3600")
            .body(data)
            .unwrap(),
        _ => not_found(),
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_uri_scheme_protocol("bio", |ctx, req| serve_file(ctx.app_handle(), &req.uri().to_string()))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            // BIO_DB (only in development builds) points at a test copy instead
            let test_db = cfg!(debug_assertions).then(|| std::env::var_os("BIO_DB")).flatten();
            let db_path = test_db.map(PathBuf::from).unwrap_or_else(|| dir.join("biodata.db"));
            let conn = db::open(&db_path)?;
            // A database problem is shown in the window instead of closing silently.
            let init = db::init(&conn);
            let (startup_error, stale_fts) = match init {
                Ok(stale) => (None, stale),
                Err(e) => (Some(e), false),
            };
            let first_run = startup_error.is_none() && db::get_meta(&conn, "initial_import")?.is_none();
            if first_run {
                db::set_meta(&conn, "initial_import", "1")?;
                db::set_meta(&conn, "callouts_cleaned", "1")?; // the import cleans them
                db::set_meta(&conn, "titles_tidied", "1")?; // and tidies the titles
                db::set_meta(&conn, "keys_v2", "1")?;
            }
            let mut conn = conn;
            if startup_error.is_none() && db::get_meta(&conn, "callouts_cleaned")?.is_none() {
                // One-time cleanup of the notes imported before; a copy of the
                // database is kept next to it first.
                let copy = dir.join("biodata antes de limpiar notas.db");
                if !copy.exists() {
                    conn.execute("VACUUM INTO ?1", [copy.to_string_lossy()])?;
                }
                store::clean_callouts(&mut conn)?;
                db::set_meta(&conn, "callouts_cleaned", "1")?;
            }
            if startup_error.is_none() && db::get_meta(&conn, "titles_tidied")?.is_none() {
                // one-time: titles of old Word files imported before (a few rows)
                store::tidy_doc_titles(&mut conn)?;
                db::set_meta(&conn, "titles_tidied", "1")?;
            }
            if startup_error.is_none() && db::get_meta(&conn, "keys_v2")?.is_none() {
                // "Srimad …" names: their keys now leave the honorific out
                store::refresh_person_keys(&mut conn)?;
                db::set_meta(&conn, "keys_v2", "1")?;
            }
            if startup_error.is_none() {
                let _ = seed_parampara_once(&mut conn);
            }
            let backing_up = Arc::new(AtomicBool::new(false));
            if startup_error.is_none() && !first_run {
                // the automatic copy, a little after starting so the window opens fast
                let (path, flag, handle) = (db_path.clone(), backing_up.clone(), app.handle().clone());
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(20));
                    flag.store(true, Ordering::SeqCst);
                    let made = backup::auto_backup(&path);
                    flag.store(false, Ordering::SeqCst);
                    match made {
                        Ok(Some(p)) => {
                            let _ = handle.emit("backup-done", p.to_string_lossy().to_string());
                        }
                        Ok(None) => {}
                        Err(e) => {
                            let _ = handle.emit("import-error", format!("Copia automática: {e}"));
                        }
                    }
                });
            }
            app.manage(AppState {
                db_path,
                conn: Mutex::new(conn),
                busy: Arc::new(AtomicBool::new(false)),
                backing_up,
                startup_error,
            });
            if first_run {
                let sources = DEFAULT_SOURCES
                    .iter()
                    .filter(|(p, _)| Path::new(p).is_dir())
                    .map(|(p, c)| import::Source { path: p.to_string(), collection: c.to_string() })
                    .collect();
                let _ = start_import(app.handle(), import::Options { sources, person_per_file: false, person_id: None });
            } else if stale_fts {
                let _ = start_rebuild(app.handle());
            }
            // The window starts hidden and the page shows it once the splash
            // is painted; this is only a safety net if that never happens.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(4));
                if let Some(w) = handle.get_webview_window("main") {
                    let _ = w.show();
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            import_sources,
            rebuild_search,
            default_sources,
            search,
            list_docs,
            get_doc,
            get_sections,
            resolve_link,
            list_persons,
            get_person,
            search_persons,
            new_person,
            update_person,
            delete_person,
            merge_persons,
            move_person,
            set_person_photo,
            remove_person_photo,
            create_doc,
            save_section,
            update_doc,
            rename_folder,
            trash_doc,
            restore_doc,
            list_trash,
            purge_docs,
            list_bookmarks,
            add_bookmark,
            update_bookmark,
            delete_bookmark,
            save_original,
            open_original,
            export_doc,
            backup,
            backup_settings,
            set_backup_settings,
            list_copies,
            backup_now,
            delete_copy,
            restore_copy,
            list_versions,
            restore_version,
            upcoming,
            lunar_label,
            import_calendar,
            timeline,
            export_markdown,
            export_person,
            calendar_info,
            clear_calendar,
            list_tags,
            add_person_images,
            remove_person_image,
            image_as_photo,
            review_docs,
            mark_distinct,
            list_distinct,
            open_external,
            path_kinds
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bio-Data");
}
