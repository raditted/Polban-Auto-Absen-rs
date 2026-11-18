# Polban Presensi: Desktop GUI Design System

> Reading this as: Native desktop attendance companion for Polban students, in a Calm Academic Utility visual language, dial ENERGY 1 / RHYTHM 2 / MOTION 1.

## 1. Visual Dials & Aesthetic Direction

- **ENERGY 1 (Calm Utility)**: Restrained neutral surfaces and a single institutional accent keep attention on class schedule status rather than decorative chrome.
- **RHYTHM 2 (Structured Clarity)**: Predictable card groupings, aligned metadata rows, and clear visual weight hierarchy support rapid morning scanning.
- **MOTION 1 (Functional Feedback)**: Instant hover/focus transitions and deterministic status dot changes without distracting bounces or idle animations.

---

## 2. Color Tokens & WCAG 2.x Contrast Verification

Core palette uses three neutral steps (`bg_canvas`, `bg_surface`, `bg_subtle`), one institutional accent (**Polban Cobalt** `#1D4ED8`), and four semantic statuses (Emerald, Amber, Red, Blue). Every text pair meets `>= 4.5:1` (normal text) and every interactive control border and status badge border meets `>= 3.0:1` (R-25).

### 2.1 Light Mode Tokens

| Token | Hex | Role | Contrast Check (WCAG 2.x) |
| :--- | :--- | :--- | :--- |
| `bg_canvas` | `#F8FAFC` | Window background | Base canvas |
| `bg_surface` | `#FFFFFF` | Cards, panels, inputs | Base elevated surface |
| `bg_subtle` | `#F1F5F9` | Secondary headers, zebra/hover fill | Subtle elevation |
| `text_primary` | `#0F172A` | Course names, headings, NIM | `17.85:1` vs `#FFFFFF`, `17.06:1` vs `#F8FAFC` |
| `text_secondary` | `#475569` | Lecturer names, timestamps, labels | `7.58:1` vs `#FFFFFF`, `7.24:1` vs `#F8FAFC` |
| `border_subtle` | `#CBD5E1` | Non-interactive card dividers | Structural separation only |
| `border_interactive` | `#738296` | Inputs, buttons, focusable controls | `3.92:1` vs `#FFFFFF`, `3.74:1` vs `#F8FAFC`, `3.57:1` vs `#F1F5F9` |
| `accent` | `#1D4ED8` | Primary Polban Cobalt CTA & focus ring | `6.70:1` vs `#FFFFFF` |
| `accent_hover` | `#1E40AF` | Primary CTA hover fill | `8.72:1` vs `#FFFFFF` |
| `on_accent` | `#FFFFFF` | Text/icon on `accent` fill | `6.70:1` vs `#1D4ED8` |

### 2.2 Dark Mode Tokens

| Token | Hex | Role | Contrast Check (WCAG 2.x) |
| :--- | :--- | :--- | :--- |
| `bg_canvas` | `#0B1120` | Window background | Base dark canvas |
| `bg_surface` | `#111827` | Cards, panels, inputs | Base elevated dark surface |
| `bg_subtle` | `#1E293B` | Secondary headers, hover fill | Subtle dark elevation |
| `text_primary` | `#F8FAFC` | Course names, headings, NIM | `16.96:1` vs `#111827`, `18.00:1` vs `#0B1120` |
| `text_secondary` | `#94A3B8` | Lecturer names, timestamps, labels | `6.92:1` vs `#111827`, `7.34:1` vs `#0B1120`, `5.71:1` vs `#1E293B` |
| `border_subtle` | `#334155` | Non-interactive card dividers | Structural separation only |
| `border_interactive` | `#64748B` | Inputs, buttons, focusable controls | `3.73:1` vs `#111827`, `3.96:1` vs `#0B1120`, `3.07:1` vs `#1E293B` |
| `accent` | `#1D4ED8` | Primary Polban Cobalt button fill | `6.70:1` with `#FFFFFF` text |
| `accent_hover` | `#2563EB` | Primary CTA hover fill | `5.17:1` with `#FFFFFF` text |
| `accent_text` | `#60A5FA` | Links, active tab indicator, focus ring | `6.98:1` vs `#111827`, `7.41:1` vs `#0B1120` |
| `on_accent` | `#FFFFFF` | Text/icon on `accent` fill | `6.70:1` vs `#1D4ED8` |

### 2.3 Semantic Status Tokens (Jadwal & Engine State)

| Status | Light (`text` / `bg` / `border`) | Light Contrast | Dark (`text` / `bg` / `border`) | Dark Contrast |
| :--- | :--- | :--- | :--- | :--- |
| **Emerald** (Sudah Absen / OK) | `#047857` / `#ECFDF5` / `#059669` | Text `5.21:1`, Border `3.77:1` (`#FFFFFF`), `3.58:1` (`#ECFDF5`) | `#34D399` / `#064E3B` / `#10B981` | Text `5.06:1`, Border `6.99:1` (`#111827`), `3.83:1` (`#064E3B`) |
| **Amber** (Belum Buka / Warn) | `#B45309` / `#FFFBEB` / `#D97706` | Text `4.84:1`, Border `3.19:1` (`#FFFFFF`), `3.07:1` (`#FFFBEB`) | `#FBBF24` / `#451A03` / `#F59E0B` | Text `8.97:1`, Border `8.26:1` (`#111827`), `6.97:1` (`#451A03`) |
| **Red** (Belum Waktunya / Error) | `#B91C1C` / `#FEF2F2` / `#DC2626` | Text `5.91:1`, Border `4.83:1` (`#FFFFFF`), `4.41:1` (`#FEF2F2`) | `#F87171` / `#450A0A` / `#EF4444` | Text `5.84:1`, Border `4.71:1` (`#111827`), `4.29:1` (`#450A0A`) |
| **Blue** (Siap Absen / Aktif) | `#1D4ED8` / `#EFF6FF` / `#2563EB` | Text `6.16:1`, Border `5.17:1` (`#FFFFFF`), `4.75:1` (`#EFF6FF`) | `#BFDBFE` / `#1E3A8A` / `#60A5FA` | Text `7.29:1`, Border `6.98:1` (`#111827`), `4.07:1` (`#1E3A8A`) |

