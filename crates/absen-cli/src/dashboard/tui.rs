//! Live terminal dashboard rendered with `ratatui` and `crossterm`.

use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::Once;
use std::time::{Duration, Instant};

use absen_core::text::jam_ringkas;
use absen_core::{Color as AbsenColor, Event as EngineEvent, LogLevel};
use chrono::{Datelike, Local, NaiveDate, Weekday};
use crossterm::cursor::{Hide, Show};
use crossterm::event as ct_event;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Widget};
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::dashboard::keys::{map_crossterm_event, ScrollAction, UserAction};
use crate::dashboard::state::DashboardState;
use crate::platform::{self, ASCII_BANNER, BANNER_WIDTH, COMPACT_TITLE, VERSION};

const BORDER_ASCII: border::Set = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

static TERMINAL_ACTIVE: AtomicBool = AtomicBool::new(false);
static ALT_SCREEN_ACTIVE: AtomicBool = AtomicBool::new(false);
static PANIC_HOOK_ONCE: Once = Once::new();

pub(crate) fn is_terminal_active() -> bool {
    TERMINAL_ACTIVE.load(Ordering::SeqCst)
}

fn restore_terminal_state() {
    if !TERMINAL_ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut stdout = io::stdout();
    let _ = execute!(stdout, Show);
    if ALT_SCREEN_ACTIVE.swap(false, Ordering::SeqCst) {
        let _ = stdout.write_all(b"\x1b[?1007l");
        let _ = execute!(stdout, LeaveAlternateScreen);
    }
    let _ = disable_raw_mode();
    let _ = stdout.flush();
}

/// RAII guard that restores terminal mode on normal exit, Ctrl+C, or panic.
struct TerminalGuard;

impl TerminalGuard {
    fn enter(alt_screen: bool, ascii: bool) -> io::Result<Self> {
        enable_raw_mode()?;
        TERMINAL_ACTIVE.store(true, Ordering::SeqCst);

        let mut stdout = io::stdout();
        if alt_screen {
            if let Err(e) = execute!(stdout, EnterAlternateScreen) {
                restore_terminal_state();
                return Err(e);
            }
            ALT_SCREEN_ACTIVE.store(true, Ordering::SeqCst);
            if !ascii {
                // DECSET 1007: alternate scroll mode sends Up/Down arrows on wheel.
                let _ = stdout.write_all(b"\x1b[?1007h");
                let _ = stdout.flush();
            }
        }
        if let Err(e) = execute!(stdout, Hide) {
            restore_terminal_state();
            return Err(e);
        }
        Ok(Self)
    }

    fn restore(self) {
        restore_terminal_state();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal_state();
    }
}

fn install_panic_hook() {
    PANIC_HOOK_ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal_state();
            prev(info);
        }));
    });
}

/// Format local date in Indonesian (manual Weekday & Month mapping).
pub fn format_tanggal_id(date: NaiveDate) -> String {
    let hari = match date.weekday() {
        Weekday::Mon => "Senin",
        Weekday::Tue => "Selasa",
        Weekday::Wed => "Rabu",
        Weekday::Thu => "Kamis",
        Weekday::Fri => "Jumat",
        Weekday::Sat => "Sabtu",
        Weekday::Sun => "Minggu",
    };
    let bulan = match date.month() {
        1 => "Januari",
        2 => "Februari",
        3 => "Maret",
        4 => "April",
        5 => "Mei",
        6 => "Juni",
        7 => "Juli",
        8 => "Agustus",
        9 => "September",
        10 => "Oktober",
        11 => "November",
        12 => "Desember",
        _ => "",
    };
    format!("{hari}, {:02} {bulan} {}", date.day(), date.year())
}

#[derive(Debug, Clone)]
pub struct TuiOptions {
    pub alt_screen: bool,
    pub ascii: bool,
    pub engine: String,
    pub interval: u64,
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub alt_screen: bool,
    pub ascii: bool,
    pub full_render: bool,
    pub tanggal: String,
    pub platform: String,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            alt_screen: true,
            ascii: false,
            full_render: false,
            tanggal: format_tanggal_id(Local::now().date_naive()),
            platform: platform::platform_label().to_string(),
        }
    }
}

/// Scroll viewport state tracked across frames.
#[derive(Debug, Clone, Default)]
pub struct ViewState {
    pub scroll: usize,
    pub max_scroll: usize,
    pub page: usize,
    pub follow: bool,
}

impl ViewState {
    pub fn apply_scroll(&mut self, action: ScrollAction) {
        let step: isize = match action {
            ScrollAction::Up => -1,
            ScrollAction::Down => 1,
            ScrollAction::PageUp => -(self.page.max(1) as isize),
            ScrollAction::PageDown => self.page.max(1) as isize,
            ScrollAction::Home => {
                self.scroll = 0;
                self.follow = false;
                return;
            }
            ScrollAction::End => {
                self.scroll = self.max_scroll;
                self.follow = self.max_scroll > 0;
                return;
            }
        };
        let next = (self.scroll as isize + step).clamp(0, self.max_scroll as isize) as usize;
        self.scroll = next;
        self.follow = self.max_scroll > 0 && self.scroll == self.max_scroll;
    }
}

fn border_set(ascii: bool) -> border::Set {
    if ascii {
        BORDER_ASCII
    } else {
        border::ROUNDED
    }
}

fn status_style(color: AbsenColor) -> Style {
    match color {
        AbsenColor::Blue => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        AbsenColor::Green => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        AbsenColor::Orange => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        AbsenColor::Red => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        AbsenColor::Unknown => Style::default().add_modifier(Modifier::DIM),
    }
}

