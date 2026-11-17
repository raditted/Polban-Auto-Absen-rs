//! HTML parser for Polban academic login, user header, and `#jadwal` table.

use std::sync::LazyLock;

use scraper::{ElementRef, Html, Selector};

use crate::model::{AbsenFormData, Action, Color, Skip, UserInfo};
use crate::text::{collapse_whitespace, skip_reason};

static SEL_USER_HEADER: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".user-header").expect("valid selector .user-header"));
static SEL_TABLE_JADWAL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("table#jadwal").expect("valid selector table#jadwal"));
static SEL_TABLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("table").expect("valid selector table"));
static SEL_TR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("tr").expect("valid selector tr"));
static SEL_TD: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("td").expect("valid selector td"));
static SEL_BTN: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a, button").expect("valid selector a, button"));
static SEL_INPUT_KLS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("input#kls").expect("valid selector input#kls"));

/// Check whether HTML represents the login form (session expired or login failed).
pub fn is_login_page(html: &str) -> bool {
    html.contains("name=\"username\"") && html.contains("name=\"password\"")
}

/// Classify a button's Bootstrap CSS classes into `Color` with exact Python priority.
pub fn classify_classes<'a>(classes: impl Iterator<Item = &'a str>) -> Color {
    let mut has_success = false;
    let mut has_danger = false;
    let mut has_warning = false;
    let mut has_blue = false;

    for cls in classes {
        match cls {
            "btn-success" => has_success = true,
            "btn-danger" => has_danger = true,
            "btn-warning" => has_warning = true,
            "btn-primary" | "btn-info" => has_blue = true,
            _ => {}
        }
    }

    if has_success {
        Color::Green
    } else if has_danger {
        Color::Red
    } else if has_warning {
        Color::Orange
    } else if has_blue {
        Color::Blue
    } else {
        Color::Unknown
    }
}

fn contains_ascii_ignore_case(haystack: &str, needle: &[u8]) -> bool {
    let h = haystack.as_bytes();
    if needle.is_empty() || h.len() < needle.len() {
        return false;
    }
    h.windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle))
}

/// Skip navigation/logout/profile buttons that match `.btn` in rows.
pub fn is_nav_or_logout(href: Option<&str>, text: &str) -> bool {
    if let Some(h) = href {
        if contains_ascii_ignore_case(h, b"logout")
            || contains_ascii_ignore_case(h, b"keluar")
            || contains_ascii_ignore_case(h, b"login")
            || contains_ascii_ignore_case(h, b"profil")
        {
            return true;
        }
    }
    contains_ascii_ignore_case(text, b"logout")
        || contains_ascii_ignore_case(text, b"keluar")
        || contains_ascii_ignore_case(text, b"profil")
}

/// Collect whitespace-normalized text from an HTML element without intermediate Vecs.
fn element_text(el: ElementRef<'_>) -> String {
    let mut out = String::new();
    for chunk in el.text() {
        for word in chunk.split_whitespace() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(word);
        }
    }
    out
}

/// Extract student NIM, name, and major from `.user-header`.
pub fn parse_user_header(html: &str, fallback_nim: &str) -> UserInfo {
    let mut info = UserInfo {
        nim: fallback_nim.to_string(),
        nama: String::new(),
        jurusan: String::new(),
    };
    let doc = Html::parse_document(html);
    let Some(uh) = doc.select(&SEL_USER_HEADER).next() else {
        return info;
    };

    let mut tokens: Vec<&str> = Vec::new();
    for chunk in uh.text() {
        tokens.extend(chunk.split_whitespace());
    }
    if tokens.is_empty() {
        return info;
    }

    if tokens[0].chars().all(|c| c.is_ascii_digit()) {
        info.nim = tokens[0].to_string();
        let rest = &tokens[1..];
        let mut ji = rest.len();
        for (i, tok) in rest.iter().enumerate() {
            let is_d_digit =
                tok.starts_with('D') && tok.as_bytes().get(1).is_some_and(u8::is_ascii_digit);
            let is_teknik = tok.len() >= 6 && tok.as_bytes()[..6].eq_ignore_ascii_case(b"teknik");
            if tok.contains('-') || is_d_digit || is_teknik {
                ji = i;
                break;
            }
        }
        info.nama = rest[..ji].join(" ");
        info.jurusan = rest[ji..].join(" ");
    } else {
        info.nama = tokens.join(" ");
    }
    info
}

fn td_text(tds: &[String], idx: usize) -> &str {
    tds.get(idx).map(String::as_str).unwrap_or("")
}

