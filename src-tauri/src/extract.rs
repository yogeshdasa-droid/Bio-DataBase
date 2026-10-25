//! Turns the supported file types into plain text / markdown for importing.
//! md, markdown and txt are read as they are; pdf, docx, html and old Word /
//! WordPerfect .doc files are converted.

use std::path::Path;

/// PDFs larger than this are imported without text (extraction would be too slow).
const MAX_PDF_BYTES: usize = 120 * 1024 * 1024;

pub fn ext(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

pub fn is_importable(path: &Path) -> bool {
    matches!(ext(path).as_str(), "md" | "markdown" | "txt" | "pdf" | "docx" | "doc" | "html" | "htm")
}

/// Formats whose original file is worth keeping (the text loses their layout).
pub fn keeps_original(path: &Path) -> bool {
    matches!(ext(path).as_str(), "pdf" | "docx" | "doc")
}

pub fn mime_of(name: &str) -> &'static str {
    match ext(Path::new(name)).as_str() {
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "doc" => "application/msword",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// Text of a document, or `None` when the file cannot be read at all.
pub fn read_document(path: &Path, bytes: &[u8]) -> Option<String> {
    let text = match ext(path).as_str() {
        "pdf" => pdf(bytes),
        "docx" => docx(bytes)?,
        "doc" => legacy_doc(bytes),
        "html" | "htm" => html(&crate::text::decode(bytes)),
        "md" | "markdown" => crate::text::strip_callouts(&crate::text::decode(bytes)),
        _ => crate::text::decode(bytes),
    };
    Some(if is_balaram(&text) { balaram_to_iast(&text) } else { text })
}

// ---------------------------------------------------------------- Balaram

/// Old "Balaram" fonts drew IAST letters over Latin-1 ones: "Çré Kåñëa" is
/// "Śrī Kṛṣṇa". Letters like ä, å, ë, ö, ò never appear in English or Spanish,
/// so a text full of them is Balaram (and its ñ is ṣ, not the Spanish ñ).
fn is_balaram(text: &str) -> bool {
    let marks = text.chars().filter(|c| "äåëöòùÄÅËÖÒ".contains(*c)).count();
    marks >= 5 && marks * 400 >= text.chars().filter(|c| c.is_alphabetic()).count()
}

const BALARAM: &[(char, char)] = &[
    ('ä', 'ā'), ('Ä', 'Ā'), ('é', 'ī'), ('É', 'Ī'), ('ü', 'ū'), ('Ü', 'Ū'), ('å', 'ṛ'), ('Å', 'Ṛ'),
    ('ì', 'ṅ'), ('Ì', 'Ṅ'), ('ï', 'ñ'), ('Ï', 'Ñ'), ('ö', 'ṭ'), ('Ö', 'Ṭ'), ('ò', 'ḍ'), ('Ò', 'Ḍ'),
    ('ë', 'ṇ'), ('Ë', 'Ṇ'), ('ç', 'ś'), ('Ç', 'Ś'), ('ñ', 'ṣ'), ('Ñ', 'Ṣ'), ('à', 'ṁ'), ('À', 'Ṁ'),
    ('ù', 'ḥ'), ('Ù', 'Ḥ'), ('è', 'ṝ'), ('ÿ', 'ḷ'),
];

fn balaram_to_iast(text: &str) -> String {
    text.chars()
        .map(|c| BALARAM.iter().find(|(b, _)| *b == c).map_or(c, |(_, i)| *i))
        .collect()
}

fn pdf(bytes: &[u8]) -> String {
    if bytes.len() > MAX_PDF_BYTES {
        return String::new();
    }
    // pdf-extract can panic on unusual fonts: a broken PDF must not stop the import.
    let pages = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem_by_pages(bytes));
    let Ok(Ok(pages)) = pages else { return String::new() };
    let pages: Vec<String> = pages.iter().map(|p| tidy_pdf_page(p)).filter(|p| !p.is_empty()).collect();
    if pages.len() <= 3 {
        // short biographies read better without page headings
        return pages.join("\n\n");
    }
    let mut out = String::new();
    for (i, body) in pages.iter().enumerate() {
        // A heading per page: the reader's contents list becomes a page index.
        out.push_str(&format!("## p. {}\n\n{}\n\n", i + 1, body));
    }
    out
}

/// Joins the hard-wrapped lines of a PDF page back into paragraphs.
fn tidy_pdf_page(page: &str) -> String {
    let mut paras: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in page.lines() {
        let l = line.trim();
        if l.is_empty() {
            if !cur.is_empty() {
                paras.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if cur.ends_with('-') && !cur.ends_with(" -") {
            cur.pop(); // hyphenated word split across lines
        } else if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(l);
    }
    if !cur.is_empty() {
        paras.push(cur);
    }
    paras.join("\n\n")
}

fn docx(bytes: &[u8]) -> Option<String> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut xml = String::new();
    zip.by_name("word/document.xml").ok()?.read_to_string(&mut xml).ok()?;
    let xml = xml
        .replace("</w:p>", "\n\n")
        .replace("<w:tab/>", "\t")
        .replace("<w:br/>", "\n");
    Some(decode_entities(&strip_tags(&xml)).trim().to_string())
}

/// Old Word (2.0–97), Word for DOS and WordPerfect files keep their text
/// mostly as plain characters between binary data: the readable runs are
/// collected and the formatting codes left out.
fn legacy_doc(bytes: &[u8]) -> String {
    let narrow = encoding_rs::WINDOWS_1252.decode(bytes).0;
    let a = readable_runs(&narrow);
    // Word 97 stores text as UTF-16 when it has characters outside Windows-1252.
    let wide = encoding_rs::UTF_16LE.decode(bytes).0;
    let b = readable_runs(&wide);
    if b.len() > a.len() {
        b
    } else {
        a
    }
}

fn readable_runs(s: &str) -> String {
    // Latin script only (with IAST letters and typographic punctuation): misread
    // binary data shows up as other scripts.
    let ok = |c: char| {
        matches!(c, '\n' | '\r' | '\t')
            || ((' '..'\u{7f}').contains(&c)
                || ('\u{a0}'..'\u{180}').contains(&c) && !"ÿþŒœŠšŸŽž".contains(c)
                || ('\u{1e00}'..'\u{1f00}').contains(&c)
                || ('\u{2010}'..'\u{2027}').contains(&c))
    };
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        let norm = run.replace("\r\n", "\n").replace('\r', "\n");
        let lines: Vec<&str> = norm
            .split('\n')
            .map(str::trim_end)
            .filter(|l| {
                let t = l.trim();
                let n = t.chars().count();
                if n == 0 {
                    return true;
                }
                let letters = t.chars().filter(|c| c.is_alphabetic() || *c == ' ').count();
                letters * 100 >= n * 85 && t.chars().any(char::is_alphabetic)
            })
            .collect();
        let letters: usize = lines.iter().map(|l| l.chars().filter(|c| c.is_alphabetic()).count()).sum();
        if letters >= 25 {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(lines.join("\n").trim());
        }
        run.clear();
    };
    for c in s.chars() {
        if ok(c) {
            run.push(c);
        } else if !run.is_empty() {
            flush(&mut run, &mut out);
        }
    }
    flush(&mut run, &mut out);
    // runs of blank lines collapse
    let re = regex::Regex::new(r"\n[ \t]*\n(\s*\n)+").unwrap();
    re.replace_all(&out, "\n\n").to_string()
}

fn html(src: &str) -> String {
    let re_drop = regex::Regex::new(r"(?is)<(script|style|head)\b.*?</(script|style|head)>").unwrap();
    let re_block = regex::Regex::new(r"(?i)<br\s*/?>|</(p|div|li|tr|h[1-6]|blockquote|section|article)>").unwrap();
    let re_heading = regex::Regex::new(r"(?i)<h([1-6])[^>]*>").unwrap();
    let s = re_drop.replace_all(src, "");
    let s = re_heading.replace_all(&s, |c: &regex::Captures| format!("\n\n{} ", "#".repeat(c[1].parse().unwrap_or(2))));
    let s = re_block.replace_all(&s, "\n\n");
    let s = decode_entities(&strip_tags(&s));
    let re_blank = regex::Regex::new(r"\n[ \t]*\n(\s*\n)+").unwrap();
    re_blank.replace_all(s.trim(), "\n\n").to_string()
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

pub fn decode_entities(s: &str) -> String {
    let re = regex::Regex::new(r"&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);").unwrap();
    re.replace_all(s, |c: &regex::Captures| {
        let e = &c[1];
        let ch = if let Some(hex) = e.strip_prefix("#x").or_else(|| e.strip_prefix("#X")) {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(dec) = e.strip_prefix('#') {
            dec.parse().ok().and_then(char::from_u32)
        } else {
            match e {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => None,
            }
        };
        ch.map(String::from).unwrap_or_else(|| c[0].to_string())
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert!(is_importable(Path::new("a/B.PDF")) && is_importable(Path::new("x.DOC")));
        assert!(!is_importable(Path::new("a.bak")));
        let h = html("<html><head><title>x</title></head><body><h2>Śrī Kṛṣṇa</h2><p>uvāca &amp; more</p></body></html>");
        assert_eq!(h, "## Śrī Kṛṣṇa\n\nuvāca & more");
        assert_eq!(tidy_pdf_page("The Lord\nsaid to Ar-\njuna\n\nNext"), "The Lord said to Arjuna\n\nNext");
    }

    #[test]
    fn legacy() {
        let mut bytes = vec![0xD0u8, 0xCF, 0x11, 0xE0, 0xFF, 0xFF, 0x00, 0x01];
        bytes.extend_from_slice(b"Times New Roman\x00\x00\x03");
        bytes.extend_from_slice(b"About Sri Vishnu Priya-devi:\r\nVishnupriya is bhu-shakti. She is also Satyabhama.\r\n");
        bytes.extend_from_slice(&[0xFF; 12]);
        let t = legacy_doc(&bytes);
        assert_eq!(t, "About Sri Vishnu Priya-devi:\nVishnupriya is bhu-shakti. She is also Satyabhama.");
    }

    #[test]
    fn balaram() {
        let b = "Ten Roots of Gauòéya Vaiñëava Philosophy by Çréla Bhaktivinoda Öhäkura: ämnäyaù präha tattvaà, Kåñëa";
        assert!(is_balaram(b));
        assert_eq!(
            balaram_to_iast(b),
            "Ten Roots of Gauḍīya Vaiṣṇava Philosophy by Śrīla Bhaktivinoda Ṭhākura: āmnāyaḥ prāha tattvaṁ, Kṛṣṇa"
        );
        assert!(!is_balaram("El niño y la señora caminaron por la montaña; él sonrió, después habló con Kṛṣṇa."));
    }
}
