//! The Vaiṣṇava (Gaurabda) lunar calendar: the tithi and month of any day,
//! worked out from the positions of the Sun and the Moon at sunrise in
//! Śrī Māyāpur, and the appearance / disappearance days written in the
//! records ("Phalguna Purnima", "Bhadra Krsna Astami", "8 October 1898").
//!
//! Positions follow Meeus, *Astronomical Algorithms* (low-precision Sun,
//! main terms of the Moon): good to a few minutes for the tithi changes,
//! so a day can rarely differ from the printed calendars.

use crate::fold::fold_text;
use regex::Regex;
use serde::Serialize;
use std::f64::consts::PI;
use std::sync::OnceLock;

/// Śrī Māyāpur.
const LAT: f64 = 23.4233;
const LON: f64 = 88.3883;

/// Vaiṣṇava months, from the one ending on Caitra Pūrṇimā; 12 = the extra month.
pub const MONTHS: [&str; 13] = [
    "Viṣṇu", "Madhusūdana", "Trivikrama", "Vāmana", "Śrīdhara", "Hṛṣīkeśa", "Padmanābha", "Dāmodara", "Keśava",
    "Nārāyaṇa", "Mādhava", "Govinda", "Puruṣottama",
];
/// The same months by their usual (pūrṇimānta) names.
const MONTH_STEMS: [&[&str]; 12] = [
    &["caitr"],
    &["vaisak", "baisak"],
    &["jyaist", "jyest", "jaist", "jyast"],
    &["asad", "asar"],
    &["sravan", "srabon"],
    &["bhadr"],
    &["asvin", "asvayuj"],
    &["kartik"],
    &["margasir", "agrahayan", "agrahan"],
    &["paus", "pous"],
    &["magh"],
    &["phalgun", "falgun"],
];
const VAISNAVA_STEMS: [&str; 12] =
    ["visnu", "madhusudan", "trivikram", "vaman", "sridhar", "hrsikes", "padmanabh", "damodar", "kesav", "narayan", "madhav", "govind"];
const TITHIS: [&str; 15] = [
    "Pratipat", "Dvitīyā", "Tṛtīyā", "Caturthī", "Pañcamī", "Ṣaṣṭhī", "Saptamī", "Aṣṭamī", "Navamī", "Daśamī", "Ekādaśī",
    "Dvādaśī", "Trayodaśī", "Caturdaśī", "Pūrṇimā",
];
/// (stem, tithi of the bright half) — checked in this order.
const TITHI_STEMS: &[(&str, u8)] = &[
    ("pratipa", 1),
    ("prathama", 1),
    ("dvitiy", 2),
    ("trtiy", 3),
    ("tritiy", 3),
    ("caturdas", 14),
    ("caturth", 4),
    ("pancam", 5),
    ("sasth", 6),
    ("sasti", 6),
    ("saptam", 7),
    ("astam", 8),
    ("navam", 9),
    ("ekadas", 11),
    ("dvadas", 12),
    ("trayodas", 13),
    ("dasam", 10),
    ("purnim", 15),
    ("paurnam", 15),
    ("amavas", 30),
];
/// Days known by their name: (folded words, month, tithi).
const NAMED_DAYS: &[(&str, u8, u8)] = &[
    ("gaura purnim", 11, 15),
    ("utthana ekadas", 7, 11),
    ("rama navam", 0, 9),
    ("nrsimha caturdas", 1, 14),
    ("janmastam", 5, 23),
    ("radhastam", 5, 8),
    ("rasa purnim", 7, 15),
];

// ---------------------------------------------------------------- astronomy

fn norm(deg: f64) -> f64 {
    deg.rem_euclid(360.0)
}
fn sin(deg: f64) -> f64 {
    (deg * PI / 180.0).sin()
}
fn cos(deg: f64) -> f64 {
    (deg * PI / 180.0).cos()
}

