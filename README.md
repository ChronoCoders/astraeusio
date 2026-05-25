# Astraeusio

Real-time space weather intelligence platform with ML-powered geomagnetic storm prediction. Aggregates live data from NASA, NOAA, Kyoto WDC, Celestrak, and the ISS, stores it locally, and serves it through a low-latency dashboard with automated anomaly detection.

---

## Overview

Astraeusio monitors the near-Earth space environment continuously. A Rust backend polls thirteen external data feeds (across four providers) at fixed intervals, persists every record to an embedded DuckDB database, and serves all dashboard data from that local store. A Python LSTM model runs as a sidecar service and provides multi-horizon Kp forecasts (3h / 6h / 12h / 24h) with Monte Carlo uncertainty estimates. A React frontend polls the backend and renders live charts, maps, metric cards, and an anomaly feed.

Key capabilities:

- Live Kp index (1-minute and official 3-hour), solar wind speed and density, GOES X-ray flux, IMF Bz, Dst index, and space weather alerts from NOAA SWPC and Kyoto WDC
- Near-Earth asteroid feed (7-day lookahead), ISS position with altitude and velocity, and the Starlink constellation from Celestrak TLEs
- NASA APOD, EPIC Earth imagery, and Exoplanet Archive catalog
- Multi-horizon Kp forecast (3h / 6h / 12h / 24h), each with a 95% confidence interval derived from Monte Carlo Dropout
- Anomaly detection across five event types with warning/critical severity tiers, plus user-defined custom rules
- Events history, summary reports with CSV export, API keys, outbound webhooks, and email alerts
- Authentication: JWT + bcrypt, optional TOTP 2FA, email verification, and optional GitHub / Google OAuth
- Model Context Protocol (MCP) endpoint for agent access
- English and Turkish localizations

---

## Architecture

```
Browser
  └── React 19 (Vite / Tailwind)
        └── /api/* and /auth/* proxy → Rust backend (Axum :3000)
                                         ├── DuckDB (embedded, astraeus.duckdb)
                                         ├── async db_writer channel (batched, non-blocking writes)
                                         ├── ~15 Tokio background tasks (13 pollers + anomaly scan + forecast refresh)
                                         └── POST /predict → Python ML service (FastAPI :8000)
```

### Backend

Written in Rust (Edition 2024) using Axum 0.8 as the HTTP framework and Tokio as the async runtime. All external API calls use `reqwest` with a 60-second timeout and per-source retry logic (3 attempts, exponential backoff).

Route handlers never call external APIs directly. Every data type is maintained by a dedicated background Tokio task that fetches, validates, and enqueues writes to DuckDB through an async `db_writer` channel (batched and non-blocking). Handlers read from the database and serve responses in single-digit milliseconds. An in-process TTL cache (10–3600 seconds depending on the endpoint) prevents redundant DB reads under concurrent browser connections.

All floating-point values are stored as scaled 64-bit integers (`flux_e12`, `speed_e1`, `lat_e6`, etc.) and de-scaled on read. Timestamps are ISO-8601 UTC text for source time-tags and UTC Unix seconds (`i64`) for `fetched_at`. No external database server is required; DuckDB runs embedded in the same process.

Authentication uses HS256 JWT tokens issued on login and verified on protected routes. Passwords are hashed with bcrypt (default cost). Optional TOTP 2FA, email verification (via Resend), and GitHub / Google OAuth are supported; an OAuth provider with no configured client id/secret is automatically disabled.

**Background poller intervals (defaults, override via env):**

| Source | Interval |
|---|---|
| ISS position | 5 s |
| Kp index (1-min) | 60 s |
| Solar wind | 60 s |
| IMF (DSCOVR magnetometer) | 60 s |
| X-ray flux | 120 s |
| Space weather alerts | 300 s |
| Dst index | 300 s |
| Kp index (official 3-hour) | 30 min |
| ML Kp forecast refresh | 30 min |
| NEO feed | 30 min |
| EPIC imagery | 30 min |
| Starlink TLEs | 1 h |
| APOD | 1 h |
| Exoplanet catalog | 24 h |
| Anomaly scan | 60 s (initial delay 90 s) |

**Core dependencies:** `axum 0.8`, `tokio 1`, `duckdb 1` (bundled), `reqwest 0.13`, `serde_json 1`, `jsonwebtoken 10`, `bcrypt 0.15`, `totp-rs 5`, `tracing 0.1`, `anyhow 1`, `thiserror 2`, `dotenvy 0.15`

### ML Service

