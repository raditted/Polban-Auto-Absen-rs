//! Text normalization, signature splitting, and status reason helpers.

use std::borrow::Cow;

use crate::model::Color;

/// Collapse runs of whitespace into single spaces without extra intermediate Vec allocations.
pub(crate) fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for word in input.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// Strip leading lecturer code such as `"KO078N-Trisna Gelar Abdillah, S.T., M.Kom"`.
///
/// Only removes the prefix before `'-'` when the code is non-empty, contains no spaces,
/// is at most 12 characters long, and contains at least one ASCII digit (preserving
/// hyphenated names like `"Ana-Maria"`).
pub fn nama_dosen(teks: &str) -> String {
    let normalized = collapse_whitespace(teks);
    if let Some((kode, sisa)) = normalized.split_once('-') {
        let kode = kode.trim();
        let sisa = sisa.trim();
        if !sisa.is_empty()
            && !kode.is_empty()
            && !kode.contains(' ')
            && kode.chars().count() <= 12
            && kode.chars().any(|c| c.is_ascii_digit())
        {
            return sisa.to_string();
        }
    }
    normalized
}

/// Parse `H:MM` or `HH:MM` with optional `:SS` at byte offset `pos`.
/// Returns `(hh_mm_end, total_end)`.
fn parse_time_token(bytes: &[u8], pos: usize) -> Option<(usize, usize)> {
    let len = bytes.len();
    if pos >= len || !bytes[pos].is_ascii_digit() {
        return None;
    }
    let mut i = pos + 1;
    if i < len && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i >= len || bytes[i] != b':' {
        return None;
    }
    i += 1;
    if i + 1 >= len || !bytes[i].is_ascii_digit() || !bytes[i + 1].is_ascii_digit() {
        return None;
    }
    let hh_mm_end = i + 2;
    let mut total_end = hh_mm_end;
    if total_end + 2 < len
        && bytes[total_end] == b':'
        && bytes[total_end + 1].is_ascii_digit()
        && bytes[total_end + 2].is_ascii_digit()
    {
        total_end += 3;
    }
    Some((hh_mm_end, total_end))
}

/// Match `s.? \s* d.?` or `-` separator at `pos`, returning index right after separator.
fn parse_range_sep(bytes: &[u8], pos: usize) -> Option<usize> {
    let len = bytes.len();
    if pos >= len {
        return None;
    }
    if bytes[pos] == b'-' {
        return Some(pos + 1);
    }
    if bytes[pos] == b's' || bytes[pos] == b'S' {
        let mut i = pos + 1;
        if i < len && bytes[i] == b'.' {
            i += 1;
        }
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < len && (bytes[i] == b'd' || bytes[i] == b'D') {
            i += 1;
            if i < len && bytes[i] == b'.' {
                i += 1;
            }
            return Some(i);
        }
    }
    None
}

/// Compact `"07:00:00 s.d. 09:30:00"` into `"07:00-09:30"` for narrow columns.
/// Returns the original string unchanged if no time range is recognized.
pub fn jam_ringkas(jam: &str) -> Cow<'_, str> {
    let bytes = jam.as_bytes();
    let len = bytes.len();
    for start in 0..len {
        let Some((t1_hhmm_end, mut cursor)) = parse_time_token(bytes, start) else {
            continue;
        };
        while cursor < len && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let Some(after_sep) = parse_range_sep(bytes, cursor) else {
            continue;
        };
        cursor = after_sep;
        while cursor < len && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let Some((t2_hhmm_end, _t2_total_end)) = parse_time_token(bytes, cursor) else {
            continue;
        };
        let t1 = &jam[start..t1_hhmm_end];
        let t2 = &jam[cursor..t2_hhmm_end];
        if start == 0 && t2_hhmm_end == len && &jam[t1_hhmm_end..cursor] == "-" {
            return Cow::Borrowed(jam);
        }
        return Cow::Owned(format!("{t1}-{t2}"));
    }
    Cow::Borrowed(jam)
}