/// Julian day at 0h UT of a Gregorian date.
pub fn jd(y: i32, m: u32, d: u32) -> f64 {
    let (y, m) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let a = (y as f64 / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    (365.25 * (y as f64 + 4716.0)).floor() + (30.6001 * (m as f64 + 1.0)).floor() + d as f64 + b - 1524.5
}

/// Gregorian date of a Julian day (the civil day it falls in, at UT).
fn ymd(jd: f64) -> (i32, u32, u32) {
    let z = (jd + 0.5).floor();
    let alpha = ((z - 1867216.25) / 36524.25).floor();
    let a = z + 1.0 + alpha - (alpha / 4.0).floor();
    let b = a + 1524.0;
    let c = ((b - 122.1) / 365.25).floor();
    let d = (365.25 * c).floor();
    let e = ((b - d) / 30.6001).floor();
    let day = b - d - (30.6001 * e).floor();
    let month = if e < 14.0 { e - 1.0 } else { e - 13.0 };
    let year = if month > 2.0 { c - 4716.0 } else { c - 4715.0 };
    (year as i32, month as u32, day as u32)
}

fn centuries(jd: f64) -> f64 {
    (jd - 2451545.0) / 36525.0
}

/// Apparent longitude of the Sun (degrees, tropical).
fn sun_lon(jd: f64) -> f64 {
    let t = centuries(jd);
    let l0 = 280.46646 + 36000.76983 * t + 0.0003032 * t * t;
    let m = 357.52911 + 35999.05029 * t - 0.0001537 * t * t;
    let c = (1.914602 - 0.004817 * t - 0.000014 * t * t) * sin(m) + (0.019993 - 0.000101 * t) * sin(2.0 * m) + 0.000289 * sin(3.0 * m);
    let omega = 125.04 - 1934.136 * t;
    norm(l0 + c - 0.00569 - 0.00478 * sin(omega))
}

/// Longitude of the Moon (degrees, tropical), main periodic terms.
fn moon_lon(jd: f64) -> f64 {
    let t = centuries(jd);
    let lp = 218.3164477 + 481267.88123421 * t - 0.0015786 * t * t;
    let d = 297.8501921 + 445267.1114034 * t - 0.0018819 * t * t;
    let m = 357.5291092 + 35999.0502909 * t - 0.0001536 * t * t;
    let mp = 134.9633964 + 477198.8675055 * t + 0.0087414 * t * t;
    let f = 93.2720950 + 483202.0175233 * t - 0.0036539 * t * t;
    let e = 1.0 - 0.002516 * t - 0.0000074 * t * t;
    // (D, M, M', F, coefficient in millionths of a degree)
    const TERMS: &[(f64, f64, f64, f64, f64)] = &[
        (0., 0., 1., 0., 6288774.),
        (2., 0., -1., 0., 1274027.),
        (2., 0., 0., 0., 658314.),
        (0., 0., 2., 0., 213618.),
        (0., 1., 0., 0., -185116.),
        (0., 0., 0., 2., -114332.),
        (2., 0., -2., 0., 58793.),
        (2., -1., -1., 0., 57066.),
        (2., 0., 1., 0., 53322.),
        (2., -1., 0., 0., 45758.),
        (0., 1., -1., 0., -40923.),
        (1., 0., 0., 0., -34720.),
        (0., 1., 1., 0., -30383.),
        (2., 0., 0., -2., 15327.),
        (0., 0., 1., 2., -12528.),
        (0., 0., 1., -2., 10980.),
        (4., 0., -1., 0., 10675.),
        (0., 0., 3., 0., 10034.),
        (4., 0., -2., 0., 8548.),
        (2., 1., -1., 0., -7888.),
        (2., 1., 0., 0., -6766.),
        (1., 0., -1., 0., -5163.),
        (1., 1., 0., 0., 4987.),
        (2., -1., 1., 0., 4036.),
        (2., 0., 2., 0., 3994.),
        (4., 0., 0., 0., 3861.),
        (2., 0., -3., 0., 3665.),
        (0., 1., -2., 0., -2689.),
        (2., 0., -1., 2., -2602.),
        (2., -1., -2., 0., 2390.),
        (1., 0., 1., 0., -2348.),
        (2., -2., 0., 0., 2236.),
        (0., 1., 2., 0., -2120.),
        (0., 2., 0., 0., -2069.),
    ];
    let mut sum = 0.0;
    for &(cd, cm, cmp, cf, coef) in TERMS {
        let ecorr = match cm.abs() as i32 {
            1 => e,
            2 => e * e,
            _ => 1.0,
        };
        sum += coef * ecorr * sin(cd * d + cm * m + cmp * mp + cf * f);
    }
    let a1 = 119.75 + 131.849 * t;
    let a2 = 53.09 + 479264.290 * t;
    sum += 3958.0 * sin(a1) + 1962.0 * sin(lp - f) + 318.0 * sin(a2);
    norm(lp + sum / 1_000_000.0)
}

/// Moon ahead of the Sun (0–360°): each 12° is one tithi.
fn elongation(jd: f64) -> f64 {
    norm(moon_lon(jd) - sun_lon(jd))
}

/// Tithi at an instant: 1–15 bright half (15 = Pūrṇimā), 16–30 dark half (30 = Amāvasyā).
fn tithi_at(jd: f64) -> u8 {
    (elongation(jd) / 12.0).floor() as u8 + 1
}

/// Sunrise in Māyāpur on the civil day starting at `jd0` (0h UT), as a Julian day.
fn sunrise(jd0: f64) -> f64 {
    // first guess 6:00 local = 0:30 UT; two rounds are plenty
    let mut t = jd0 + 0.5 / 24.0;
    for _ in 0..2 {
        let lambda = sun_lon(t);
        let eps = 23.439 - 0.0000004 * (t - 2451545.0);
        let decl = (sin(eps) * sin(lambda)).asin() * 180.0 / PI;
        let ra = norm((cos(eps) * sin(lambda)).atan2(cos(lambda)) * 180.0 / PI);
        let l0 = norm(280.46646 + 36000.76983 * centuries(t));
        let mut eot = l0 - 0.0057183 - ra; // degrees
        eot = (eot + 180.0).rem_euclid(360.0) - 180.0;
        let cos_h = (sin(-0.833) - sin(LAT) * sin(decl)) / (cos(LAT) * cos(decl));
        let h = cos_h.clamp(-1.0, 1.0).acos() * 180.0 / PI;
        let noon_min = 720.0 - 4.0 * LON - 4.0 * eot;
        t = jd0 + (noon_min - 4.0 * h) / 1440.0;
    }
    t
}

const SYNODIC: f64 = 29.530589;

/// The last new moon at or before `jd`.
fn new_moon_before(jd: f64) -> f64 {
    let mut t = jd - elongation(jd) / 360.0 * SYNODIC;
    for _ in 0..6 {
        let e = (elongation(t) + 180.0).rem_euclid(360.0) - 180.0;
        t -= e / 360.0 * SYNODIC;
    }
    if t > jd + 1e-6 {
        new_moon_before(jd - 3.0)
    } else {
        t
    }
}

/// Lahiri ayanāṁśa (degrees), enough to find the sign the Sun is in.
fn ayanamsa(jd: f64) -> f64 {
    23.857 + (jd - 2451545.0) / 365.25 * 0.013969
}

/// Sidereal sign of the Sun: 0 = Meṣa … 11 = Mīna.
fn sun_sign(jd: f64) -> usize {
    (norm(sun_lon(jd) - ayanamsa(jd)) / 30.0) as usize
}

/// The lunar month that starts with the new moon `nm`: (Caitra = 0, extra month).
fn lunation(nm: f64) -> (usize, bool) {
    let next = new_moon_before(nm + 32.0);
    let (a, b) = (sun_sign(nm), sun_sign(next));
    ((a + 1) % 12, a == b)
}

/// Vaiṣṇava month and tithi of a moment at sunrise.
fn month_tithi(at: f64) -> (u8, u8) {
    let tithi = tithi_at(at);
    let nm = new_moon_before(at);
    // the dark half belongs to the month ending with the next full moon
    let lun = if tithi <= 15 { nm } else { new_moon_before(nm + 32.0) };
    let (m, extra) = lunation(lun);
    (if extra { 12 } else { m as u8 }, tithi)
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct LunarDay {
    /// 0–11 (Viṣṇu … Govinda), 12 = Puruṣottama
    pub month: u8,
    /// 1–30
    pub tithi: u8,
}

/// The tithi(s) of a civil day: the one at sunrise in Māyāpur, plus a tithi
/// that starts and ends before the next sunrise (it is kept on this day).
pub fn day(y: i32, m: u32, d: u32) -> Vec<LunarDay> {
    let j = jd(y, m, d);
    let (month, tithi) = month_tithi(sunrise(j));
    let mut out = vec![LunarDay { month, tithi }];
    let (next_month, next_tithi) = month_tithi(sunrise(j + 1.0));
    let skipped = tithi % 30 + 1;
    if next_tithi != skipped && next_tithi != tithi {
        out.push(LunarDay { month: if skipped == 16 { next_month } else { month }, tithi: skipped });
    }
    out
}

pub fn label(d: LunarDay) -> String {
    let t = match d.tithi {
        15 => TITHIS[14].to_string(),
        30 => "Amāvasyā".to_string(),
        n if n < 15 => format!("Gaura {}", TITHIS[n as usize - 1]),
        n => format!("Kṛṣṇa {}", TITHIS[n as usize - 16]),
    };
    format!("{} · {t}", MONTHS[d.month as usize])
}

// ---------------------------------------------------------------- days in the records

fn gregorian_re() -> &'static [Regex; 3] {
    static RE: OnceLock<[Regex; 3]> = OnceLock::new();
    RE.get_or_init(|| {
        let en = "(january|february|march|april|may|june|july|august|september|october|november|december)";
        let es = "(enero|febrero|marzo|abril|mayo|junio|julio|agosto|septiembre|setiembre|octubre|noviembre|diciembre)";
        [
            Regex::new(&format!(r"(?i)\b(\d{{1,2}})\s+{en},?\s+(\d{{4}})\b")).unwrap(),
            Regex::new(&format!(r"(?i)\b{en}\s+(\d{{1,2}}),?\s+(\d{{4}})\b")).unwrap(),
            Regex::new(&format!(r"(?i)\b(\d{{1,2}})\s+de\s+{es}\s+(?:de\s+)?(\d{{4}})\b")).unwrap(),
        ]
    })
}

fn month_number(name: &str) -> u32 {
    let n = name.to_lowercase();
    let en = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let es = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];
    en.iter().chain(es.iter()).position(|p| n.starts_with(p)).map(|i| (i % 12) as u32 + 1).unwrap_or(0) + if n.starts_with("set") { 9 } else { 0 }
}

