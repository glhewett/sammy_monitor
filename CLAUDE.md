# CLAUDE.md

## Project overview

Sammy Monitor is a single-binary HTTP uptime monitor in Rust. It periodically GETs a list of URLs, persists results to SQLite, and sends email alerts via direct SMTP. There is no web server, no Prometheus exposition, and no external services required beyond an SMTP relay.

## Source layout

```
src/
  main.rs       — CLI entry point (clap), loads settings, opens DB, starts worker
  lib.rs        — re-exports db, settings, worker modules
  settings.rs   — MonitorConfig, Settings (TOML), SmtpConfig (env vars)
  worker.rs     — async monitoring loop, alert state machine, email dispatch
  db.rs         — SQLite schema + insert via rusqlite
tests/
  integration_metrics_test.rs — DB open/insert integration tests
```

## Key commands

```bash
cargo build --release          # production binary → target/release/sammy_monitor
cargo run -- --settings ./settings.toml   # dev run (reads .env and .env.local)
cargo test                     # unit + integration tests
RUST_LOG=debug cargo run -- --settings ./settings.toml
```

## Architecture notes

**Worker loop** (`worker.rs`): runs every 60 seconds, fires an HTTP GET for each monitor whose interval has elapsed, records results via `db.rs`, and drives a per-monitor state machine for alerting.

**Alert state machine** per monitor: `Unknown → FailingNoAlert{n} → Down → Up`. A DOWN email fires when `consecutive_failures >= down_alert_threshold`. A RECOVERY email fires on the first success after `Down`.

**Monitor identity**: `MonitorConfig::id()` derives a UUID v5 from the URL using `Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes())`. Same URL = same UUID across every run. The `id` field is not present in `settings.toml`.

**SMTP**: loaded from environment variables (`SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS`, `SMTP_FROM`, `SMTP_TO`, `SMTP_TLS`). If any required var is missing, email is silently disabled. `.env` and `.env.local` are loaded automatically at startup via `dotenvy`; `.env.local` overrides `.env` and is gitignored.

**SQLite**: opened at startup via `db_path` from settings (default `./sammy_monitor.db`). Schema is created on first open. Each check result row includes monitor_id, timestamp (RFC 3339), success flag, response time (ms), status code, and error message.

## Configuration

`settings.toml` fields:

| Key | Default | Description |
|---|---|---|
| `db_path` | `./sammy_monitor.db` | SQLite file path |
| `down_alert_threshold` | `5` | Consecutive failures before DOWN alert |
| `monitors` | — | Array of monitor blocks |

Per-monitor fields: `name`, `url`, `interval` (minutes), `enabled`.

## Dependencies of note

- `reqwest` 0.11 — HTTP client (30s timeout, custom User-Agent)
- `rusqlite` 0.31 bundled — SQLite
- `lettre` 0.11 — async SMTP email
- `uuid` 1.7 features `v4, v5, serde` — v5 for deterministic IDs
- `clap` 4.5 — CLI argument parsing
- `dotenvy` — `.env` / `.env.local` loading
- `tracing` / `tracing-subscriber` — structured logging