fn log_level_style(level: LogLevel) -> Style {
    match level {
        LogLevel::Ok => Style::default().fg(Color::Green),
        LogLevel::Fail => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        LogLevel::Warn => Style::default().fg(Color::Yellow),
        LogLevel::Info => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::DIM),
        LogLevel::Skip => Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
    }
}

/// Word-aware fold helper returning wrapped lines for width `max_w` (splits mid-word
/// only when a single word exceeds `max_w`, matching `rich` overflow="fold").
fn wrap_text(text: &str, max_w: usize) -> Vec<String> {
    let w = max_w.max(1);
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    for raw_line in text.split('\n') {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut cur_len = 0usize;
        for word in raw_line.split_inclusive(' ') {
            let trimmed = word.trim_end_matches(' ');
            let trimmed_len = trimmed.chars().count();
            let full_len = word.chars().count();

            if cur_len > 0 && cur_len + trimmed_len > w {
                lines.push(current.trim_end().to_string());
                current.clear();
                cur_len = 0;
            }

            if trimmed_len > w {
                for ch in word.chars() {
                    if cur_len >= w {
                        lines.push(std::mem::take(&mut current));
                        cur_len = 0;
                    }
                    current.push(ch);
                    cur_len += 1;
                }
            } else {
                if cur_len + full_len > w && cur_len > 0 {
                    // Word without trailing space fits on current line, space pushes next word.
                    current.push_str(trimmed);
                    lines.push(std::mem::take(&mut current));
                    cur_len = 0;
                } else {
                    current.push_str(word);
                    cur_len += full_len;
                }
            }
        }
        if !current.is_empty() || lines.is_empty() {
            lines.push(current.trim_end().to_string());
        }
    }
    lines
}

fn render_banner_buf(big: bool, width: u16, subtitle_text: &str) -> Buffer {
    let mut lines: Vec<Line<'_>> = Vec::new();
    if big {
        let block_pad = " ".repeat((width as usize).saturating_sub(BANNER_WIDTH) / 2);
        let style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        for raw in ASCII_BANNER.lines() {
            lines.push(
                Line::from(Span::styled(format!("{block_pad}{raw}"), style))
                    .alignment(Alignment::Left),
            );
        }
    } else {
        lines.push(
            Line::from(Span::styled(
                COMPACT_TITLE.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
        );
    }
    lines.push(
        Line::from(Span::styled(
            subtitle_text.to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::DIM),
        ))
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    let height = lines.len() as u16;
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    Paragraph::new(lines).render(area, &mut buf);
    buf
}

enum StatusVal<'a> {
    Plain(&'a str, Style),
    Spans(Vec<Span<'a>>),
}

fn spans_char_len(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| s.content.chars().count()).sum()
}

fn wrap_spans<'a>(spans: Vec<Span<'a>>, max_w: usize) -> Vec<Line<'a>> {
    let w = max_w.max(1);
    let mut lines: Vec<Vec<Span<'a>>> = vec![Vec::new()];
    let mut cur_w = 0usize;

    for span in spans {
        let style = span.style;
        let mut chunk = String::new();
        for ch in span.content.chars() {
            if cur_w >= w {
                if !chunk.is_empty() {
                    lines
                        .last_mut()
                        .expect("non-empty lines")
                        .push(Span::styled(std::mem::take(&mut chunk), style));
                }
                lines.push(Vec::new());
                cur_w = 0;
            }
            chunk.push(ch);
            cur_w += 1;
        }
        if !chunk.is_empty() {
            lines
                .last_mut()
                .expect("non-empty lines")
                .push(Span::styled(chunk, style));
        }
    }

    lines.into_iter().map(Line::from).collect()
}