A FastAPI application (Python) that exposes a `POST /predict` endpoint and a `GET /health` endpoint reporting per-horizon validation metrics. The backend refreshes the forecast on a 30-minute poller (and on demand from `/api/kp-forecast`) and persists the returned prediction to the `kp_forecast` table for anomaly detection use. `torch` is pinned to the CPU build to keep the inference image small.

**Dependencies:** `torch` (CPU build), `fastapi`, `uvicorn`, `numpy`, `pandas`, `pyarrow`, `requests`

### Frontend

React 19 with Vite and Tailwind CSS. Data is fetched by a `useApi` hook that polls at 30-second (live data) or 120-second (slower-changing data) intervals. The Vite dev server proxies `/api/*` and `/auth/*` to `localhost:3000`. Localizations are managed with `i18next` (English and Turkish).

Authenticated pages: Dashboard, Forecast, Charts, Map, Alerts, Events, Reports, API Keys, Billing, Settings. Public pages: Landing, Products, Pricing, Docs, About, Blog.

---

## Data Sources

| Source | Base URL | Data |
|---|---|---|
| NOAA SWPC | `services.swpc.noaa.gov` | Kp index (1-min & official 3-hour), solar wind (RTSW 1-min), GOES X-ray (1-day), space weather alerts, IMF (DSCOVR magnetometer), Dst (Kyoto WDC relay) |
| NASA | `api.nasa.gov` | APOD, NeoWs 7-day feed, EPIC imagery, Exoplanet Archive |
| Open Notify | `api.open-notify.org` | ISS latitude, longitude, altitude, velocity |
| Celestrak | `celestrak.org` | Starlink constellation TLEs (3-line element sets) |

NOAA and Celestrak endpoints are public and require no API key. NASA requests use `NASA_API_KEY`; a DEMO_KEY is available for low-volume use.

---

## ML Model

### Architecture

Two-layer LSTM with a two-layer linear head and a four-output (multi-horizon) prediction.

```
Input: (batch, 16, 19)  — 16 time steps × 19 features
LSTM:  input_size=19, hidden_size=64, num_layers=2, dropout=0.2
Head:  Linear(64, 32) → ReLU → Linear(32, 4)
Output: Kp prediction for 4 horizons — 3h, 6h, 12h, 24h ahead
```

### Input Features

Each time step in the 48-hour input window (16 steps × 3 hours) carries 19 features:

| Group | Features |
|---|---|
| Kp (scaled) | `kp` (normalized to [0, 1]), `lag_1`…`lag_7`, `kp_24h_max`, `kp_72h_mean` |
| Cyclical time | `hour_sin/cos` (24 h), `month_sin/cos` (12 mo), `solar_cycle_phase_sin/cos` (period 4018.5 days, ref Dec 2019) |
| Physics drivers | `f107_adj` (F10.7 adjusted flux), `sn` (sunspot number), `f107_1d_delta` (24-hour F10.7 change) |

The Kp-scaled and time features are derived from the Kp history; the physics drivers are optional inputs (`f107`, `sunspot`) — when omitted, the server substitutes the feature defaults saved in the checkpoint. Min-max normalization constants for the physics features are stored in the checkpoint.

### Training

The model is trained on approximately 20 years of 3-hourly Kp data from GFZ Potsdam / NOAA, augmented with F10.7 and sunspot series, preprocessed to Parquet by `ml/preprocess.py`.

- **Loss function:** weighted HuberLoss across the four horizons (weights 1.0 / 0.8 / 0.6 / 0.4 — nearer horizons dominate)
- **Optimizer:** Adam, initial LR 1e-3
- **LR schedule:** ReduceLROnPlateau, factor 0.5, patience 3 epochs
- **Gradient clipping:** max norm 1.0
- **Early stopping:** patience 7 epochs
- **Batch size:** 512

Walk-forward validation retrains the model from scratch on all data preceding each fold and reports per-horizon RMSE/MAE. The final production model is trained on the full dataset; all normalization constants are saved in the checkpoint.

### Uncertainty Estimation

Uncertainty is derived from Monte Carlo Dropout. At inference time the model is kept in `train()` mode for 50 stochastic forward passes per horizon. Each horizon's 95% confidence interval is computed as mean ± 1.96 × standard deviation across passes, clipped to [0, 9].

### Inference API

