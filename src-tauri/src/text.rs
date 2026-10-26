//! Text helpers: decoding, tags, splitting large documents into sections,
//! plain-text extraction and result snippets.

use crate::fold::{fold_word, word_spans};
use regex::Regex;
use std::sync::LazyLock;

// ---------------------------------------------------------------- decoding

pub fn decode(bytes: &[u8]) -> String {
    let text = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        encoding_rs::UTF_16LE.decode(bytes).0.into_owned()
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        encoding_rs::UTF_16BE.decode(bytes).0.into_owned()
    } else {
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_owned(),
            Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
        }
    };
    text.replace("\r\n", "\n").replace('\r', "\n")
}

// ---------------------------------------------------------------- metadata

pub fn clean_title(stem: &str) -> String {
    let t = stem.replace('_', " ");
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        stem.trim().to_string()
    } else {
        t
    }
}

/// A title taken from the first line of an old Word file: without the stray
/// byte those files put before it ("ÃTHE ACCEPTANCE…" → "THE ACCEPTANCE…"),
/// and in IAST when it was typed with a Balaram font ("Çréla" → "Śrīla").
pub fn tidy_doc_title(title: &str) -> String {
    let t = title.trim_start_matches(|c: char| c.is_whitespace() || (('\u{80}'..='\u{FF}').contains(&c) && !crate::extract::is_balaram_letter(c)));
    // ä å ë ö ò ù are never English: the title is Balaram
    let t = if t.chars().any(|c| "äåëöòù".contains(c)) { crate::extract::balaram_to_iast(t) } else { t.to_string() };
    t.trim().to_string()
}

static RE_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[\s(,])#(\p{L}[\p{L}\p{N}_/\-]*)").unwrap()
});

pub fn extract_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for c in RE_TAG.captures_iter(text) {
        let t = c[1].trim_end_matches(['-', '/']).to_string();
        if !tags.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
            tags.push(t);
            if tags.len() >= 25 {
                break;
            }
        }
    }
    tags
}

// ---------------------------------------------------------------- plain text

static RE_EMBED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[\[[^\]]*\]\]|!\[[^\]]*\]\([^)]*\)").unwrap());
static RE_WIKILINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[(?:[^\]|]*\|)?([^\]]*)\]\]").unwrap());
static RE_MDLINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
static RE_CALLOUT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[![A-Za-z-]+\][+-]?").unwrap());
static RE_SYNTAX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*(?:#{1,6}\s+|>\s*|[-*+]\s+|\d+\.\s+)|[*_`~]{1,3}|<[^>]+>").unwrap());
static RE_BLOCK_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)[ \t]\^[A-Za-z0-9-]{3,}[ \t]*$").unwrap());
static RE_WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Markdown/Obsidian markup removed, whitespace collapsed.
pub fn plain(text: &str) -> String {
    let t = RE_BLOCK_ID.replace_all(text, "");
    let t = RE_EMBED.replace_all(&t, " ");
    let t = RE_WIKILINK.replace_all(&t, "$1");
    let t = RE_MDLINK.replace_all(&t, "$1");
    let t = RE_CALLOUT.replace_all(&t, "");
    let t = RE_SYNTAX.replace_all(&t, " ");
    RE_WS.replace_all(&t, " ").trim().to_string()
}

static RE_CALLOUT_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^[ \t]*>[ \t]*\[![A-Za-z-]+\][+-]?[ \t]*(.*?)[ \t]*$").unwrap());
static RE_QUOTE_MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*(?:>[ \t]?)+").unwrap());
static RE_BLANKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n(?:[ \t]*\n){2,}").unwrap());

/// Obsidian notes wrap everything in a callout ("> [!example] NAME", then
/// "> text" lines). The callout title becomes a "## NAME" heading and the
/// quote marks go away, so the text reads plainly.
pub fn strip_callouts(text: &str) -> String {
    let t = RE_CALLOUT_LINE.replace_all(text, |c: &regex::Captures| {
        if c[1].is_empty() { String::new() } else { format!("## {}", &c[1]) }
    });
    let t = RE_QUOTE_MARK.replace_all(&t, "");
    RE_BLANKS.replace_all(&t, "\n\n").trim().to_string()
}

// ---------------------------------------------------------------- sections

pub struct Section {
    pub heading: Option<String>,
    pub text: String,
}

const SINGLE_MAX: usize = 24_000;
const TARGET: usize = 6_000;
const HARD: usize = 16_000;

fn is_heading_like(line: &str, prev_blank: bool) -> bool {
    let t = line.trim();
    if t.starts_with('#') && t.trim_start_matches('#').starts_with(' ') {
        return true;
    }
    if !prev_blank {
        return false;
    }
    let n = t.chars().count();
    (2..=70).contains(&n)
        && t.chars().next().is_some_and(|c| c.is_alphanumeric())
        && !t.ends_with(['.', ',', ';', ':', '?', '!', '"', '”', ')', '-'])
}

fn heading_of(text: &str) -> Option<String> {
    let line = text.lines().find(|l| !l.trim().is_empty())?;
    let p = plain(line);
    if p.is_empty() {
        return None;
    }
    let mut h: String = p.chars().take(90).collect();
    if p.chars().count() > 90 {
        h.push('…');
    }
    Some(h)
}

