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
- **Email Alerting**: Automated email notifications for service failures and recoveries
- **Advanced Queries**: Pre-calculated recording rules for better performance
- **Robust Configuration**: TOML-based settings with comprehensive validation

## 🚀 Quick Start (Docker)

Get up and running in under 2 minutes with the complete monitoring stack:

```bash
# 1. Clone the repository
git clone https://github.com/glhewett/sammy_monitor.git
cd sammy_monitor

# 2. Create your configuration from the sample
cp settings.sample.toml settings.toml

# 3. Configure email settings (optional)
# Edit alertmanager.yml to set your SMTP server and email addresses

# 4. Edit settings.toml to add your websites to monitor
# (Use any text editor to add your URLs, intervals, etc.)

# 5. Start the complete monitoring stack
# The settings.toml file will be automatically mounted into the container
docker-compose up -d

# 6. Access your services:
# • Sammy Monitor Metrics: http://localhost:3000/metrics
# • Prometheus: http://localhost:9090
# • Alertmanager: http://localhost:9093
```

That's it! The system will automatically:
- Build and start the Sammy Monitor service
- Start Prometheus to collect metrics and evaluate alerts
- Start Alertmanager for email notifications
- Begin monitoring your configured websites
- Send email alerts when services fail for >5 minutes

## Email Alerts

The system includes comprehensive email alerting:

### Alert Types:
- **🚨 CRITICAL**: Service down >5 minutes, very slow responses (>10s), SLA breaches
- **⚠️ WARNING**: High error rates (>10%), slow responses (>5s)
- **ℹ️ INFO**: Service recovery notifications, high traffic alerts

### Configuration:
Edit `alertmanager.yml` to configure your SMTP settings:

```yaml
global:
  smtp_smarthost: 'your-smtp-server:587'
  smtp_from: 'alerts@your-company.com'
  smtp_auth_username: 'your-email@your-company.com'
  smtp_auth_password: 'your-password'
```

## Prometheus Queries

The system includes pre-calculated recording rules for efficient querying:

### Useful Queries:

**Service Status:**
```promql
# Current status (1=up, 0=down)
http_monitor_up

# Services currently down
http_monitor_up == 0
```

**Uptime Calculations:**
```promql
# 5-minute uptime percentage
monitor_uptime_5m

# 24-hour uptime percentage  
monitor_uptime_24h

# 7-day uptime percentage
monitor_uptime_7d
```

**Response Times:**
```promql
# 50th percentile response time
monitor_response_time_p50

# 95th percentile response time
monitor_response_time_p95

# 99th percentile response time
monitor_response_time_p99

# Average response time over 5 minutes
monitor_avg_response_time_5m
```

**Error Rates:**
```promql
# Error rate over last 5 minutes
monitor_error_rate_5m

# Error rate over last hour
monitor_error_rate_1h
```

**Request Rates:**
```promql
# Requests per second over 5 minutes
monitor_request_rate_5m

# Total requests (all time)
increase(http_monitor_requests_total[24h])
```

**SLA Tracking:**
```promql
# Monthly SLA compliance (30 days)
monitor_sla_monthly

# Services below 99.5% SLA
monitor_sla_monthly < 99.5
```

### Advanced Queries:

**Service Health Overview:**
```promql
# All services with their current status and uptime
{__name__=~"monitor_uptime_24h|http_monitor_up"}
```

**Performance Issues:**
```promql
# Services with slow response times (>2s P95)
monitor_response_time_p95 > 2

# Services with high error rates (>5%)
monitor_error_rate_5m > 5
```

**Alert Status:**
```promql
# Currently firing alerts
ALERTS{alertstate="firing"}

# Alert history
ALERTS_FOR_STATE
```

## Getting Started

### Prerequisites

- **Rust** (1.88+ recommended) 
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

## Docker Installation

### Quick Start with Docker

1. **Pull the latest image:**

   ```bash
   docker pull ghcr.io/glhewett/sammy_monitor:latest
   ```

2. **Create your configuration:**

   ```bash
   cp settings.sample.toml settings.toml
   # Edit settings.toml to add your monitors
   ```

3. **Run with Docker:**
   ```bash
   docker run -d \
     --name sammy_monitor \
     -p 3000:3000 \
     -v $(pwd)/settings.toml:/app/settings.toml:ro \
     ghcr.io/glhewett/sammy_monitor:latest
   ```

   **Note**: The `settings.toml` file must be mounted as a volume. The container does not include any settings file by default.

### Docker Compose

For a complete monitoring stack with Prometheus and Alertmanager:

1. **Clone and configure:**

   ```bash
   git clone https://github.com/glhewett/sammy_monitor.git
   cd sammy_monitor
   cp settings.sample.toml settings.toml
   # Edit settings.toml and alertmanager.yml
   ```

2. **Start the stack:**
   ```bash
   # The settings.toml file is automatically mounted as a volume
   docker-compose up -d
   ```

This provides:

- **Sammy Monitor**: http://localhost:3000/metrics
- **Prometheus**: http://localhost:9090  
- **Alertmanager**: http://localhost:9093

### Building Custom Images

Use the provided build script for easy image creation:

```bash
# Build Alpine-based image (default)
./build.sh

# Build with custom tag
./build.sh --tag v1.0.0

# Build and push to registry
./build.sh --push --tag v1.0.0
```

The image is Alpine-based for minimal size (~20-50MB) while maintaining full functionality.

### Environment Variables

Configure the container using environment variables:

- `RUST_LOG`: Set logging level (debug, info, warn, error)
- Settings file location can be customized via command args

### Accessing the Services

Once running, Sammy Monitor provides:

- **Metrics Endpoint**: http://localhost:3000/metrics (Prometheus format)

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

## Alerting Rules

The system includes comprehensive alerting rules:

- **ServiceDown**: Triggers when service is down >5 minutes
- **HighErrorRate**: Triggers when error rate >10% for >10 minutes  
- **SlowResponse**: Triggers when P95 response time >5s for >5 minutes
- **VerySlowResponse**: Triggers when P95 >10s for >2 minutes (critical)
- **SLABreach**: Triggers when monthly uptime <99.5%
- **ServiceRecovered**: Fires when service comes back online

All rules include detailed descriptions and are routed to appropriate email channels based on severity.

---

_Named in honor of Sammy Davis Jr. - a legendary performer who never missed a beat. Just like this monitor won't miss checking your services._