/// Split an action/skip signature into `(nama_mk, dosen, jam)` without allocating.
///
/// Formats:
/// - 4+ parts (skip): `btn_txt|nama_mk|dosen|jam` -> `(parts[1], parts[2], parts[3])`
/// - 3 parts (action): `nama_mk|dosen|jam` -> `(parts[0], parts[1], parts[2])`
/// - 2 parts (legacy/pw): `btn_txt|nama_mk` -> `(parts[1], "", "")`
/// - 1 part: `nama_mk` -> `(parts[0], "", "")`
pub fn split_sig(sig: &str) -> (&str, &str, &str) {
    let mut parts = sig.split('|').map(str::trim);
    let p0 = parts.next().unwrap_or("");
    let Some(p1) = parts.next() else {
        return (p0, "", "");
    };
    let Some(p2) = parts.next() else {
        return (p1, "", "");
    };
    match parts.next() {
        Some(p3) => (p1, p2, p3),
        None => (p0, p1, p2),
    }
}

/// Human-readable Indonesian reason why a non-blue row is skipped.
pub const fn skip_reason(color: Color) -> &'static str {
    match color {
        Color::Green => "sudah absen (dihitung berhasil)",
        Color::Orange => "belum buka -- dosen belum membuka sesi absen",
        Color::Red => "belum waktunya -- di luar jam absensi",
        Color::Unknown => "warna tidak dikenali -- dilewati demi keamanan",
        Color::Blue => "tidak perlu diklik",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nama_dosen_strips_code_and_keeps_hyphenated_names() {
        assert_eq!(
            nama_dosen("KO078N-Trisna Gelar Abdillah, S.T., M.Kom"),
            "Trisna Gelar Abdillah, S.T., M.Kom"
        );
        assert_eq!(nama_dosen("  BD011N -  Budi   Santoso  "), "Budi Santoso");
        assert_eq!(nama_dosen("Ana-Maria"), "Ana-Maria");
        assert_eq!(nama_dosen("Jean-Luc Picard"), "Jean-Luc Picard");
        assert_eq!(
            nama_dosen("VERYLONGKODE123-Someone"),
            "VERYLONGKODE123-Someone"
        );
        assert_eq!(nama_dosen("KO078N-"), "KO078N-");
    }

    #[test]
    fn jam_ringkas_compacts_ranges() {
        assert_eq!(jam_ringkas("07:00:00 s.d. 09:30:00"), "07:00-09:30");
        assert_eq!(jam_ringkas("7:00 - 9:30"), "7:00-9:30");
        assert_eq!(jam_ringkas("07:00 - 09:30"), "07:00-09:30");
        assert_eq!(jam_ringkas("13:20:00 sd 15:00:00"), "13:20-15:00");
        assert_eq!(jam_ringkas("07:00-09:30"), "07:00-09:30");
        assert!(matches!(jam_ringkas("07:00-09:30"), Cow::Borrowed(_)));
        assert_eq!(jam_ringkas("jadwal fleksibel"), "jadwal fleksibel");
    }

    #[test]
    fn split_sig_handles_one_to_four_parts() {
        assert_eq!(
            split_sig("Absen|Algoritma|KO001N-Dosen A|07:00-09:30"),
            ("Algoritma", "KO001N-Dosen A", "07:00-09:30")
        );
        assert_eq!(
            split_sig("Algoritma|KO001N-Dosen A|07:00-09:30"),
            ("Algoritma", "KO001N-Dosen A", "07:00-09:30")
        );
        assert_eq!(split_sig("Klik|Algoritma"), ("Algoritma", "", ""));
        assert_eq!(split_sig("Algoritma"), ("Algoritma", "", ""));
        assert_eq!(split_sig(""), ("", "", ""));
    }

    #[test]
    fn skip_reasons_match_python() {
        assert_eq!(skip_reason(Color::Green), "sudah absen (dihitung berhasil)");
        assert_eq!(
            skip_reason(Color::Orange),
            "belum buka -- dosen belum membuka sesi absen"
        );
        assert_eq!(
            skip_reason(Color::Red),
            "belum waktunya -- di luar jam absensi"
        );
        assert_eq!(
            skip_reason(Color::Unknown),
            "warna tidak dikenali -- dilewati demi keamanan"
        );
        assert_eq!(skip_reason(Color::Blue), "tidak perlu diklik");
    }
}