/// Small files are one section. Large ones (whole books exported from
/// VedaBase, letters, lectures...) are cut every few KB at a heading-like line
/// so search results and the reader point at the right place.
pub fn split_sections(text: &str) -> Vec<Section> {
    if text.len() <= SINGLE_MAX {
        return vec![Section { heading: None, text: text.trim().to_string() }];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_blank = true;
    for line in text.split('\n') {
        let blank = line.trim().is_empty();
        if !cur.is_empty()
            && ((cur.len() >= TARGET && is_heading_like(line, prev_blank)) || (cur.len() >= HARD && blank))
        {
            out.push(Section { heading: heading_of(&cur), text: std::mem::take(&mut cur) });
        }
        prev_blank = blank;
        if cur.is_empty() && blank {
            continue;
        }
        cur.push_str(line);
        cur.push('\n');
    }
    if !cur.trim().is_empty() {
        out.push(Section { heading: heading_of(&cur), text: cur });
    }
    out
}

// ---------------------------------------------------------------- snippets

/// What to highlight: a single word, or a quoted phrase whose words must
/// appear one after another. `prefix` applies to the last word.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Term {
    pub words: Vec<String>,
    pub prefix: bool,
}

/// Non-overlapping matches over a list of folded words, as
/// (index of first word, number of words). Longer phrases win.
pub fn find_matches(folded: &[String], terms: &[Term]) -> Vec<(usize, usize)> {
    let mut sorted: Vec<&Term> = terms.iter().filter(|t| !t.words.is_empty()).collect();
    sorted.sort_by(|a, b| b.words.len().cmp(&a.words.len()));
    let mut out = Vec::new();
    let mut i = 0;
    'outer: while i < folded.len() {
        for t in &sorted {
            let n = t.words.len();
            if i + n > folded.len() {
                continue;
            }
            let ok = t.words.iter().enumerate().all(|(k, w)| {
                let f = &folded[i + k];
                if t.prefix && k == n - 1 {
                    f.starts_with(w.as_str())
                } else {
                    f == w
                }
            });
            if ok {
                out.push((i, n));
                i += n;
                continue 'outer;
            }
        }
        i += 1;
    }
    out
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// HTML snippet (escaped, matches wrapped in `<mark>`) around the densest
/// cluster of matches, plus the number of matches (a phrase counts once).
pub fn snippet(text: &str, terms: &[Term], max_words: usize) -> (String, usize) {
    let p = plain(text);
    let spans = word_spans(&p);
    if spans.is_empty() {
        return (String::new(), 0);
    }
    let hits: Vec<(usize, usize)> = if terms.is_empty() {
        Vec::new()
    } else {
        let folded: Vec<String> = spans.iter().map(|(s, e)| fold_word(&p[*s..*e])).collect();
        find_matches(&folded, terms)
    };

    // Window of `max_words` words containing the most hits.
    let mut start = 0usize;
    if !hits.is_empty() {
        let mut best = (0usize, hits[0].0);
        let mut j = 0;
        for i in 0..hits.len() {
            while j < hits.len() && hits[j].0 < hits[i].0 + max_words {
                j += 1;
            }
            if j - i > best.0 {
                best = (j - i, hits[i].0);
            }
        }
        start = best.1.saturating_sub(max_words / 4);
    }
    let mut end = (start + max_words).min(spans.len());
    // never cut a phrase in half
    if let Some(&(h, n)) = hits.iter().find(|(h, n)| *h < end && h + n > end) {
        end = (h + n).min(spans.len());
    }
    let from = if start == 0 { 0 } else { spans[start].0 };
    let to = if end == spans.len() { p.len() } else { spans[end - 1].1 };

    let mut html = String::new();
    if start > 0 {
        html.push_str("… ");
    }
    let mut pos = from;
    for &(h, n) in hits.iter().filter(|(h, _)| *h >= start && *h < end) {
        let (s, e) = (spans[h].0, spans[h + n - 1].1);
        escape(&p[pos..s], &mut html);
        html.push_str("<mark>");
        escape(&p[s..e], &mut html);
        html.push_str("</mark>");
        pos = e;
    }
    escape(&p[pos..to], &mut html);
    if end < spans.len() {
        html.push_str(" …");
    }
    (html, hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callouts() {
        let note = "> [!example] DURGA DASA\n> \n>He was a disciple of Srinivasa.\n>\n>\n> > nested\nplain > not a quote";
        assert_eq!(strip_callouts(note), "## DURGA DASA\n\nHe was a disciple of Srinivasa.\n\nnested\nplain > not a quote");
        assert_eq!(strip_callouts("> [!note]\n> text"), "text");
        assert_eq!(strip_callouts("no callout here"), "no callout here");
    }

    #[test]
    fn snippets() {
        let terms = vec![Term { words: vec!["uvaca".into()], prefix: false }];
        let (s, n) = snippet("śrī-bhagavān uvāca — the Lord said", &terms, 20);
        assert_eq!(n, 1);
        assert!(s.contains("<mark>uvāca</mark>"));

        // a quoted phrase is highlighted as a whole, and only where the words are together
        let phrase = vec![Term { words: vec!["jaiva".into(), "dharma".into()], prefix: false }];
        let (s, n) = snippet("Jaiva-dharma is a book. Jaiva alone, dharma alone.", &phrase, 20);
        assert_eq!(n, 1);
        assert_eq!(s, "<mark>Jaiva-dharma</mark> is a book. Jaiva alone, dharma alone.");
    }
}