fn render_status_panel(state: &DashboardState, width: u16, opts: &RenderOptions) -> Buffer {
    let pad_x: u16 = if width >= 50 { 2 } else { 1 };
    let inner_w = width.saturating_sub(2 + 2 * pad_x).max(1);
    let label_w: usize = 10; // "Hasil Scan"
    let value_w = (width as usize)
        .saturating_sub(2 + 2 * pad_x as usize + 13)
        .max(1);

    let nama = if state.user_info.nama.is_empty() {
        "-"
    } else {
        &state.user_info.nama
    };
    let nim = if state.user_info.nim.is_empty() {
        "-"
    } else {
        &state.user_info.nim
    };
    let auth_lower = state.auth_status.to_ascii_lowercase();
    let auth_style =
        if state.auth_status.contains("OK") || state.auth_status.contains("Authenticated") {
            Style::default().fg(Color::Green)
        } else if state.auth_status.contains("...") || auth_lower.contains("re-login") {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Red)
        };

    let countdown_str = format!("{:02}:{:02}", state.countdown / 60, state.countdown % 60);
    let interval_str = format!("{}s", state.interval);
    let ok_str = format!("{} berhasil", state.stat_ok);
    let fail_str = format!("{} gagal", state.stat_fail);

    let mesin_spans = vec![
        Span::raw(state.engine.clone()),
        Span::raw("   "),
        Span::styled("Interval : ", Style::default().add_modifier(Modifier::DIM)),
        Span::raw(interval_str.clone()),
        Span::raw("   "),
        Span::styled("Next scan : ", Style::default().add_modifier(Modifier::DIM)),
        Span::styled(
            countdown_str.clone(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    let absen_spans = vec![
        Span::styled(ok_str, Style::default().fg(Color::Green)),
        Span::styled(" / ", Style::default().add_modifier(Modifier::DIM)),
        Span::styled(fail_str, Style::default().fg(Color::Red)),
    ];

    let mesin_len = spans_char_len(&mesin_spans);
    let absen_len = spans_char_len(&absen_spans);

    let mut items: Vec<(&str, StatusVal<'_>)> = vec![
        (
            "Nama",
            StatusVal::Plain(nama, Style::default().add_modifier(Modifier::BOLD)),
        ),
        ("NIM", StatusVal::Plain(nim, Style::default())),
    ];
    if !state.user_info.jurusan.is_empty() {
        items.push((
            "Jurusan",
            StatusVal::Plain(&state.user_info.jurusan, Style::default()),
        ));
    }
    items.push(("Tanggal", StatusVal::Plain(&opts.tanggal, Style::default())));
    items.push(("", StatusVal::Plain("", Style::default())));
    items.push(("Status", StatusVal::Plain(&state.auth_status, auth_style)));
    items.push((
        "Hasil Scan",
        StatusVal::Plain(&state.last_scan_info, Style::default()),
    ));

    if mesin_len + 11 + absen_len <= value_w {
        let mut combined = mesin_spans;
        combined.push(Span::raw("   "));
        combined.push(Span::styled(
            "Absen : ",
            Style::default().add_modifier(Modifier::DIM),
        ));
        combined.extend(absen_spans);
        items.push(("Engine", StatusVal::Spans(combined)));
    } else if mesin_len <= value_w {
        items.push(("Engine", StatusVal::Spans(mesin_spans)));
        items.push(("Absen", StatusVal::Spans(absen_spans)));
    } else {
        items.push(("Engine", StatusVal::Plain(&state.engine, Style::default())));
        items.push(("Interval", StatusVal::Spans(vec![Span::raw(interval_str)])));
        items.push((
            "Next scan",
            StatusVal::Spans(vec![Span::styled(
                countdown_str,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
        ));
        items.push(("Absen", StatusVal::Spans(absen_spans)));
    }

    let dim = Style::default().add_modifier(Modifier::DIM);
    let mut rendered_lines: Vec<Line<'_>> = Vec::new();
    for (label, val) in items {
        if label.is_empty() {
            rendered_lines.push(Line::from(""));
            continue;
        }
        let val_lines: Vec<Line<'_>> = match val {
            StatusVal::Plain(txt, st) => wrap_text(txt, value_w)
                .into_iter()
                .map(|part| Line::from(Span::styled(part, st)))
                .collect(),
            StatusVal::Spans(sp) => wrap_spans(sp, value_w),
        };
        for (idx, mut line) in val_lines.into_iter().enumerate() {
            let prefix = if idx == 0 {
                format!("{label:<label_w$} : ")
            } else {
                " ".repeat(label_w + 3)
            };
            let mut full_spans = vec![Span::styled(prefix, dim)];
            full_spans.append(&mut line.spans);
            rendered_lines.push(Line::from(full_spans));
        }
    }

    let height = (rendered_lines.len() as u16) + 2;
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set(opts.ascii))
        .border_style(dim)
        .title(Span::styled(
            "Identitas",
            Style::default().add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    block.render(area, &mut buf);

    let padded = Rect::new(
        inner.x.saturating_add(pad_x),
        inner.y,
        inner_w.min(inner.width),
        inner.height,
    );
    Paragraph::new(rendered_lines).render(padded, &mut buf);
    buf
}

fn render_jadwal_panel(state: &DashboardState, width: u16, opts: &RenderOptions) -> Buffer {
    let narrow = width < 100;
    let tiny = width < 56;
    let micro = width < 44;
    let pad_x: u16 = if tiny { 0 } else { 1 };
    let dim = Style::default().add_modifier(Modifier::DIM);

    let panel_inner_w = width.saturating_sub(2 + 2 * pad_x).max(1);

    // Calculate flex widths for Mata Kuliah and Dosen columns.
    // Spacing between columns in ratatui Table: column_spacing(1)
    let mut fixed_w: u16 = 3; // No
    let mut col_count: u16 = 2; // No + Mata Kuliah
    if !narrow {
        fixed_w += 22; // Jam
        col_count += 1;
    }
    if !micro {
        fixed_w += 14; // Status
        col_count += 1;
    }
    let spacing_total = col_count.saturating_sub(1);
    let flex_avail = panel_inner_w.saturating_sub(fixed_w + spacing_total);
    let (mk_w, dosen_w) = if !tiny {
        let half = (flex_avail / 2).max(8);
        (half, flex_avail.saturating_sub(half).max(8))
    } else {
        (flex_avail.max(8), 0)
    };

    let mut header_cells = vec![
        Cell::from(Line::from("No").alignment(Alignment::Right)).style(dim),
        Cell::from("Mata Kuliah"),
    ];
    let mut constraints = vec![Constraint::Length(3), Constraint::Length(mk_w)];
    if !narrow {
        header_cells.push(Cell::from("Jam"));
        constraints.push(Constraint::Length(22));
    }
    if !micro {
        header_cells.push(Cell::from(
            Line::from("Status").alignment(Alignment::Center),
        ));
        constraints.push(Constraint::Length(14));
    }
    if !tiny {
        header_cells.push(Cell::from("Dosen"));
        constraints.push(Constraint::Length(dosen_w));
    }

    let header = Row::new(header_cells)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(0);

    let mut rows: Vec<Row<'_>> = Vec::new();
    let mut body_lines: u16 = 0;

    if state.jadwal.is_empty() {
        let mut cells = vec![
            Cell::from(""),
            Cell::from(Span::styled("Belum ada data jadwal", dim)),
        ];
        while cells.len() < col_count as usize {
            cells.push(Cell::from(""));
        }
        rows.push(Row::new(cells).height(1));
        body_lines = 1;
    } else {
        for (idx, mk) in state.jadwal.iter().enumerate() {
            let st_label = mk.status.status_label();
            let st_style = status_style(mk.status);
            let no_str = format!("{}", idx + 1);

            let mut mk_lines: Vec<Line<'_>> = wrap_text(&mk.nama, mk_w as usize)
                .into_iter()
                .map(Line::from)
                .collect();

            if narrow {
                if micro {
                    mk_lines.push(Line::from(Span::styled(st_label, st_style)));
                }
                if !mk.jam.is_empty() {
                    let compact_jam = jam_ringkas(&mk.jam);
                    for l in wrap_text(&compact_jam, mk_w as usize) {
                        mk_lines.push(Line::from(l));
                    }
                }
                if tiny && !mk.dosen.is_empty() {
                    for l in wrap_text(&mk.dosen, mk_w as usize) {
                        mk_lines.push(Line::from(l));
                    }
                }
            }

            let dosen_text = if mk.dosen.is_empty() { "-" } else { &mk.dosen };
            let dosen_lines: Vec<Line<'_>> = if !tiny {
                wrap_text(dosen_text, dosen_w as usize)
                    .into_iter()
                    .map(Line::from)
                    .collect()
            } else {
                Vec::new()
            };

            let row_h = mk_lines.len().max(dosen_lines.len()).max(1) as u16;
            body_lines = body_lines.saturating_add(row_h);

            let mut cells = vec![
                Cell::from(Line::from(no_str).alignment(Alignment::Right)).style(dim),
                Cell::from(mk_lines),
            ];
            if !narrow {
                cells.push(Cell::from(mk.jam.clone()));
            }
            if !micro {
                cells.push(Cell::from(
                    Line::from(Span::styled(st_label, st_style)).alignment(Alignment::Center),
                ));
            }
            if !tiny {
                cells.push(Cell::from(dosen_lines));
            }
            rows.push(Row::new(cells).height(row_h));
        }
    }

    // Header takes 1 line (+ 1 separator line under header in slim/table style) + 2 outer borders.
    let height = body_lines.saturating_add(4);
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set(opts.ascii))
        .border_style(dim)
        .title(Span::styled(
            "Jadwal Hari Ini",
            Style::default().add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    block.render(area, &mut buf);

    let table_area = Rect::new(
        inner.x.saturating_add(pad_x),
        inner.y,
        panel_inner_w.min(inner.width),
        inner.height,
    );

    if table_area.height >= 2 {
        let header_area = Rect::new(table_area.x, table_area.y, table_area.width, 1);
        let sep_area = Rect::new(table_area.x, table_area.y + 1, table_area.width, 1);
        let body_area = Rect::new(
            table_area.x,
            table_area.y + 2,
            table_area.width,
            table_area.height.saturating_sub(2),
        );

        let header_table = Table::new(Vec::<Row>::new(), constraints.clone())
            .header(header)
            .column_spacing(1);
        Widget::render(header_table, header_area, &mut buf);

        let sep_char = if opts.ascii { "-" } else { "─" };
        let sep_line = sep_char.repeat(table_area.width as usize);
        Paragraph::new(Line::from(Span::styled(sep_line, dim))).render(sep_area, &mut buf);

        let body_table = Table::new(rows, constraints).column_spacing(1);
        Widget::render(body_table, body_area, &mut buf);
    }

    buf
}

fn render_log_panel(
    state: &DashboardState,
    width: u16,
    fixed_height: Option<u16>,
    opts: &RenderOptions,
) -> Buffer {
    let pad_x: u16 = if width >= 50 { 2 } else { 1 };
    let inner_w = width.saturating_sub(2 + 2 * pad_x).max(1) as usize;
    let dim = Style::default().add_modifier(Modifier::DIM);

    let mut lines: Vec<Line<'_>> = Vec::new();
    if state.log.is_empty() {
        lines.push(Line::from(Span::styled("Belum ada aktivitas.", dim)));
    } else {
        // timestamp (8) + ' ' + tag (4) + ' ' = 14 chars prefix
        let msg_w = inner_w.saturating_sub(14).max(1);
        for entry in &state.log {
            let code_prefix = format!("[{}] ", entry.code);
            let mut full_spans = vec![
                Span::styled(code_prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(entry.message.clone()),
            ];
            let wrapped = wrap_spans(std::mem::take(&mut full_spans), msg_w);
            for (idx, mut wline) in wrapped.into_iter().enumerate() {
                let mut row_spans = if idx == 0 {
                    vec![
                        Span::styled(entry.timestamp.clone(), dim),
                        Span::raw(" "),
                        Span::styled(entry.level.tag().to_string(), log_level_style(entry.level)),
                        Span::raw(" "),
                    ]
                } else {
                    vec![Span::raw("              ")]
                };
                row_spans.append(&mut wline.spans);
                lines.push(Line::from(row_spans));
            }
        }
    }

    let natural_h = (lines.len() as u16) + 2;
    let height = fixed_height.unwrap_or(natural_h).max(natural_h);
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);

    let title = format!("Log Aktivitas ({} terakhir)", state.log.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set(opts.ascii))
        .border_style(dim)
        .title(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    block.render(area, &mut buf);

    let padded = Rect::new(
        inner.x.saturating_add(pad_x),
        inner.y,
        inner.width.saturating_sub(2 * pad_x),
        inner.height,
    );
    Paragraph::new(lines).render(padded, &mut buf);
    buf
}

fn select_footer(
    width: u16,
    cycle: u64,
    start: usize,
    shown: usize,
    total: usize,
    opts: &RenderOptions,
) -> Option<String> {
    let candidates: Vec<String> = if shown < total {
        let pos = format!("{}-{}/{}", start + 1, start + shown, total);
        let arrows = if opts.ascii { "Up/Dn" } else { "↑↓" };
        vec![
            format!("siklus #{cycle} | {arrows} PgUp PgDn Home End: scroll {pos} | Ctrl+C keluar"),
            format!("siklus #{cycle} | {arrows} PgUp PgDn: scroll {pos}"),
            format!("{arrows} scroll {pos}"),
            pos,
        ]
    } else {
        let plat = opts.platform.to_ascii_lowercase();
        vec![
            format!("{VERSION} {plat} | siklus #{cycle} | Ctrl+C untuk keluar"),
            format!("siklus #{cycle} | Ctrl+C keluar"),
            format!("siklus #{cycle}"),
        ]
    };

    candidates
        .into_iter()
        .find(|t| t.chars().count() + 6 <= width as usize)
}

fn blit_buffer(src: &Buffer, dst: &mut Buffer, dst_x: u16, dst_y: u16, src_y: usize, rows: usize) {
    if dst_x < dst.area.left() || dst_x >= dst.area.right() {
        return;
    }
    let max_x = src.area.width.min(dst.area.right().saturating_sub(dst_x));
    for dy in 0..rows {
        let sy = src_y + dy;
        let Some(ty) = dst_y.checked_add(dy as u16) else {
            break;
        };
        if sy >= src.area.height as usize || ty >= dst.area.bottom() {
            break;
        }
        if ty < dst.area.top() {
            continue;
        }
        for dx in 0..max_x {
            let s_cell = &src[(src.area.x + dx, src.area.y + sy as u16)];
            let d_cell = &mut dst[(dst_x + dx, ty)];
            *d_cell = s_cell.clone();
        }
    }
}

/// Build the full unclipped virtual body buffer at width `inner_w`.
pub fn build_body_buffer(
    state: &DashboardState,
    inner_w: u16,
    view_h: Option<u16>,
    opts: &RenderOptions,
) -> Buffer {
    let sub = format!("Polban Auto Absen -- {} {VERSION}", opts.platform);
    let mut head = render_banner_buf(false, inner_w, &sub);
    let status = render_status_panel(state, inner_w, opts);
    let jadwal = render_jadwal_panel(state, inner_w, opts);
    let mut log = render_log_panel(state, inner_w, None, opts);

    let mut spare: isize = 0;
    if let Some(vh) = view_h {
        let used = head.area.height + status.area.height + jadwal.area.height + log.area.height + 2;
        spare = vh as isize - used as isize;
    }

    if inner_w as usize >= BANNER_WIDTH && (view_h.is_none() || spare > 0) {
        let big = render_banner_buf(true, inner_w, &sub);
        let extra = (big.area.height as isize) - (head.area.height as isize);
        if view_h.is_none() || extra <= spare {
            head = big;
            spare -= extra;
        }
    }

    if spare > 0 && opts.alt_screen && view_h.is_some() {
        let target_h = log.area.height.saturating_add(spare as u16);
        log = render_log_panel(state, inner_w, Some(target_h), opts);
    }

    let total_h =
        head.area.height + status.area.height + 1 + jadwal.area.height + 1 + log.area.height;
    let mut body = Buffer::empty(Rect::new(0, 0, inner_w, total_h));
    let mut y = 0u16;
    for part in [&head, &status] {
        blit_buffer(part, &mut body, 0, y, 0, part.area.height as usize);
        y += part.area.height;
    }
    y += 1; // gap
    blit_buffer(&jadwal, &mut body, 0, y, 0, jadwal.area.height as usize);
    y += jadwal.area.height + 1; // gap
    blit_buffer(&log, &mut body, 0, y, 0, log.area.height as usize);

    body
}

/// Pure render entrypoint testable with `ratatui::backend::TestBackend`.
pub fn render_dashboard(
    buf: &mut Buffer,
    area: Rect,
    state: &DashboardState,
    view: &mut ViewState,
    opts: &RenderOptions,
) {
    if area.width < 4 || area.height < 4 {
        return;
    }

    let roomy = area.width >= 80 && area.height >= 30;
    let pad_y: u16 = if roomy { 1 } else { 0 };
    let pad_x: u16 = if roomy { 2 } else { 1 };
    let max_content_w = area.width.saturating_sub(2 + pad_x).max(1);
    let inner_w = area
        .width
        .saturating_sub(2 + 2 * pad_x)
        .max(10)
        .min(max_content_w);
    let view_h = area.height.saturating_sub(2 + 2 * pad_y).max(1);

    let body = build_body_buffer(
        state,
        inner_w,
        if opts.full_render { None } else { Some(view_h) },
        opts,
    );
    let total = body.area.height as usize;
    let vh_usize = view_h as usize;

    let (start, shown) = if opts.full_render || total <= vh_usize {
        view.scroll = 0;
        view.max_scroll = 0;
        (0, total)
    } else {
        view.max_scroll = total - vh_usize;
        view.page = (vh_usize.saturating_sub(1)).max(1);
        if view.follow {
            view.scroll = view.max_scroll;
        }
        view.scroll = view.scroll.min(view.max_scroll);
        (view.scroll, vh_usize)
    };

    let scrolling = shown < total;
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set(opts.ascii))
        .border_style(Style::default().fg(Color::LightBlue))
        .title(Span::styled(
            "POLBAN AUTO ABSEN",
            Style::default().add_modifier(Modifier::BOLD),
        ));

    if let Some(footer) = select_footer(area.width, state.cycle_count, start, shown, total, opts) {
        block = block.title_bottom(
            Line::from(Span::styled(
                footer,
                Style::default().add_modifier(Modifier::DIM),
            ))
            .alignment(Alignment::Center),
        );
    }

    let outer_inner = block.inner(area);
    block.render(area, buf);

    let content_x = outer_inner.x.saturating_add(pad_x);
    let content_y = outer_inner.y.saturating_add(pad_y);
    let rows_to_copy = shown.min(outer_inner.height.saturating_sub(pad_y) as usize);
    blit_buffer(&body, buf, content_x, content_y, start, rows_to_copy);

    if scrolling && rows_to_copy > 0 {
        let thumb = ((rows_to_copy * rows_to_copy) as f64 / total as f64)
            .round()
            .max(1.0) as usize;
        let top = ((start * (rows_to_copy.saturating_sub(thumb))) as f64
            / (total.saturating_sub(rows_to_copy)).max(1) as f64)
            .round() as usize;
        let bar_x = outer_inner.right().saturating_sub(1);
        let on_sym = if opts.ascii { "#" } else { "█" };
        let off_sym = if opts.ascii { "|" } else { "│" };
        let on_style = Style::default().fg(Color::LightBlue);
        let off_style = Style::default().add_modifier(Modifier::DIM);

        for i in 0..rows_to_copy {
            let cell = &mut buf[(bar_x, content_y + i as u16)];
            if i >= top && i < top + thumb {
                cell.set_symbol(on_sym);
                cell.set_style(on_style);
            } else {
                cell.set_symbol(off_sym);
                cell.set_style(off_style);
            }
        }
    }
}

/// Print short summary after leaving alternate screen (parity with `Dashboard.render_summary`).
pub fn render_summary(state: &DashboardState) {
    let sub = platform::subtitle();
    let nama = if state.user_info.nama.is_empty() {
        "-"
    } else {
        &state.user_info.nama
    };
    let nim = if state.user_info.nim.is_empty() {
        "-"
    } else {
        &state.user_info.nim
    };
    println!();
    println!("{COMPACT_TITLE}  {sub}");
    println!("  Nama    : {nama}  (NIM {nim})");
    println!(
        "  Siklus  : {}   berhasil {}   gagal {}",
        state.cycle_count, state.stat_ok, state.stat_fail
    );
    println!("  Scan    : {}", state.last_scan_info);
    for mk in &state.jadwal {
        let label = mk.status.status_label();
        let dosen = if mk.dosen.is_empty() {
            String::new()
        } else {
            format!("  {}", mk.dosen)
        };
        println!("   - {} {}  {label}{dosen}", mk.nama, mk.jam);
    }
    println!();
}

/// Print full unclipped dashboard frame to stdout when exiting `--once` / `--no-alt-screen`.
fn print_full_frame_to_stdout(state: &DashboardState, width: u16, opts: &RenderOptions) {
    let mut full_opts = opts.clone();
    full_opts.full_render = true;
    full_opts.alt_screen = false;

    let pad_x: u16 = if width >= 80 { 2 } else { 1 };
    let inner_w = width.saturating_sub(2 + 2 * pad_x).max(10);
    let body = build_body_buffer(state, inner_w, None, &full_opts);
    let total_h = body.area.height
        + 2
        + if width >= 80 && body.area.height + 4 >= 30 {
            2
        } else {
            0
        };
    let area = Rect::new(0, 0, width, total_h);
    let mut buf = Buffer::empty(area);
    let mut view = ViewState::default();
    render_dashboard(&mut buf, area, state, &mut view, &full_opts);

    let mut out = io::stdout().lock();
    for y in 0..area.height {
        let mut line = String::with_capacity(area.width as usize);
        for x in 0..area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        let trimmed = line.trim_end();
        let _ = writeln!(out, "{trimmed}");
    }
    let _ = out.flush();
}

/// Run the interactive ratatui event loop until worker exits or user quits.
pub fn run_tui(
    event_rx: Receiver<EngineEvent>,
    shutdown_tx: Sender<()>,
    opts: TuiOptions,
) -> io::Result<DashboardState> {
    install_panic_hook();
    let guard = TerminalGuard::enter(opts.alt_screen, opts.ascii)?;

    let mut state = DashboardState::default();
    state.engine = opts.engine.clone();
    state.interval = opts.interval;

    let mut render_opts = RenderOptions {
        alt_screen: opts.alt_screen,
        ascii: opts.ascii,
        full_render: false,
        tanggal: format_tanggal_id(Local::now().date_naive()),
        platform: platform::platform_label().to_string(),
    };

    let backend = CrosstermBackend::new(io::stdout());
    let term_size = crossterm::terminal::size().unwrap_or((80, 24));
    let viewport = if opts.alt_screen {
        Viewport::Fullscreen
    } else {
        Viewport::Inline(term_size.1.clamp(1, 28))
    };
    let mut terminal = Terminal::with_options(backend, TerminalOptions { viewport })?;
    let mut view = ViewState::default();
    let mut dirty = true;
    let mut worker_done = false;
    let mut last_date_check = Instant::now();

    loop {
        loop {
            match event_rx.try_recv() {
                Ok(ev) => {
                    state.apply(ev);
                    dirty = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    worker_done = true;
                    break;
                }
            }
        }

        if last_date_check.elapsed() >= Duration::from_secs(60) {
            last_date_check = Instant::now();
            let now_tanggal = format_tanggal_id(Local::now().date_naive());
            if now_tanggal != render_opts.tanggal {
                render_opts.tanggal = now_tanggal;
                dirty = true;
            }
        }

        if dirty {
            terminal.draw(|frame| {
                let area = frame.area();
                render_dashboard(frame.buffer_mut(), area, &state, &mut view, &render_opts);
            })?;
            dirty = false;
        }

        if worker_done {
            break;
        }

        if ct_event::poll(Duration::from_millis(50))? {
            let mut should_quit = false;
            for _ in 0..64 {
                let ev = ct_event::read()?;
                if matches!(ev, ct_event::Event::Resize(_, _)) {
                    dirty = true;
                } else if let Some(action) = map_crossterm_event(&ev) {
                    match action {
                        UserAction::Quit => {
                            should_quit = true;
                        }
                        UserAction::Scroll(scroll) => {
                            view.apply_scroll(scroll);
                            dirty = true;
                        }
                    }
                }
                if !ct_event::poll(Duration::ZERO)? {
                    break;
                }
            }
            if should_quit {
                let _ = shutdown_tx.send(());
                break;
            }
        }
    }

    if !opts.alt_screen {
        let _ = terminal.clear();
    }
    drop(terminal);
    guard.restore();

    if !opts.alt_screen && io::stdout().is_terminal() {
        let width = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);
        print_full_frame_to_stdout(&state, width, &render_opts);
    } else if opts.alt_screen {
        render_summary(&state);
    }

    Ok(state)
}

/// Run plain reporter when TUI is disabled (`--plain` or non-TTY).
pub fn is_interactive_tty() -> bool {
    io::stdout().is_terminal() && io::stdin().is_terminal()
}

#[cfg(test)]
mod tests {
    use absen_core::UserInfo;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::dashboard::state::MataKuliah;

    fn sample_state() -> DashboardState {
        let mut st = DashboardState::default();
        st.user_info = UserInfo {
            nim: "251511001".into(),
            nama: "Budi Santoso".into(),
            jurusan: "D3-Teknik Informatika".into(),
        };
        st.auth_status = "OK -- Authenticated".into();
        st.last_scan_info =
            "07:05:00 (1 siap absen | 1 sudah absen | 1 belum buka | 0 belum waktunya)".into();
        st.engine = "http".into();
        st.interval = 60;
        st.countdown = 42;
        st.cycle_count = 3;
        st.stat_ok = 1;
        st.stat_fail = 0;
        st.jadwal = vec![
            MataKuliah {
                nama: "Struktur Data".into(),
                jam: "07:00:00 s.d. 09:30:00".into(),
                dosen: "Trisna Gelar Abdillah, S.T., M.Kom".into(),
                status: AbsenColor::Blue,
            },
            MataKuliah {
                nama: "Basis Data".into(),
                jam: "09:30:00 s.d. 12:00:00".into(),
                dosen: "Siti Aminah, M.T.".into(),
                status: AbsenColor::Green,
            },
            MataKuliah {
                nama: "Jaringan Komputer".into(),
                jam: "13:00:00 s.d. 15:30:00".into(),
                dosen: "Andi Wijaya".into(),
                status: AbsenColor::Orange,
            },
        ];
        use crate::dashboard::state::LogEntry;

        st.log.push_back(LogEntry {
            timestamp: "07:04:55".into(),
            code: "LOGIN".into(),
            message: "Authenticated".into(),
            level: LogLevel::Ok,
        });
        st.log.push_back(LogEntry {
            timestamp: "07:05:00".into(),
            code: "ABSEN".into(),
            message: "BERHASIL (percobaan 1): Basis Data".into(),
            level: LogLevel::Ok,
        });
        st
    }

    fn render_to_string(
        width: u16,
        height: u16,
        state: &DashboardState,
        view: &mut ViewState,
        opts: &RenderOptions,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                render_dashboard(f.buffer_mut(), area, state, view, opts);
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..height {
            let mut row = String::new();
            for x in 0..width {
                row.push_str(buf[(x, y)].symbol());
            }
            out.push_str(row.trim_end());
            out.push('\n');
        }
        out
    }

    fn fixed_opts(ascii: bool) -> RenderOptions {
        RenderOptions {
            alt_screen: true,
            ascii,
            full_render: false,
            tanggal: "Selasa, 17 November 2026".into(),
            platform: "Windows".into(),
        }
    }

    #[test]
    fn formats_indonesian_dates() {
        assert_eq!(
            format_tanggal_id(NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()),
            "Senin, 16 November 2026"
        );
        assert_eq!(
            format_tanggal_id(NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()),
            "Selasa, 17 November 2026"
        );
        assert_eq!(
            format_tanggal_id(NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()),
            "Jumat, 01 Januari 2027"
        );
    }

    #[test]
    fn renders_wide_120x40_with_big_banner_and_five_columns() {
        let st = sample_state();
        let mut view = ViewState::default();
        let screen = render_to_string(120, 40, &st, &mut view, &fixed_opts(false));

        assert!(screen.contains("POLBAN AUTO ABSEN"));
        assert!(screen.contains(r"\___/"));
        assert!(!screen.contains(COMPACT_TITLE));
        assert!(screen.contains("Selasa, 17 November 2026"));
        assert!(screen.contains("Budi Santoso"));
        assert!(
            screen.contains("No")
                && screen.contains("Jam")
                && screen.contains("Status")
                && screen.contains("Dosen")
        );
        assert!(screen.contains("07:00:00 s.d. 09:30:00"));
        assert!(screen.contains("SIAP ABSEN"));
        assert!(screen.contains("SUDAH ABSEN"));
        assert!(screen.contains("BELUM BUKA"));
        assert!(screen.contains("Log Aktivitas (2 terakhir)"));
        assert_eq!(view.max_scroll, 0);
        insta::assert_snapshot!("wide_120x40", screen);
    }

    #[test]
    fn renders_narrow_80x28_compacting_jam_under_matkul() {
        let st = sample_state();
        let mut view = ViewState::default();
        let screen = render_to_string(80, 28, &st, &mut view, &fixed_opts(false));

        assert!(screen.contains("07:00-09:30"));
        assert!(!screen.contains("07:00:00 s.d. 09:30:00"));
        assert!(screen.contains("Dosen"));
        assert!(screen.contains("Status"));
        insta::assert_snapshot!("narrow_80x28", screen);
    }

    #[test]
    fn renders_tiny_50x24_moving_dosen_under_matkul() {
        let st = sample_state();
        let mut view = ViewState::default();
        let opts = fixed_opts(false);
        let screen = render_to_string(50, 24, &st, &mut view, &opts);

        assert!(screen.contains(COMPACT_TITLE));
        assert!(screen.contains("Status"));
        insta::assert_snapshot!("tiny_50x24", screen);

        view.apply_scroll(ScrollAction::End);
        let scrolled = render_to_string(50, 24, &st, &mut view, &opts);
        assert!(scrolled.contains("Trisna Gelar"));
        assert!(scrolled.contains("07:00-09:30"));
    }

    #[test]
    fn renders_micro_40x20_with_scrollbar_and_scroll_navigation() {
        let st = sample_state();
        let mut view = ViewState::default();
        let opts = fixed_opts(false);
        let screen = render_to_string(40, 20, &st, &mut view, &opts);

        assert!(view.max_scroll > 0);
        assert!(screen.contains("█"));
        assert!(screen.contains("scroll 1-"));
        insta::assert_snapshot!("micro_40x20_top", screen);

        view.apply_scroll(ScrollAction::PageDown);
        let mid = render_to_string(40, 20, &st, &mut view, &opts);
        assert!(mid.contains("SIAP ABSEN"));

        view.apply_scroll(ScrollAction::End);
        let bottom = render_to_string(40, 20, &st, &mut view, &opts);
        assert_eq!(view.scroll, view.max_scroll);
        assert!(bottom.contains("Log Aktivitas (2 terakhir)"));
    }

    #[test]
    fn renders_ascii_mode_with_plus_borders_and_hash_scrollbar() {
        let st = sample_state();
        let mut view = ViewState::default();
        let screen = render_to_string(40, 20, &st, &mut view, &fixed_opts(true));

        assert!(screen.contains("+--"));
        assert!(screen.contains("#"));
        assert!(screen.contains("|"));
        assert!(!screen.contains("╭"));
        assert!(!screen.contains("█"));
    }

    #[test]
    fn renders_inline_viewport_with_nonzero_origin_without_clipping() {
        let st = sample_state();
        let mut view = ViewState::default();
        let mut opts = fixed_opts(false);
        opts.alt_screen = false;

        // area.top() (25) > area.height (22) would have clipped everything under the old bug.
        let area = Rect::new(0, 25, 80, 22);
        let mut buf = Buffer::empty(area);
        render_dashboard(&mut buf, area, &st, &mut view, &opts);

        let extract = |b: &Buffer| {
            let mut text = String::new();
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    text.push_str(b[(x, y)].symbol());
                }
                text.push('\n');
            }
            text
        };

        let top_text = extract(&buf);
        assert!(
            top_text.contains("Budi Santoso"),
            "inline origin failed:\n{top_text}"
        );
        assert!(
            top_text.contains("Jadwal Hari Ini"),
            "inline origin failed:\n{top_text}"
        );

        view.apply_scroll(ScrollAction::End);
        let mut buf2 = Buffer::empty(area);
        render_dashboard(&mut buf2, area, &st, &mut view, &opts);
        let bottom_text = extract(&buf2);
        assert!(
            bottom_text.contains("Struktur Data"),
            "inline scrolled failed:\n{bottom_text}"
        );
    }

    #[test]
    fn renders_extreme_small_and_unicode_without_panicking() {
        let mut st = sample_state();
        st.user_info.nama = "Éka Pratama — Ünicode 🚀".into();
        st.jadwal[0].nama = "Pemrograman Berorientasi Objek — Kelas Khusus 🚀".into();
        st.jadwal[0].dosen = "Dr. Ångström, M.Kom — Tim Pengajar".into();

        let opts = fixed_opts(false);
        for (w, h) in [(0, 0), (3, 3), (4, 4), (10, 6), (15, 10), (24, 12)] {
            let mut view = ViewState::default();
            let area = Rect::new(0, 0, w, h);
            let mut buf = Buffer::empty(area);
            render_dashboard(&mut buf, area, &st, &mut view, &opts);
        }
    }
}