/// A modern date written in the text ("8 October 1898", "Sept 2, 1838", "2 de septiembre de 1838").
/// Only from 1752 on: older dates in the texts are often in the Julian calendar.
fn gregorian(text: &str) -> Option<(i32, u32, u32)> {
    let [a, b, c] = gregorian_re();
    let (d, m, y) = if let Some(x) = a.captures(text) {
        (x[1].parse().ok()?, month_number(&x[2]), x[3].parse().ok()?)
    } else if let Some(x) = b.captures(text) {
        (x[2].parse().ok()?, month_number(&x[1]), x[3].parse().ok()?)
    } else if let Some(x) = c.captures(text) {
        (x[1].parse().ok()?, month_number(&x[2]), x[3].parse().ok()?)
    } else {
        return None;
    };
    ((1752..=2200).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d)).then_some((y, m, d))
}

/// The lunar day written in a field: a month and tithi by name, a day known
/// by its name (Gaura Pūrṇimā, Utthāna Ekādaśī…), or a modern date.
pub fn parse_day(text: &str) -> Option<LunarDay> {
    let folded = fold_text(text);
    for &(name, month, tithi) in NAMED_DAYS {
        if folded.contains(name) {
            return Some(LunarDay { month, tithi });
        }
    }
    let words: Vec<&str> = folded.split(' ').filter(|w| !w.is_empty()).collect();
    for (i, w) in words.iter().enumerate() {
        let Some(&(_, base)) = TITHI_STEMS.iter().find(|(s, _)| w.starts_with(s)) else { continue };
        let from = i.saturating_sub(3);
        let near = &words[from..(i + 3).min(words.len())];
        let month = near.iter().enumerate().find_map(|(k, n)| {
            MONTH_STEMS.iter().position(|stems| stems.iter().any(|s| n.starts_with(s))).or_else(|| {
                // Vaiṣṇava month names are also people's names: only before the tithi
                (from + k < i).then(|| VAISNAVA_STEMS.iter().position(|s| n.starts_with(s))).flatten()
            })
        });
        let Some(month) = month else { continue };
        let tithi = if base >= 15 {
            base
        } else if near.iter().any(|n| n.starts_with("krsn") || ["badi", "vadi", "dark", "waning", "oscur"].iter().any(|k| n.starts_with(k))) {
            base + 15
        } else if near.iter().any(|n| ["sukl", "sudi", "suddh", "gaura", "bright", "waxing", "clar"].iter().any(|k| n.starts_with(k))) {
            base
        } else {
            continue;
        };
        return Some(LunarDay { month: month as u8, tithi });
    }
    let (y, m, d) = gregorian(text)?;
    Some(day(y, m, d)[0])
}