```
POST /predict
Body:    { "readings": [float, ...] }              7–48 Kp values, oldest first
         optional "f107":    [float, ...]          F10.7 adjusted flux, same length
         optional "sunspot": [float, ...]          daily sunspot number, same length
Returns: {
           "forecast": [
             { "horizon_hours", "predicted_kp", "ci_lower", "ci_upper", "uncertainty" },
             …  one entry per horizon (3h / 6h / 12h / 24h)
           ],
           "n_mc_samples", "trained_through",
           // flat fields mirror the 3-hour horizon for backward compatibility:
           "predicted_kp", "ci_lower", "ci_upper", "uncertainty", "horizon_hours"
         }

GET /health   → model status + per-horizon validation metrics (RMSE / MAE for 3h/6h/12h/24h)
```

---

## Anomaly Detection

The anomaly scanner runs every 60 seconds as a background Tokio task. It queries the most recent values from each relevant table and writes triggered events to the `alerts_anomaly` table. Deduplication is enforced by a composite primary key `(anomaly_type, source_ref)`, so a sustained condition (e.g. an ongoing storm) produces one record per source time-tag rather than one per scan.

| Type | Condition | Warning | Critical |
|---|---|---|---|
| `kp_storm` | Latest Kp reading | Kp ≥ 5.0 | Kp ≥ 8.0 |
| `solar_wind_speed` | Latest proton speed | > 700 km/s | > 900 km/s |
| `xray_flare` | Latest 0.1–0.8 nm flux | ≥ 1×10⁻⁵ W/m² (M class) | ≥ 1×10⁻⁴ W/m² (X class) |
| `asteroid_close` | NEO miss distance (7-day window) | < 1 LD | < 0.5 LD |
| `ml_forecast_storm` | Most recent stored Kp forecast (3h) | Kp ≥ 5.0 | Kp ≥ 8.0 |

LD = Lunar Distance (384,400 km).

Anomalies are surfaced on the dashboard in a dedicated panel. Critical events are highlighted in red. The panel polls `GET /api/anomalies` every 30 seconds. Users can also define custom threshold rules via `/api/custom-rules`.

---

## API Reference

All routes are served on the backend at `BIND_ADDR` (default `0.0.0.0:3000`). Auth and public endpoints require no token; data and account endpoints require a valid JWT or API key in the `Authorization: Bearer <token>` header.

### Authentication

| Method | Path | Description |
|---|---|---|
| `POST` | `/auth/register` | Create account (email + password) |
| `POST` | `/auth/login` | Authenticate; returns JWT (or a 2FA partial token) |
| `POST` | `/auth/change-password` | Change password (requires current password) |
| `POST` | `/auth/forgot-password` · `/auth/reset-password` | Password reset flow |
| `POST` | `/auth/verify-email/{token}` · `/auth/resend-verification` | Email verification |
| `POST` | `/auth/2fa/setup` · `/verify` · `/disable` · `/login` | TOTP 2FA lifecycle |
| `GET` | `/auth/oauth/{provider}/start` · `/callback` | GitHub / Google social login |
| `GET` | `/api/auth/providers` | Lists configured OAuth providers (public) |

### Data (JWT or API key)

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/kp` · `/api/kp-3h` | Kp index, 1-minute and official 3-hour |
| `GET` | `/api/solar-wind` · `/api/imf` · `/api/dst` | Solar wind, IMF Bz, Dst index |
| `GET` | `/api/xray` · `/api/alerts` | GOES X-ray flux, space weather alerts |
| `GET` | `/api/iss` · `/api/starlink` | ISS position, Starlink constellation |
| `GET` | `/api/apod` · `/api/neo` · `/api/epic` · `/api/exoplanets` | NASA imagery & catalogs |
| `GET` | `/api/kp-forecast` | ML multi-horizon Kp forecast (3h/6h/12h/24h) |
| `GET` | `/api/forecast/history` · `/api/forecast/metrics` | Forecast history and accuracy metrics |
| `GET` | `/api/events` · `/api/anomalies` | Event history and live anomalies |
| `GET` | `/api/reports/summary` · `/export` · `/kp` · `/solar-wind` | Aggregations and CSV export |

### Public (no auth)

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/public/kp` · `/solar-wind` · `/forecast` | Landing-page live widgets |
| `GET` | `/health` · `/api/health` | Health checks |

