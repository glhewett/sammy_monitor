# Sammy Monitor

[![CI](https://github.com/glhewett/sammy_monitor/actions/workflows/ci.yml/badge.svg)](https://github.com/glhewett/sammy_monitor/actions/workflows/ci.yml)

![Sammy Monitor Logo](./sammy-logo.svg)

## Summary

Sammy Monitor is a lightweight HTTP uptime monitor written in Rust. It periodically checks a list of URLs, persists results to a local SQLite database, and sends email alerts when a service goes down or recovers. There are no external dependencies — no Prometheus, no Alertmanager, no metrics server.

**Key Features:**

- **Zero external services**: single binary + SQLite + SMTP
- **Configurable intervals**: per-monitor check frequency in minutes
- **SQLite persistence**: every check result is stored locally
- **Email alerting**: down and recovery alerts via direct SMTP (`lettre`)
- **Threshold-based alerts**: configurable consecutive-failure count before alerting
- **Auto-derived IDs**: monitor identity is derived from the URL — no UUID generation required
- **TOML configuration**: simple, human-readable settings file

## Quick Start

### Run locally

```bash
# 1. Clone
git clone https://github.com/glhewett/sammy_monitor.git
cd sammy_monitor

# 2. Create config
cp settings.sample.toml settings.toml
# Edit settings.toml — add your URLs, set intervals

# 3. Configure SMTP (optional — alerts are disabled if omitted)
cp .env .env.local
# Edit .env.local with your real SMTP credentials

# 4. Build and run
cargo build --release
./target/release/sammy_monitor --settings ./settings.toml
```

### Run with Docker

```bash
cp settings.sample.toml settings.toml
# Edit settings.toml

docker-compose up -d
```

The `docker-compose.yml` mounts `settings.toml` read-only and persists the SQLite database in a named volume.

## Configuration

### `settings.toml`

```toml
# Path to the SQLite database (default: ./sammy_monitor.db)
db_path = "./sammy_monitor.db"

# Consecutive failures before a DOWN alert is sent (default: 5)
down_alert_threshold = 5

[[monitors]]
name = "My Website"
url  = "https://example.com"
interval = 5      # minutes between checks
enabled  = true

[[monitors]]
name = "API"
url  = "https://api.example.com/health"
interval = 1
enabled  = true
```

No `id` field is needed — each monitor's identity is automatically derived from its URL.

### Monitor fields

| Field | Required | Description |
|---|---|---|
| `name` | yes | Human-readable label |
| `url` | yes | HTTP/HTTPS URL to check |
| `interval` | yes | Check frequency in minutes |
| `enabled` | yes | `true` / `false` |

### SMTP environment variables

Set these in `.env.local` (or export them directly):

| Variable | Required | Description |
|---|---|---|
| `SMTP_HOST` | yes | SMTP server hostname |
| `SMTP_PORT` | yes | Port (587 for STARTTLS, 465 for TLS) |
| `SMTP_USER` | yes | Login username |
| `SMTP_PASS` | yes | Password |
| `SMTP_FROM` | yes | Sender address |
| `SMTP_TO` | yes | Recipient(s), comma-separated |
| `SMTP_TLS` | no | `true` (default) or `false` |

If any required variable is missing, email alerts are disabled and a warning is logged at startup. HTTP monitoring continues normally.

## Alerting

Sammy Monitor uses a threshold-based state machine per monitor:

1. First failure → `FailingNoAlert` state, failure counter starts
2. Each subsequent failure increments the counter
3. When `down_alert_threshold` consecutive failures are reached → **DOWN alert sent**, state moves to `Down`
4. Next success after `Down` → **RECOVERY alert sent**, state moves to `Up`

With `interval = 1` and `down_alert_threshold = 5`, an alert fires approximately 5 minutes after a service becomes unreachable.

## Monitoring Output

```
INFO  Worker started with 2 monitors
INFO  Checking 2 monitors due for testing
✓ My Website (https://example.com) - OK in 145ms [200]
✓ API (https://api.example.com/health) - OK in 267ms [200]
INFO  Worker completed in 412ms, sleeping for 59588ms
```

## Development

```bash
# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run -- --settings ./settings.toml

# Build release binary
cargo build --release
```

## Docker

The `docker-compose.yml` runs a single container:

- `settings.toml` mounted at `/app/settings.toml` (read-only)
- SQLite database persisted in a `sammy_db` named volume
- `RUST_LOG=info` by default

To customize the log level or pass a different settings path, edit the `environment` and `command` sections of `docker-compose.yml`.

---

_Named in honor of Sammy Davis Jr. — a legendary performer who never missed a beat. Just like this monitor won't miss checking your services._