/// Year of birth / passing written in a field, in the common era: "(1486)",
/// "1407 Saka" (+78), or the first plausible year.
pub fn year_of(text: &str) -> Option<i32> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?i)\b(\d{3,4})\b(\s*(?:saka|śaka|sakabda|śakābda))?").unwrap());
    let mut first = None;
    for c in re.captures_iter(text) {
        let n: i32 = c[1].parse().ok()?;
        let saka = c.get(2).is_some();
        let ad = if saka { n + 78 } else { n };
        if !(400..=2100).contains(&ad) {
            continue;
        }
        // an explicit common-era year wins over a Śaka one
        if !saka {
            return Some(ad);
        }
        first.get_or_insert(ad);
    }
    first
}

// ---------------------------------------------------------------- the user's calendar (.ics)

pub struct IcsEvent {
    pub uid: String,
    /// YYYY-MM-DD
    pub date: String,
    pub summary: String,
    pub description: String,
}

/// The all-day events of an iCalendar file (as exported by calendar apps).
pub fn parse_ics(text: &str) -> Vec<IcsEvent> {
    // long lines go on in the next line after a space ("folding")
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let l = raw.trim_end_matches('\r');
        match lines.last_mut() {
            Some(last) if l.starts_with(' ') || l.starts_with('\t') => last.push_str(&l[1..]),
            _ => lines.push(l.to_string()),
        }
    }
    let unescape = |s: &str| s.replace("\\n", "\n").replace("\\N", "\n").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\");
    let mut out = Vec::new();
    let mut cur: Option<IcsEvent> = None;
    for l in &lines {
        if l == "BEGIN:VEVENT" {
            cur = Some(IcsEvent { uid: String::new(), date: String::new(), summary: String::new(), description: String::new() });
            continue;
        }
        if l == "END:VEVENT" {
            if let Some(e) = cur.take().filter(|e| e.date.len() == 10 && !e.summary.is_empty()) {
                out.push(e);
            }
            continue;
        }
        let (Some(e), Some((key, value))) = (cur.as_mut(), l.split_once(':')) else { continue };
        match key.split(';').next().unwrap_or("") {
            "UID" => e.uid = value.trim().to_string(),
            "SUMMARY" => e.summary = unescape(value).trim().to_string(),
            "DESCRIPTION" => e.description = unescape(value).trim().to_string(),
            "DTSTART" => {
                let digits: String = value.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
                if digits.len() == 8 {
                    e.date = format!("{}-{}-{}", &digits[..4], &digits[4..6], &digits[6..8]);
                }
            }
            _ => {}
        }
    }
    for e in &mut out {
        if e.uid.is_empty() {
            e.uid = format!("{}-{}", e.date, e.summary);
        }
    }
    out
}