**R-31 Color Rationale**: Neutral slate backgrounds paired with Polban Cobalt (`#1D4ED8`) anchor institutional identity while reserving saturated Emerald, Amber, and Red exclusively for actionable attendance states.

---

## 3. Typography & Font Bundling

- **Primary UI & Headings**: `Plus Jakarta Sans` (`Regular` 400 and `SemiBold` 600), bundled as static OFL TTFs via `include_bytes!`.
- **Monospace Data**: `JetBrains Mono` (`Regular` 400), bundled as static OFL TTF via `include_bytes!`, reserved strictly for NIM, time ranges (`07:00-09:30`), countdown seconds (`59s`), and cycle counters.

| Role | Family & Weight | Size (pt/px) | Usage |
| :--- | :--- | :--- | :--- |
| `display_title` | Plus Jakarta Sans SemiBold | `18px` | App title, modal headings |
| `section_heading` | Plus Jakarta Sans SemiBold | `14px` | Panel headers (Jadwal Kuliah, Aktivitas, Pengaturan) |
| `body` | Plus Jakarta Sans Regular | `13px` | Course names, lecturer names, log messages |
| `caption` | Plus Jakarta Sans Regular | `11.5px` | Subtitles, helper hints, badge text |
| `mono_metric` | JetBrains Mono Regular | `18px` | Countdown timer in header |
| `mono_data` | JetBrains Mono Regular | `12.5px` | NIM, course time range, log timestamps |

**R-31 Typography Rationale**: Pairing Plus Jakarta Sans (an Indonesian-designed geometric humanist sans) with JetBrains Mono separates readable Indonesian labels from fixed-width numeric schedules that never jitter during countdown ticks.

---

## 4. Spacing Scale & Hierarchical Corner Radius

- **Spacing Scale** (4px base unit): `4px` (tight badge/icon gap), `8px` (compact sibling gap), `12px` (internal row padding), `16px` (card padding & grid gutter), `20px` (panel separation), `24px` (outer window margin at wide width).
- **Hierarchical Corner Radius** (R-11, avoiding uniform pill shapes across unrelated controls):
  - `8px`: Top-level cards, schedule container, activity panel, settings modal.
  - `6px`: Buttons, text inputs, course list rows, toggle controls.
  - `4px`: Status tags (`SIAP ABSEN`, `SUDAH ABSEN`, `BELUM BUKA`, `BELUM WAKTUNYA`) and log severity chips.

**R-31 Spacing & Radius Rationale**: Nested elements step down in radius (`8px` container to `6px` interactive row/button to `4px` inline status tag) so visual containment reads immediately without heavy drop shadows.

---

## 5. Responsive Layout Breakpoints

| Breakpoint | Width Range | Layout Structure | Header & Controls Behavior |
| :--- | :--- | :--- | :--- |
| **Wide** | `>= 860px` | Two-column split below header: Jadwal Kuliah (`60%`) on left, Aktivitas (`40%`) on right, `16px` gutter. | Single-row header with student identity (Name, NIM, Prodi), status pill, countdown metric, and primary action buttons (`Mulai`/`Berhenti`, `Scan Sekarang`, `Pengaturan`). |
| **Medium** | `580px .. 859px` | Single-column vertical stack: Jadwal Kuliah above Aktivitas (`16px` vertical gap). | Two-row header: identity and status on row 1; countdown and action buttons on row 2. |
| **Compact** | `< 580px` | Single-column tight padding (`12px`), segmented view switcher (`Jadwal` / `Aktivitas`) to keep schedule rows visible above the fold. | Compact header displaying NIM, status tag, condensed time ranges (`07:00-09:30`), and full-width primary toggle button. |

**R-31 Layout Rationale**: Keeping Jadwal Kuliah dominant in wide and medium viewports and using a switcher below `580px` ensures students see live class attendance status without scrolling past secondary logs.

---

## 6. Iconography & App Identity

- **Vector UI Icons** (`crates/absen-gui/src/ui/icons.rs`): Crisp 1.5px stroke procedural `egui::Painter` icons (play, stop, refresh/scan, settings gear, eye show/hide, status check/alert) rather than raw emoji, keeping rendering identical across Windows, macOS, and Linux.
- **Window & Tray App Icon** (`crates/absen-gui/src/icon.rs`): Deterministic 32x32 and 64x64 RGBA rasterizer rendering a Polban Cobalt (`#1D4ED8`) rounded square (`radius ~20%`) with crisp geometric white `PA` monogram and an optional bottom-right status badge (`Idle` slate, `Monitoring` emerald, `Error` red) with a 1px contrast border.

**R-31 Icon Rationale**: Procedural vector strokes and an embedded RGBA monogram guarantee zero missing glyph boxes across Linux/macOS/Windows while conveying live monitoring state directly in the OS system tray.
