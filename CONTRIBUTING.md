# Contributing

Terima kasih ingin berkontribusi. Proyek ini menjaga kesederhanaan, paritas perilaku, dan keandalan binary tunggal.

## Prinsip

1. **Clean code & performa** — fungsi kecil satu tugas, komentar untuk "kenapa" bukan "apa", hindari alokasi atau dependency berat yang tidak perlu (`LazyLock` untuk selector, scan ASCII langsung).
2. **Jangan hardcode kredensial** — selalu lewat `.env`, jangan pernah commit `.env`, `.clicked.json`, atau `*-dump.html`, dan pastikan password selalu di-redact pada output `Debug`/log.
3. **Tes offline saja** — semua tes harus memakai HTML fixture sintetis dan mock HTTP server lokal (`httpmock`), jangan pernah menghubungi `akademik.polban.ac.id` dari test suite.

## Struktur

- `crates/absen-core/` — library inti (`config`, `state`, `model`, `text`, `parser`, `client`, `engine`, `event`).
- `crates/absen-cli/` — binary `polban-absen` (`main`, `platform`, `dashboard/{state,plain,keys,tui}`).
- `windows/`, `linux/`, `termux/` — launcher & installer per platform untuk binary `polban-absen`.
- `extension/` — Chrome MV3 (tetap JavaScript).
- `legacy/python/` — arsip implementasi v2.0 (tidak lagi dirawat).

## Gate Sebelum Commit

Jalankan semua gate berikut dari root repository:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Untuk verifikasi build release:

```bash
cargo build --release --bin polban-absen
```

Untuk perubahan pada `extension/`:

```bash
node --check extension/*.js
```

## Melaporkan Bug

Sertakan: versi (`polban-absen --version`), OS/terminal, log dari dashboard/`--plain`, dan struktur HTML yang sudah dianonimkan (via `--dump`) bila terkait parsing tabel `#jadwal`.