### Account & integrations (JWT)

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/user/me` · `/api/usage` | Profile, plan, and usage stats |
| `POST` | `/api/user/plan` | Update plan tier |
| `GET POST DELETE` | `/api/keys` · `/api/keys/{id}` | API key management |
| `GET POST DELETE` | `/api/webhooks` · `/api/webhooks/{id}` | Outbound webhooks |
| `GET POST` | `/api/email-alerts` | Email alert thresholds |
| `GET POST DELETE` | `/api/custom-rules` · `/{id}` · `/{id}/toggle` | Custom anomaly rules |
| `POST` | `/mcp` | Model Context Protocol (JSON-RPC 2.0) |

---

## Testing

```bash
cd backend
cargo test                 # unit + integration tests
cargo clippy -- -D warnings # lint (must be clean)
```

The backend test suite covers anomaly severity thresholds (`anomaly.rs`) and DuckDB round-trips and forecast-metric scoring against an in-memory store (`db.rs`).

---

## Getting Started

### Prerequisites

- Rust 1.85 or later (Edition 2024)
- Python 3.11 or later
- Node.js 20 or later
- A NASA API key from [api.nasa.gov](https://api.nasa.gov)

### Backend

```bash
cd backend
cp .env.example .env       # fill in NASA_API_KEY and JWT_SECRET
cargo build --release
cargo run --release
```

The server listens on `0.0.0.0:3000` by default. On first start it creates `astraeus.duckdb` in the working directory and begins polling all data sources.

### ML Service

```bash
cd ml
pip install -r requirements.txt

# Download and preprocess historical Kp data (one-time)
python download_kp.py
python preprocess.py

# Train the model (produces ml/models/kp_lstm.pt)
python train.py

# Start the inference server
uvicorn serve:app --port 8000
```

The inference server must be running for `/api/kp-forecast` to return fresh data; if it is unavailable the backend serves the most recent cached forecast (annotated as degraded). All other endpoints function independently.

### Frontend

```bash
cd frontend
npm install
npm run dev       # dev server on :5173, proxies /api/* to :3000
npm run build     # production build to dist/
```

### Docker (all three services)

```bash
docker compose up --build      # builds and starts ml, backend, and frontend
```

`docker-compose.yml` runs the ML service, backend, and an nginx-served frontend, sharing a Docker volume for the DuckDB file and trained model.

---

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `NASA_API_KEY` | Yes | — | NASA Open APIs key. DEMO_KEY works for low-volume local use. |
| `JWT_SECRET` | Yes | — | Secret used to sign and verify JWT tokens. 64-char hex recommended. |
| `DB_PATH` | No | `astraeus.duckdb` | Path to the DuckDB file (use forward slashes on Windows). |
| `BIND_ADDR` | No | `0.0.0.0:3000` | Host and port the backend HTTP server binds to. |
| `ML_SERVICE_URL` | No | `http://localhost:8000` | Base URL of the FastAPI ML inference service. |
| `APP_URL` | No | `http://localhost:5173` | Public frontend base URL (email links, OAuth redirect base). |
| `RESEND_API_KEY`, `RESEND_FROM` | No | — | Enable transactional email (verification, alerts) via Resend. |
| `GITHUB_CLIENT_ID/SECRET`, `GOOGLE_CLIENT_ID/SECRET` | No | — | Enable the corresponding OAuth provider; unset = disabled. |
| `RUST_LOG` | No | — | Tracing log filter, e.g. `backend=info,warn`. |

Place these in a `.env` file in the `backend/` directory. The backend loads it automatically via `dotenvy`. Poller intervals are also overridable via env (e.g. `KP_INTERVAL`, `STARLINK_INTERVAL`).

---

## Deployment

Astraeusio runs as three processes, locally or via Docker Compose:

1. **ML service** (`uvicorn serve:app --port 8000`) — inference-only, stateless after model load
2. **Backend** (`cargo run --release`) — serves HTTP and manages all background polling
3. **Frontend** — build with `npm run build`, serve `dist/` from any static host or reverse proxy

Point your reverse proxy (nginx, Caddy) at port 3000 for API traffic and the `dist/` directory for static assets. The frontend build assumes API requests are served from the same origin under `/api/` and `/auth/`.

DuckDB stores all data in a single file (`astraeus.duckdb`). Back it up by copying the file while the backend is stopped, or use DuckDB's `EXPORT DATABASE` for a portable snapshot. The ML model file (`ml/models/kp_lstm.pt`) is kept alongside the inference service; if the ML service is unavailable, `/api/kp-forecast` falls back to the cached forecast and all other endpoints continue to function normally.

---

## License

Astraeusio is licensed under the [Business Source License 1.1](LICENSE).

**Change Date:** 2029-04-20
**Change License:** Apache License, Version 2.0

Until the Change Date, production use requires a commercial license from ChronoCoders. After the Change Date, the software is available under the Apache 2.0 license. Non-production use (evaluation, development, testing, research) is permitted under BSL 1.1 without restriction.
