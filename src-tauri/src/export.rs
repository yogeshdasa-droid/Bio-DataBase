//! Everything out as Markdown: one file per record and one per text, in
//! folders that Obsidian (or any editor) can open; and a single record with
//! all its texts, to share or print.

use crate::search;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;

type Res<T> = Result<T, String>;

fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}

/// A name that Windows accepts as a file name.
fn safe(name: &str) -> String {
    let s: String = name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '-' } else { c }).collect();
    let s: String = s.trim().trim_end_matches('.').chars().take(120).collect();
    if s.is_empty() { "sin título".into() } else { s }
}

/// The record's fields as Markdown (heading, a list of facts, notes).
fn person_head(p: &search::Person, labels: &Labels) -> String {
    let f = &p.fields;
    let mut out = format!("# {}\n\n", f.name);
    for (label, value) in [
        (labels.aka, &f.aka),
        (labels.appearance, &f.appearance),
        (labels.disappearance, &f.disappearance),
        (labels.place, &f.place),
        (labels.guru, &f.guru),
        (labels.disciples, &f.disciples),
        (labels.works, &f.works),
        (labels.tags, &f.tags),
    ] {
        if !value.trim().is_empty() {
            out.push_str(&format!("- **{label}:** {}\n", value.trim()));
        }
    }
    if !f.notes.trim().is_empty() {
        out.push_str(&format!("\n## {}\n\n{}\n", labels.notes, f.notes.trim()));
    }
    out
}

pub struct Labels {
    pub aka: &'static str,
    pub appearance: &'static str,
    pub disappearance: &'static str,
    pub place: &'static str,
    pub guru: &'static str,
    pub disciples: &'static str,
    pub works: &'static str,
    pub tags: &'static str,
    pub notes: &'static str,
    pub texts: &'static str,
}

pub const ES: Labels = Labels {
    aka: "Otros nombres",
    appearance: "Aparición",
    disappearance: "Desaparición",
    place: "Lugar",
    guru: "Guru",
    disciples: "Discípulos",
    works: "Obras",
    tags: "Etiquetas",
    notes: "Notas",
    texts: "Textos",
};

/// One record with the whole of its texts, in a single Markdown file.
pub fn export_person(conn: &Connection, id: i64, path: &Path) -> Res<()> {
    let p = search::get_person(conn, id).map_err(e)?;
    let mut out = person_head(&p, &ES);
    for d in &p.docs {
        let body = search::full_text(conn, d.id).map_err(e)?;
        out.push_str(&format!("\n---\n\n## {}\n\n*{}*\n\n{}\n", d.title, d.collection, body.trim()));
    }
    std::fs::write(path, out).map_err(e)
}

/// Everything into `dir`: `Fichas/<name>.md` (the record and links to its
/// texts) and `Textos/<collection>/<title>.md`. Returns (records, texts).
pub fn export_all(conn: &Connection, dir: &Path, progress: impl Fn(usize, usize)) -> Res<(usize, usize)> {
    let docs: Vec<(i64, String, String, Option<String>)> = conn
        .prepare(
            "SELECT d.id, d.title, d.collection, p.name FROM docs d LEFT JOIN persons p ON p.id = d.person_id
             WHERE d.deleted IS NULL ORDER BY d.collection, d.title",
        )
        .map_err(e)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;
    let persons: Vec<i64> = conn
        .prepare("SELECT id FROM persons ORDER BY name")
        .map_err(e)?
        .query_map([], |r| r.get(0))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;
    let total = docs.len() + persons.len();
    // the file name of each text, for the links ([[name]] as in Obsidian)
    let mut link: HashMap<i64, String> = HashMap::new();
    let mut used: HashMap<String, usize> = HashMap::new();
    for (i, (id, title, collection, person)) in docs.iter().enumerate() {
        let folder = dir.join("Textos").join(safe(collection));
        std::fs::create_dir_all(&folder).map_err(e)?;
        let mut stem = safe(title);
        let n = used.entry(stem.to_lowercase()).or_insert(0);
        *n += 1;
        if *n > 1 {
            stem = format!("{stem} ({n})");
        }
        let body = search::full_text(conn, *id).map_err(e)?;
        let front = match person {
            Some(p) => format!("---\nficha: \"[[Fichas/{}|{p}]]\"\ncolección: {collection}\n---\n\n", safe(p)),
            None => format!("---\ncolección: {collection}\n---\n\n"),
        };
        std::fs::write(folder.join(format!("{stem}.md")), format!("{front}# {title}\n\n{}\n", body.trim())).map_err(e)?;
        // with its folder: a text may have the same name as a record
        link.insert(*id, format!("Textos/{}/{stem}|{title}", safe(collection)));
        if i % 50 == 0 {
            progress(i, total);
        }
    }
    let fichas = dir.join("Fichas");
    std::fs::create_dir_all(&fichas).map_err(e)?;
    for (i, id) in persons.iter().enumerate() {
        let p = search::get_person(conn, *id).map_err(e)?;
        let mut out = person_head(&p, &ES);
        if !p.docs.is_empty() {
            out.push_str(&format!("\n## {}\n\n", ES.texts));
            for d in &p.docs {
                if let Some(l) = link.get(&d.id) {
                    out.push_str(&format!("- [[{l}]]\n"));
                }
            }
        }
        std::fs::write(fichas.join(format!("{}.md", safe(&p.fields.name))), out).map_err(e)?;
        if i % 50 == 0 {
            progress(docs.len() + i, total);
        }
    }
    progress(total, total);
    Ok((persons.len(), docs.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_out() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let p = crate::store::new_person(
            &mut conn,
            &crate::store::PersonFields { name: "Jiva Gosvami".into(), guru: "Rupa Gosvami".into(), ..Default::default() },
        )
        .unwrap();
        let d = crate::store::create_doc(&mut conn, Some(p), "Life: of Jiva?", "Mis textos").unwrap();
        crate::store::save_section(&mut conn, d, 0, "He wrote the Sat-sandarbha.").unwrap();
        let dir = std::env::temp_dir().join(format!("bio-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(export_all(&conn, &dir, |_, _| {}).unwrap(), (1, 1));
        let text = std::fs::read_to_string(dir.join("Textos/Mis textos/Life- of Jiva-.md")).unwrap();
        assert!(text.contains("ficha: \"[[Fichas/Jiva Gosvami|Jiva Gosvami]]\"") && text.contains("Sat-sandarbha"));
        let card = std::fs::read_to_string(dir.join("Fichas/Jiva Gosvami.md")).unwrap();
        assert!(card.contains("- **Guru:** Rupa Gosvami") && card.contains("[[Textos/Mis textos/Life- of Jiva-|Life: of Jiva?]]"));
        export_person(&conn, p, &dir.join("one.md")).unwrap();
        assert!(std::fs::read_to_string(dir.join("one.md")).unwrap().contains("## Life: of Jiva?"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
