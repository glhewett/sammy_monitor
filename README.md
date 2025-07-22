# Sammy Monitor

[![CI](https://github.com/glhewett/sammy_monitor/actions/workflows/ci.yml/badge.svg)](https://github.com/glhewett/sammy_monitor/actions/workflows/ci.yml)

![Sammy Monitor Logo](./sammy-logo.svg)

## Summary

Sammy Monitor is a high-performance HTTP monitoring service built with Rust and Axum. Named in tribute to the legendary entertainer Sammy Davis Jr., this tool keeps a watchful eye on your web services with style and reliability.

**Key Features:**
- **Multi-threaded Architecture**: Independent worker threads for monitoring, separate from web server
- **Configurable Intervals**: Per-monitor timing (1 minute, 2 minutes, 5 minutes, etc.)
- **Real-time Logging**: Detailed success/failure reporting with response times
- **Prometheus Metrics**: Built-in metrics endpoint for monitoring integration
- **Web Dashboard**: Clean HTML interface showing monitor status
- **Robust Configuration**: TOML-based settings with comprehensive validation
- **Health Checking**: Dedicated health endpoint for service monitoring

## Getting Started

### Prerequisites

- **Rust** (1.70+ recommended)
- **Git**

### Installation

1. **Clone the repository:**
   ```bash
   git clone https://github.com/glhewett/sammy_monitor.git
   cd sammy_monitor
   ```

2. **Create your configuration:**
   ```bash
   cp settings.sample.toml settings.toml
   ```

3. **Edit your settings:**
   ```bash
   # Edit settings.toml to add your monitors
   nano settings.toml
   ```
   
   Example configuration:
   ```toml
   [[monitors]]
   id = "550e8400-e29b-41d4-a716-446655440000"
   name = "My Website"
   url = "https://example.com"
   interval = 5  # Check every 5 minutes
   enabled = true
   ```

4. **Build the project:**
   ```bash
   cargo build --release
   ```

5. **Run the server:**
   ```bash
   cargo run --release -- --settings ./settings.toml
   ```

### Accessing the Services

Once running, Sammy Monitor provides several endpoints:

- **Web Dashboard**: http://localhost:3000/
- **Health Check**: http://localhost:3000/health  
- **Metrics**: http://localhost:3001/metrics (Prometheus format)

### Monitoring Output

The worker will continuously monitor your configured endpoints and log results:

```
INFO  Worker started with 2 monitors
INFO  Checking 2 monitors due for testing
✓ My Website (https://example.com) - OK in 145ms [200]
✓ API Server (https://api.example.com) - OK in 267ms [200]
INFO  Worker completed in 1250ms, sleeping for 58750ms
```

### Configuration Options

Each monitor supports the following settings:

- **`id`**: Unique UUID identifier
- **`name`**: Human-readable monitor name
- **`url`**: HTTP/HTTPS URL to monitor
- **`interval`**: Check frequency in minutes (1, 2, 5, 10, etc.)
- **`enabled`**: Whether monitoring is active (true/false)

### Development

Run tests:
```bash
cargo test
```

Run with debug logging:
```bash
RUST_LOG=debug cargo run -- --settings ./settings.toml
```

---

*Named in honor of Sammy Davis Jr. - a legendary performer who never missed a beat. Just like this monitor won't miss checking your services.*