//! Backup copies of the database: automatic ones (every few days, the last
//! few kept) in a folder outside the program's own (Documents), so that
//! uninstalling with "delete the application data" does not take them too;
//! and restoring any copy.

use crate::db;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

type Res<T> = Result<T, String>;

fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}

/// Names of the automatic copies start like this (only these are pruned).
const AUTO_PREFIX: &str = "Gaudiya Vaisnavas DB automática ";
/// …and did so when the program was called Bio-Data (v0.3.0).
const OLD_AUTO_PREFIX: &str = "Bio-Data automática ";
/// Default: every 7 days, the last 3 kept.
const DEFAULT_DAYS: u32 = 7;
const DEFAULT_KEEP: u32 = 3;

/// Where the copies go, set once at start (Documents\Gaudiya Vaisnavas DB\Copias de seguridad).
static COPIES: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn set_copies_dir(dir: PathBuf) {
    let _ = COPIES.set(dir);
}

/// The folder of the copies; without one set, `copias` next to the database.
pub fn copies_dir(db_path: &Path) -> PathBuf {
    COPIES.get().cloned().unwrap_or_else(|| old_copies_dir(db_path))
}

/// Where v0.3.0 kept them (inside the program's data folder).
fn old_copies_dir(db_path: &Path) -> PathBuf {
    db_path.parent().unwrap_or(Path::new(".")).join("copias")
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// "2026-10-26 1530" in local time (SQLite knows the time zone).
fn stamp(conn: &Connection, with_time: bool) -> String {
    let fmt = if with_time { "%Y-%m-%d %H%M" } else { "%Y-%m-%d" };
    conn.query_row("SELECT strftime(?1, 'now', 'localtime')", [fmt], |r| r.get(0)).unwrap_or_else(|_| now_secs().to_string())
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 0 = never
    pub days: u32,
    pub keep: u32,
    /// seconds since 1970 of the last automatic copy (0 = none yet)
    pub last: u64,
}

pub fn settings(conn: &Connection) -> Settings {
    let num = |k: &str| db::get_meta(conn, k).ok().flatten().and_then(|v| v.parse::<u64>().ok());
    Settings {
        days: num("backup_days").map(|v| v as u32).unwrap_or(DEFAULT_DAYS),
        keep: num("backup_keep").map(|v| v as u32).unwrap_or(DEFAULT_KEEP).max(1),
        last: num("backup_last").unwrap_or(0),
    }
}

pub fn set_settings(conn: &Connection, days: u32, keep: u32) -> Res<()> {
    db::set_meta(conn, "backup_days", &days.to_string()).map_err(e)?;
    db::set_meta(conn, "backup_keep", &keep.max(1).to_string()).map_err(e)
}

/// A complete copy of the database into `target` (replaced if it exists).
/// Written under a temporary name first, so a half-made copy never looks finished.
pub fn copy_to(conn: &Connection, target: &Path) -> Res<()> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(e)?;
    }
    let part = target.with_extension("db.part");
    let _ = std::fs::remove_file(&part);
    conn.execute("VACUUM INTO ?1", [part.to_string_lossy()]).map_err(e)?;
    let _ = std::fs::remove_file(target);
    std::fs::rename(&part, target).map_err(e)
}

/// Makes the automatic copy if it is due. Returns the copy's path when made.
pub fn auto_backup(db_path: &Path) -> Res<Option<PathBuf>> {
    let conn = db::open(db_path).map_err(e)?;
    let s = settings(&conn);
    if s.days == 0 {
        return Ok(None);
    }
    let empty: bool = conn.query_row("SELECT NOT EXISTS(SELECT 1 FROM docs)", [], |r| r.get(0)).map_err(e)?;
    // an hour of slack: a copy made at 10:05 is due again a week later at 9:30
    if empty || now_secs() + 3600 < s.last + s.days as u64 * 86400 {
        return Ok(None);
    }
    let dir = copies_dir(db_path);
    let target = dir.join(format!("{AUTO_PREFIX}{}.db", stamp(&conn, false)));
    copy_to(&conn, &target)?;
    db::set_meta(&conn, "backup_last", &now_secs().to_string()).map_err(e)?;
    prune(&dir, s.keep as usize);
    Ok(Some(target))
}