/// Parse `table#jadwal` (or fallback first `table`) into `(actions, skips)`.
pub fn find_rows(html: &str, absen_endpoint: &str) -> (Vec<Action>, Vec<Skip>) {
    let doc = Html::parse_document(html);
    let Some(table) = doc
        .select(&SEL_TABLE_JADWAL)
        .next()
        .or_else(|| doc.select(&SEL_TABLE).next())
    else {
        return (Vec::new(), Vec::new());
    };

    let mut actions = Vec::new();
    let mut skips = Vec::new();
    let mut tds: Vec<String> = Vec::with_capacity(8);

    for row in table.select(&SEL_TR) {
        let Some(btn) = row
            .select(&SEL_BTN)
            .find(|el| el.value().classes().any(|c| c.contains("btn")))
        else {
            continue;
        };

        let mut btn_txt = element_text(btn);
        if btn_txt.is_empty() {
            if let Some(val) = btn.value().attr("value") {
                btn_txt = collapse_whitespace(val);
            }
        }

        if is_nav_or_logout(btn.value().attr("href"), &btn_txt) {
            continue;
        }

        let color = classify_classes(btn.value().classes());
        let disabled = btn.value().attr("disabled").is_some();

        tds.clear();
        for td in row.select(&SEL_TD).take(8) {
            tds.push(element_text(td));
        }

        let dosen = td_text(&tds, 1);
        let mk_code = td_text(&tds, 2);
        let mk_name = td_text(&tds, 3);
        let nama_mk = if mk_name.is_empty() { mk_code } else { mk_name };
        let tp = td_text(&tds, 4);
        let ja = td_text(&tds, 5);
        let jb = td_text(&tds, 6);
        let jam = td_text(&tds, 7);

        if color != Color::Blue {
            let reason = if disabled {
                "disabled"
            } else {
                skip_reason(color)
            };
            skips.push(Skip {
                color,
                sig: format!("{btn_txt}|{nama_mk}|{dosen}|{jam}"),
                reason,
            });
            continue;
        }

        if disabled {
            skips.push(Skip {
                color,
                sig: format!("{btn_txt}|{nama_mk}|{dosen}|{jam}"),
                reason: "disabled",
            });
            continue;
        }

        let dsn = dosen.split('-').next().unwrap_or("").trim().to_string();
        let kls = row
            .select(&SEL_INPUT_KLS)
            .next()
            .and_then(|inp| inp.value().attr("value"))
            .map(|v| v.trim().to_string())
            .unwrap_or_default();

        actions.push(Action {
            sig: format!("{nama_mk}|{dosen}|{jam}"),
            key: format!("{nama_mk}|{dosen}"),
            url: absen_endpoint.to_string(),
            data: AbsenFormData {
                ja: ja.to_string(),
                jb: jb.to_string(),
                mk: mk_code.to_string(),
                dsn,
                tp: tp.to_string(),
                kls,
            },
            nama_mk: nama_mk.to_string(),
            dosen: dosen.to_string(),
            jam: jam.to_string(),
            label: format!("{nama_mk} ({jam})"),
        });
    }

    (actions, skips)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_classes_priority_and_variants() {
        assert_eq!(
            classify_classes(["btn", "btn-primary"].into_iter()),
            Color::Blue
        );
        assert_eq!(
            classify_classes(["btn", "btn-info"].into_iter()),
            Color::Blue
        );
        assert_eq!(
            classify_classes(["btn", "btn-success"].into_iter()),
            Color::Green
        );
        assert_eq!(
            classify_classes(["btn", "btn-danger"].into_iter()),
            Color::Red
        );
        assert_eq!(
            classify_classes(["btn", "btn-warning"].into_iter()),
            Color::Orange
        );
        assert_eq!(
            classify_classes(["btn", "btn-default"].into_iter()),
            Color::Unknown
        );
        // Priority: success wins over primary
        assert_eq!(
            classify_classes(["btn-primary", "btn-success"].into_iter()),
            Color::Green
        );
    }

    #[test]
    fn nav_or_logout_matches_case_insensitively() {
        assert!(is_nav_or_logout(Some("/laman/logout"), "Keluar"));
        assert!(is_nav_or_logout(Some("/laman/login"), "Masuk"));
        assert!(is_nav_or_logout(Some("/mhs/profile"), "Akun"));
        assert!(is_nav_or_logout(None, "Profil Mahasiswa"));
        assert!(!is_nav_or_logout(Some("#"), "Login Matkul"));
        assert!(!is_nav_or_logout(Some("javascript:void(0)"), "Absen"));
    }
}