/// Replaces the imported calendar with the events of `text`.
pub fn import_ics(conn: &mut rusqlite::Connection, text: &str) -> Result<usize, String> {
    let events = parse_ics(text);
    if events.is_empty() {
        return Err("El archivo no tiene eventos de calendario (.ics)".into());
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM calendar_events", []).map_err(|e| e.to_string())?;
    for e in &events {
        tx.execute(
            "INSERT OR REPLACE INTO calendar_events(uid, date, summary, description) VALUES(?1, ?2, ?3, ?4)",
            rusqlite::params![e.uid, e.date, e.summary, e.description],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(events.len())
}

/// Honorifics before a name in the calendar: "Nitya-līlā praviṣṭa oṁ
/// viṣṇupāda aṣṭottara-śata-śrī Śrīmad …" — everything up to the last one goes.
const EVENT_HONORIFICS: &[&str] = &[
    "sri", "srila", "srimad", "srimati", "sripad", "sripada", "om", "visnupad", "visnupada", "astottara", "astottar", "sata",
    "nitya", "lila", "pravista", "pravist", "pujyapada", "pujyapad", "his", "divine", "grace",
];
/// Titles at the end that the records may or may not have.
const TRAILING_TITLES: &[&str] = &["gosvami", "gosvamin", "prabhu", "thakur", "maharaj", "mahasay", "prabhupad", "pandit"];

/// (kind, name) of a calendar event about a person: "Āvirbhāva of Śrī X".
fn event_person(summary: &str) -> Option<(&'static str, String)> {
    let folded = fold_text(summary);
    let (kind, rest) = [("avirbhava of ", "appearance"), ("tirobhava of ", "disappearance"), ("appearance of ", "appearance")]
        .iter()
        .find_map(|(p, k)| folded.strip_prefix(p).map(|r| (*k, r)))?;
    let words: Vec<&str> = rest.split(' ').filter(|w| !w.is_empty()).collect();
    let head = words.len().min(12);
    // compared folded: "srimad" folds to "srmad" like the text
    let honorific = |w: &str| EVENT_HONORIFICS.iter().any(|h| crate::fold::fold_word(h) == w);
    let start = words[..head].iter().rposition(|w| honorific(w)).map_or(0, |i| i + 1);
    let name = words[start..].join(" ");
    (!name.is_empty()).then_some((kind, name))
}

/// Words of a name as compared with the records: the record key without the
/// titles at the end ("Gadadhara Pandit Gosvami" ~ "Gadadhara").
fn loose_words(name: &str) -> Vec<String> {
    let key = crate::store::person_key(name);
    let mut words: Vec<String> = key.split(' ').filter(|w| !w.is_empty()).map(String::from).collect();
    while words.len() > 1 && TRAILING_TITLES.contains(&words[words.len() - 1].as_str()) {
        words.pop();
    }
    words
}

/// Finds the record a calendar name belongs to.
pub struct PersonIndex {
    /// (id, loose words) of every name and other name of every record
    names: Vec<(i64, Vec<String>)>,
    /// how much each record weighs when one name fits several (repeated
    /// records): in the Guru Paramparā first, then the one with more texts
    weight: std::collections::HashMap<i64, (bool, i64)>,
}

impl PersonIndex {
    pub fn load(conn: &rusqlite::Connection) -> rusqlite::Result<Self> {
        let mut st = conn.prepare(
            "SELECT p.id, p.name, p.aka, EXISTS(SELECT 1 FROM person_tree t WHERE t.person_id = p.id),
                    (SELECT COUNT(*) FROM docs d WHERE d.person_id = p.id AND d.deleted IS NULL)
             FROM persons p",
        )?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, bool>(3)?, r.get::<_, i64>(4)?))
        })?;
        let mut names = Vec::new();
        let mut weight = std::collections::HashMap::new();
        for row in rows {
            let (id, name, aka, tree, docs) = row?;
            weight.insert(id, (tree, docs));
            for n in std::iter::once(name.as_str()).chain(aka.split(',')).map(str::trim).filter(|n| !n.is_empty()) {
                let w = loose_words(n);
                if !w.is_empty() {
                    names.push((id, w));
                }
            }
        }
        Ok(PersonIndex { names, weight })
    }

    /// The same name, or else one name ending the other ("Saccidānanda
    /// Bhaktivinoda" ~ "Bhaktivinoda Ṭhākura") when only one record fits.
    pub fn find(&self, name: &str) -> Option<i64> {
        let w = loose_words(name);
        if w.is_empty() {
            return None;
        }
        let unique = |ids: Vec<i64>| {
            let mut ids = ids;
            ids.sort();
            ids.dedup();
            (ids.len() == 1).then(|| ids[0])
        };
        let mut exact: Vec<i64> = self.names.iter().filter(|(_, n)| *n == w).map(|(id, _)| *id).collect();
        if !exact.is_empty() {
            // the very same name on several records: repeated records of one person
            exact.sort();
            exact.dedup();
            return exact.into_iter().max_by_key(|id| self.weight.get(id).copied().unwrap_or_default());
        }
        let ends = |long: &[String], short: &[String]| {
            short.len() < long.len() && long.ends_with(short) && (short.len() >= 2 || short[0].len() >= 7)
        };
        unique(self.names.iter().filter(|(_, n)| ends(n, &w) || ends(&w, n)).map(|(id, _)| *id).collect())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarInfo {
    pub count: i64,
    pub from: Option<String>,
    pub to: Option<String>,
    /// people named in the calendar that no record matches
    pub unmatched: Vec<String>,
}

pub fn calendar_info(conn: &rusqlite::Connection) -> rusqlite::Result<CalendarInfo> {
    let (count, from, to) =
        conn.query_row("SELECT COUNT(*), MIN(date), MAX(date) FROM calendar_events", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    let index = PersonIndex::load(conn)?;
    let mut st = conn.prepare("SELECT DISTINCT summary FROM calendar_events ORDER BY summary")?;
    let summaries: Vec<String> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut unmatched: Vec<String> = Vec::new();
    for s in summaries {
        if let Some((_, name)) = event_person(&s) {
            let shown = s.split_once(" of ").map(|(_, n)| n.to_string()).unwrap_or(s.clone());
            if index.find(&name).is_none() && !unmatched.contains(&shown) {
                unmatched.push(shown);
            }
        }
    }
    Ok(CalendarInfo { count, from, to, unmatched })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalEvent {
    /// the record it is about, if any
    pub person_id: Option<i64>,
    /// the record's name, or the event as the calendar says it
    pub name: String,
    /// "appearance" | "disappearance" | "ekadasi" | "parana" | "festival"
    pub kind: &'static str,
    pub has_photo: bool,
    /// the calendar's note ("Fast till noon…")
    pub note: String,
    /// worked out here (not in the imported calendar)
    pub computed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalDay {
    /// YYYY-MM-DD
    pub date: String,
    pub labels: Vec<String>,
    pub events: Vec<CalEvent>,
}

fn festival_kind(summary: &str) -> &'static str {
    let f = fold_text(summary);
    if f.starts_with("parana") {
        "parana"
    } else if f.contains("ekadas") || f.contains("dvadas") && f.contains("maha") {
        "ekadasi"
    } else {
        "festival"
    }
}

/// `count` days from the given one. Inside the dates of the imported
/// calendar its events are shown, each person linked to their record; the
/// records it does not mention, and any day outside it, use the lunar days
/// read from the records.
pub fn upcoming(conn: &rusqlite::Connection, y: i32, m: u32, d: u32, count: i64) -> rusqlite::Result<Vec<CalDay>> {
    let photo = |id: i64| -> bool {
        conn.query_row("SELECT EXISTS(SELECT 1 FROM person_photos WHERE person_id = ?1)", [id], |r| r.get(0)).unwrap_or(false)
    };
    // the imported calendar: its dates and the people it mentions
    let info: (Option<String>, Option<String>) = conn.query_row("SELECT MIN(date), MAX(date) FROM calendar_events", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let index = PersonIndex::load(conn)?;
    let mut mentioned = std::collections::HashSet::new();
    let mut by_date: std::collections::HashMap<String, Vec<(String, String)>> = std::collections::HashMap::new();
    {
        let mut st = conn.prepare("SELECT date, summary, description FROM calendar_events ORDER BY date, rowid")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (date, summary, description) = row?;
            if let Some(id) = event_person(&summary).and_then(|(_, n)| index.find(&n)) {
                mentioned.insert(id);
            }
            by_date.entry(date).or_default().push((summary, description));
        }
    }
    let mut st = conn.prepare(
        "SELECT p.id, p.name, p.appearance, p.disappearance, EXISTS(SELECT 1 FROM person_photos ph WHERE ph.person_id = p.id)
         FROM persons p WHERE p.appearance <> '' OR p.disappearance <> '' ORDER BY p.name COLLATE NOCASE",
    )?;
    let mut marked: Vec<(LunarDay, &'static str, i64, String, bool)> = Vec::new();
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get(4)?)))?;
    for row in rows {
        let (id, name, app, dis, photo) = row?;
        for (text, kind) in [(app, "appearance"), (dis, "disappearance")] {
            if let Some(day) = parse_day(&text) {
                marked.push((day, kind, id, name.clone(), photo));
            }
        }
    }
    let names: std::collections::HashMap<i64, String> =
        marked.iter().map(|(_, _, id, n, _)| (*id, n.clone())).collect();
    let mut out = Vec::new();
    for i in 0..count {
        let (yy, mm, dd) = add_days(y, m, d, i);
        let date = format!("{yy:04}-{mm:02}-{dd:02}");
        let lunar = day(yy, mm, dd);
        let covered = matches!(&info, (Some(a), Some(b)) if a.as_str() <= date.as_str() && date.as_str() <= b.as_str());
        let mut events: Vec<CalEvent> = Vec::new();
        if covered {
            for (summary, note) in by_date.get(&date).into_iter().flatten() {
                let person = event_person(summary);
                let id = person.as_ref().and_then(|(_, n)| index.find(n));
                let name = match id {
                    Some(id) => names.get(&id).cloned().or_else(|| {
                        conn.query_row("SELECT name FROM persons WHERE id = ?1", [id], |r| r.get(0)).ok()
                    }),
                    None => None,
                };
                events.push(CalEvent {
                    person_id: id,
                    name: name.unwrap_or_else(|| summary.clone()),
                    kind: person.map(|(k, _)| k).unwrap_or_else(|| festival_kind(summary)),
                    has_photo: id.is_some_and(photo),
                    note: note.clone(),
                    computed: false,
                });
            }
        }
        for (ld, kind, id, name, has_photo) in &marked {
            if lunar.contains(ld) && !(covered && mentioned.contains(id)) {
                events.push(CalEvent { person_id: Some(*id), name: name.clone(), kind, has_photo: *has_photo, note: String::new(), computed: true });
            }
        }
        out.push(CalDay { date, labels: lunar.into_iter().map(label).collect(), events });
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeSpan {
    pub id: i64,
    pub name: String,
    pub born: Option<i32>,
    pub died: Option<i32>,
    pub parampara: bool,
    pub has_photo: bool,
}

/// The records whose fields give a year of appearance or disappearance, in
/// order of time (for the timeline).
pub fn timeline(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<LifeSpan>> {
    let mut st = conn.prepare(
        "SELECT p.id, p.name, p.appearance, p.disappearance,
                EXISTS(SELECT 1 FROM person_tree t WHERE t.person_id = p.id),
                EXISTS(SELECT 1 FROM person_photos ph WHERE ph.person_id = p.id)
         FROM persons p WHERE p.appearance <> '' OR p.disappearance <> ''",
    )?;
    let rows = st.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get(4)?, r.get(5)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, app, dis, parampara, has_photo) = row?;
        let born = year_of(&app);
        // a passing before the birth is a misreading
        let died = year_of(&dis).filter(|d| born.map_or(true, |b| *d >= b && *d - b <= 130));
        if born.is_some() || died.is_some() {
            out.push(LifeSpan { id, name, born, died, parampara, has_photo });
        }
    }
    out.sort_by_key(|l| (l.born.or(l.died.map(|d| d - 60)).unwrap_or(0), l.died.unwrap_or(0)));
    Ok(out)
}

/// The civil day of `jd` + `offset` days.
pub fn add_days(y: i32, m: u32, d: u32, offset: i64) -> (i32, u32, u32) {
    ymd(jd(y, m, d) + offset as f64 + 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32) -> LunarDay {
        day(y, m, d)[0]
    }

    #[test]
    fn known_days() {
        // Gaura Pūrṇimā 2026: 3 March (Phālguna Pūrṇimā, last day of Govinda)
        assert_eq!(at(2026, 3, 3), LunarDay { month: 11, tithi: 15 });
        // Janmāṣṭamī 2026: 4 September (Hṛṣīkeśa, Kṛṣṇa Aṣṭamī)
        assert_eq!(at(2026, 9, 4), LunarDay { month: 5, tithi: 23 });
        // 2026 has an extra (adhika) month in May–June
        assert_eq!(at(2026, 5, 25).month, 12);
        // Gaura Pūrṇimā 2025: 14 March
        assert_eq!(at(2025, 3, 14), LunarDay { month: 11, tithi: 15 });
    }

    #[test]
    fn parsing() {
        let p = |s: &str| parse_day(s).map(|d| (d.month, d.tithi));
        assert_eq!(p("Magha Sukla Saptami, 1355 Saka (1433)"), Some((10, 7)));
        assert_eq!(p("Phalguna Purnima, 23 Phalguna 1407 Saka (18 February 1486)"), Some((11, 15)));
        assert_eq!(p("Pausa Krsna Tritiya, 1500"), Some((9, 18)));
        assert_eq!(p("Vaisakha Amavasya, 1486"), Some((1, 30)));
        assert_eq!(p("1455 Saka (1533), Bhadra Sukla Dvadasi"), Some((5, 12)));
        assert_eq!(p("Utthana Ekadasi, Friday 18 November 1904, 8 am"), Some((7, 11)));
        assert_eq!(p("Sravana Krsna Astami"), Some((4, 23)));
        assert_eq!(p("Asadha, the day after the full moon (Purnima)"), None);
        assert_eq!(p("1901"), None);
        assert_eq!(p("Damodara Gaura Pancami"), Some((7, 5)), "Vaiṣṇava month name");
        // a modern date is turned into its lunar day
        assert!(p("Sunday, 2 September 1838").is_some());
        assert_eq!(p("Thursday, 4 September 2026"), Some((5, 23)));
        assert_eq!(p("4 de septiembre de 2026"), Some((5, 23)));
    }

    #[test]
    fn ics() {
        let text = r"BEGIN:VCALENDAR
BEGIN:VEVENT
UID:a1
DTSTART;VALUE=DATE:20261120
SUMMARY:Āvirbhāva of Śrī Śrīmad Bhakti Dayita Mādhava Gosvāmī Mah
 ārāja
END:VEVENT
BEGIN:VEVENT
UID:a2
DTSTART;VALUE=DATE:20261120
SUMMARY:Utthāna Ekādaśī
DESCRIPTION:Fast\, then\npāraṇa
END:VEVENT
END:VCALENDAR
";
        let ev = parse_ics(text);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].date, "2026-11-20");
        assert_eq!(ev[0].summary, "Āvirbhāva of Śrī Śrīmad Bhakti Dayita Mādhava Gosvāmī Mahārāja");
        assert_eq!(ev[1].description, "Fast, then\npāraṇa");
        assert_eq!(festival_kind(&ev[1].summary), "ekadasi");
        let (kind, name) = event_person(&ev[0].summary).unwrap();
        assert_eq!((kind, name.as_str()), ("appearance", "bhakti dayita madhava gosvami maharaja"));
        let (_, long) = event_person("Tirobhāva of Nitya-līlā praviṣṭa oṁ viṣṇupāda aṣṭottara-śata-śrī Śrīmad Bhaktivedānta Vāmana Gosvāmī Mahārāja").unwrap();
        assert_eq!(long, "bhaktivedanta vamana gosvami maharaja");
        assert!(event_person("Pāpamocani Ekādaśī").is_none());

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let tx = conn.transaction().unwrap();
        let ids: Vec<i64> = ["Bhakti Dayita Madhava", "Bhaktivinoda Thakura", "Gadadhara Pandit Gosvami", "Gadadhara Dasa", "Srivasa Pandita", "Srila Bhaktivedanta Vamana Gosvami Maharaja"]
            .iter()
            .map(|n| crate::store::create_person(&tx, n).unwrap())
            .collect();
        tx.commit().unwrap();
        let index = PersonIndex::load(&conn).unwrap();
        let find = |s: &str| index.find(&event_person(s).unwrap().1);
        assert_eq!(find(&ev[0].summary), Some(ids[0]));
        assert_eq!(find("Āvirbhāva of Śrīla Saccidānanda Bhaktivinoda Ṭhākura"), Some(ids[1]));
        assert_eq!(find("Āvirbhāva of Śrī Gadādhara Paṇḍita"), Some(ids[2]));
        assert_eq!(find("Tirobhāva of Śrī Śrīvāsa Paṇḍita"), Some(ids[4]));
        assert_eq!(find("Tirobhāva of Nitya-līlā praviṣṭa oṁ viṣṇupāda aṣṭottara-śata-śrī Śrīmad Bhaktivedānta Vāmana Gosvāmī Mahārāja"), Some(ids[5]));
        assert_eq!(find("Tirobhāva of Śrī Murāri Gupta"), None);

        // the calendar's days win over the ones worked out here
        import_ics(&mut conn, text).unwrap();
        let days = upcoming(&conn, 2026, 11, 20, 2).unwrap();
        let today = &days[0].events;
        assert_eq!(today.len(), 2, "no computed duplicate");
        assert_eq!(today[0].person_id, Some(ids[0]));
        assert_eq!(today[1].kind, "ekadasi");
        assert!(calendar_info(&conn).unwrap().unmatched.is_empty());
    }

    #[test]
    fn years() {
        assert_eq!(year_of("Phalguna Purnima, 23 Phalguna 1407 Saka (18 February 1486)"), Some(1486));
        assert_eq!(year_of("1410 Saka (1488)"), Some(1488));
        assert_eq!(year_of("Asvina Sukla Dvadasi, 1504 Saka"), Some(1582));
        assert_eq!(year_of("Early 18th century"), None);
        assert_eq!(year_of("Utthana Ekadasi, Friday 18 November 1904, 8 am"), Some(1904));
        assert_eq!(add_days(2026, 12, 31, 1), (2027, 1, 1));
    }
}