/// Keeps only the newest `keep` automatic copies (their names sort by date).
fn prune(dir: &Path, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut autos: Vec<PathBuf> = rd
        .flatten()
        .map(|d| d.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with(AUTO_PREFIX) && name.ends_with(".db")
        })
        .collect();
    autos.sort();
    let extra = autos.len().saturating_sub(keep);
    for p in &autos[..extra] {
        let _ = std::fs::remove_file(p);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Copy {
    pub path: String,
    pub name: String,
    pub size: u64,
    /// seconds since 1970
    pub modified: u64,
    pub auto: bool,
}

/// Copies in the copies folder, in the old `copias` folder, and the ones made
/// before a big change next to the database. Newest first.
pub fn list_copies(db_path: &Path) -> Vec<Copy> {
    let mut out = Vec::new();
    let main = db_path.file_name().map(|n| n.to_os_string());
    let mut dirs = vec![copies_dir(db_path), old_copies_dir(db_path), db_path.parent().unwrap_or(Path::new(".")).to_path_buf()];
    dirs.dedup();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.extension().and_then(|x| x.to_str()) != Some("db") || Some(p.file_name().unwrap().to_os_string()) == main {
                continue;
            }
            let Ok(md) = entry.metadata() else { continue };
            let name = p.file_stem().and_then(|n| n.to_str()).unwrap_or("").to_string();
            out.push(Copy {
                path: p.to_string_lossy().to_string(),
                auto: name.starts_with(AUTO_PREFIX.trim_end()) || name.starts_with(OLD_AUTO_PREFIX.trim_end()),
                name,
                size: md.len(),
                modified: md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0),
            });
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// Only files listed by `list_copies` may be deleted from the program.
pub fn delete_copy(db_path: &Path, path: &str) -> Res<()> {
    if !list_copies(db_path).iter().any(|c| c.path == path) {
        return Err("Solo se pueden borrar las copias de la lista".into());
    }
    std::fs::remove_file(path).map_err(e)
}

/// Checks that `path` is a database of this program that it can open.
pub fn check_copy(path: &Path) -> Res<()> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|_| "No se puede abrir el archivo".to_string())?;
    let version: Option<u32> = db::get_meta(&conn, "data").ok().flatten().and_then(|v| v.parse().ok());
    let Some(v) = version else {
        return Err("Este archivo no es una base de datos de Gaudiya Vaisnavas DB".into());
    };
    if v > db::DATA_VERSION {
        return Err(format!("La copia es de una versión más nueva de Gaudiya Vaisnavas DB (formato {v}); actualiza el programa primero."));
    }
    conn.query_row("SELECT COUNT(*) FROM persons", [], |r| r.get::<_, i64>(0))
        .and_then(|_| conn.query_row("SELECT COUNT(*) FROM docs", [], |r| r.get::<_, i64>(0)))
        .map_err(|_| "La copia está incompleta o dañada".to_string())?;
    Ok(())
}

/// Replaces the database with the copy at `src`. `conn` is the open
/// connection to `db_path`; it is closed, the files are swapped, and the
/// new connection is returned. What there was before is kept as a copy first.
pub fn restore(conn: Connection, db_path: &Path, src: &Path) -> (Connection, Res<PathBuf>) {
    let safety = copies_dir(db_path).join(format!("Gaudiya Vaisnavas DB antes de restaurar {}.db", stamp(&conn, true)));
    if let Err(x) = copy_to(&conn, &safety) {
        return (conn, Err(format!("No se pudo guardar lo actual antes de restaurar: {x}")));
    }
    // the copy goes next to the database first: if this fails nothing changed
    let incoming = db_path.with_extension("db.restore");
    if let Err(x) = std::fs::copy(src, &incoming) {
        return (conn, Err(e(x)));
    }
    drop(conn);
    for ext in ["db-wal", "db-shm"] {
        let _ = std::fs::remove_file(db_path.with_extension(ext));
    }
    let swapped = std::fs::remove_file(db_path).and_then(|_| std::fs::rename(&incoming, db_path));
    let _ = std::fs::remove_file(&incoming);
    if swapped.is_err() && !db_path.exists() {
        // half done: put back what there was
        let _ = std::fs::copy(&safety, db_path);
    }
    match (db::open(db_path), swapped) {
        (Ok(c), Ok(())) => (c, Ok(safety)),
        (Ok(c), Err(x)) => (c, Err(e(x))),
        (Err(x), _) => (Connection::open_in_memory().expect("memory database"), Err(e(x))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_and_restore() {
        let dir = std::env::temp_dir().join(format!("bio-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("biodata.db");
        let conn = db::open(&db_path).unwrap();
        db::init(&conn).unwrap();
        conn.execute("INSERT INTO persons(name, key) VALUES('Rupa Gosvami', 'rup gosvami')", []).unwrap();
        conn.execute("INSERT INTO docs(collection, dir, title, stem, kind) VALUES('c', 'c', 'Rupa', 'rupa', 'md')", []).unwrap();

        // due the first time, not again right away
        assert!(auto_backup(&db_path).unwrap().is_some());
        assert!(auto_backup(&db_path).unwrap().is_none());
        // pruning keeps the newest automatic copies only
        for d in ["2020-01-01", "2020-01-02", "2020-01-03"] {
            std::fs::write(copies_dir(&db_path).join(format!("{AUTO_PREFIX}{d}.db")), b"x").unwrap();
        }
        prune(&copies_dir(&db_path), 3);
        let autos = list_copies(&db_path).into_iter().filter(|c| c.auto).count();
        assert_eq!(autos, 3);
        assert!(!copies_dir(&db_path).join(format!("{AUTO_PREFIX}2020-01-01.db")).exists());

        let good = list_copies(&db_path).into_iter().find(|c| c.auto && c.size > 1).unwrap();
        assert!(check_copy(Path::new(&good.path)).is_ok());
        assert!(check_copy(&copies_dir(&db_path).join(format!("{AUTO_PREFIX}2020-01-03.db"))).is_err(), "not a database");

        // a change after the copy disappears when the copy is restored
        conn.execute("INSERT INTO persons(name, key) VALUES('Jiva', 'jiv')", []).unwrap();
        let (conn, r) = restore(conn, &db_path, Path::new(&good.path));
        let safety = r.unwrap();
        assert!(safety.exists());
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM persons", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        // the copy made before restoring still has the change
        let old = Connection::open(&safety).unwrap();
        let n: i64 = old.query_row("SELECT COUNT(*) FROM persons", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
        drop(old);
        drop(conn);
        assert!(delete_copy(&db_path, &dir.join("otro.db").to_string_lossy()).is_err(), "not in the list");
        delete_copy(&db_path, &safety.to_string_lossy()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
