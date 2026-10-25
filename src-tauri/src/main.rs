#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
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
        let _ = app.emit("import-done", summary);
        Ok(())
    })
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

/// A complete copy of the database (everything is inside it).
#[tauri::command]
async fn backup(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    if Path::new(&path).exists() {
        std::fs::remove_file(&path).map_err(err)?;
    }
    let conn = state.conn.lock().unwrap();
    conn.execute("VACUUM INTO ?1", [&path]).map(|_| ()).map_err(err)
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
    if role != "cover" && role != "original" {
        return not_found();
    }
    let state = app.state::<AppState>();
    let Ok(conn) = state.conn.lock() else { return not_found() };
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
            let db_path = dir.join("biodata.db");
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
            }
            app.manage(AppState {
                db_path,
                conn: Mutex::new(conn),
                busy: Arc::new(AtomicBool::new(false)),
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
            open_external,
            path_kinds
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bio-Data");
}
