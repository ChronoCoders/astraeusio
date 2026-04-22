# Astraeusio

Real-time space weather intelligence platform with ML-powered geomagnetic storm prediction. Aggregates live data from NASA, NOAA, and the ISS, stores it locally, and serves it through a low-latency dashboard with automated anomaly detection.

---

## Overview

Astraeusio monitors the near-Earth space environment continuously. A Rust backend polls nine external data sources at fixed intervals, persists every record to an embedded DuckDB database, and serves all dashboard data from that local store. A Python LSTM model runs as a sidecar service and provides 3-hour-ahead Kp forecasts with Monte Carlo uncertainty estimates. A React frontend polls the backend and renders live charts, metric cards, and an anomaly feed.

Key capabilities:

- Live Kp index, solar wind speed and density, X-ray flux, and space weather alerts from NOAA SWPC
- Near-Earth asteroid feed (7-day lookahead) and ISS position with altitude and velocity
- NASA APOD, EPIC Earth imagery, and Exoplanet Archive catalog
- 3-hour Kp forecast with 95% confidence interval derived from Monte Carlo Dropout
- Anomaly detection across five event types with warning/critical severity tiers
- JWT-authenticated access; bcrypt password storage
- English and Turkish localizations

---

## Architecture

```
Browser
  └── React (Vite / Tailwind)
        └── /api/* proxy → Rust backend (Axum :3000)
                             ├── DuckDB (embedded, astraeus.duckdb)
                             ├── 10 Tokio background tasks (polling + anomaly scan)
                             └── POST /predict → Python ML service (FastAPI :8000)
```

### Backend

Written in Rust (Edition 2021) using Axum as the HTTP framework and Tokio as the async runtime. All external API calls use `reqwest` with a 60-second timeout and per-source retry logic.

Route handlers never call external APIs directly. Every data type is maintained by a dedicated background Tokio task that fetches, validates, and writes to DuckDB. Handlers read from the database and serve responses in under 5 ms. An in-process TTL cache (30–3600 seconds depending on the endpoint) prevents redundant DB reads under concurrent browser connections.

All floating-point values are stored as scaled 64-bit integers (`flux_e12`, `speed_e1`, `lat_e6`, etc.) and de-scaled on read. All timestamps are UTC Unix seconds (`i64`). No external database server is required; DuckDB runs embedded in the same process.

Authentication uses RS256-compatible JWT tokens issued on login and verified on protected routes. Passwords are hashed with bcrypt (cost factor 12).

**Background poller intervals:**

| Source | Interval |
|---|---|
| ISS position | 5 s |
| Kp index | 60 s |
| Solar wind | 60 s |
| X-ray flux | 120 s |
| Space weather alerts | 300 s |
| NEO feed | 30 min |
| EPIC imagery | 30 min |
| APOD | 1 h |
| Exoplanet catalog | 24 h |
| Anomaly scan | 60 s (initial delay 90 s) |

**Core dependencies:** `axum 0.8`, `tokio 1`, `duckdb 1` (bundled), `reqwest 0.13`, `serde_json 1`, `jsonwebtoken 9`, `bcrypt 0.15`, `tracing 0.1`, `anyhow 1`, `thiserror 2`, `dotenvy 0.15`

### ML Service

A FastAPI application (Python) that exposes a single `POST /predict` endpoint. The backend calls it from the `/api/kp-forecast` route and persists the returned prediction to the `kp_forecast` table for anomaly detection use.

**Dependencies:** `torch`, `fastapi`, `uvicorn`, `numpy`, `pandas`, `pyarrow`, `requests`

### Frontend

React 19 with Vite and Tailwind CSS. All data is fetched by a `useApi` hook that polls at either 30-second (live data) or 120-second (slower-changing data) intervals. The Vite dev server proxies `/api/*` and `/auth/*` to `localhost:3000`. Localizations are managed with `i18next`; English and Turkish are included.

---

## Data Sources

| Source | Base URL | Data |
|---|---|---|
| NOAA SWPC | `services.swpc.noaa.gov` | Kp index (1-min), solar wind (RTSW 1-min), GOES X-ray (1-day), space weather alerts |
| NASA | `api.nasa.gov` | APOD, NeoWs 7-day feed, EPIC imagery, Exoplanet Archive |
| Open Notify | `api.open-notify.org` | ISS latitude, longitude, altitude, velocity |

NOAA endpoints are public and require no API key. NASA requests use `NASA_API_KEY`; a DEMO_KEY is available for low-volume use.

---

## ML Model

### Architecture

Two-layer LSTM with a two-layer linear head.

```
Input: (batch, 16, 7)  — 16 time steps × 7 features
LSTM: input_size=7, hidden_size=64, num_layers=2, dropout=0.2
Head: Linear(64, 32) → ReLU → Linear(32, 1)
Output: scalar Kp prediction for the next 3-hour period
```

### Input Features

Each time step in the 48-hour input window (16 steps × 3 hours) carries seven features:

| Feature | Description |
|---|---|
| `kp` | Kp index normalized to [0, 1] by dividing by 9.0 |
| `hour_sin`, `hour_cos` | Hour of day encoded cyclically (period 24 h) |
| `month_sin`, `month_cos` | Month encoded cyclically (period 12 months) |
| `solar_cycle_phase_sin`, `solar_cycle_phase_cos` | Solar cycle phase encoded cyclically (period 4018.5 days, reference December 2019) |

### Training

The model is trained on approximately 20 years of 3-hourly Kp data from GFZ Potsdam / NOAA, preprocessed to Parquet by `ml/preprocess.py`.

- **Loss function:** HuberLoss (robust to rare storm spikes)
- **Optimizer:** Adam, initial LR 1e-3
- **LR schedule:** ReduceLROnPlateau, factor 0.5, patience 3 epochs
- **Gradient clipping:** max norm 1.0
- **Early stopping:** patience 7 epochs
- **Max epochs:** 60
- **Batch size:** 512

Walk-forward validation uses 4 folds of approximately 6 months each (1460 three-hour periods per fold) over the trailing 2 years of the dataset. For each fold the model is retrained from scratch on all data preceding the fold. The final production model is trained on the full dataset.

### Uncertainty Estimation

Uncertainty is derived from Monte Carlo Dropout. At inference time, the model is kept in `train()` mode for 50 stochastic forward passes. The 95% confidence interval is computed as mean ± 1.96 × standard deviation across passes, clipped to [0, 9].

### Inference API

```
POST /predict
Body:    { "readings": [float, ...] }   7–48 Kp values, oldest first
Returns: { "predicted_kp", "ci_lower", "ci_upper", "uncertainty",
           "horizon_hours", "n_mc_samples", "trained_through" }
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
| `ml_forecast_storm` | Most recent stored Kp forecast | Kp ≥ 5.0 | Kp ≥ 8.0 |

LD = Lunar Distance (384,400 km).

Anomalies are surfaced on the dashboard in a dedicated panel. Critical events are highlighted in red. The panel polls `GET /api/anomalies` every 30 seconds.

---

## API Reference

All routes are served on the backend at `BIND_ADDR` (default `0.0.0.0:3000`). Authentication endpoints are public; data endpoints require a valid JWT `Authorization: Bearer <token>` header.

| Method | Path | Description |
|---|---|---|
| `POST` | `/auth/register` | Create account (email + password) |
| `POST` | `/auth/login` | Authenticate; returns JWT |
| `GET` | `/api/kp` | Kp index records (up to 1440, ASC) |
| `GET` | `/api/solar-wind` | Solar wind records (up to 1440, DESC) |
| `GET` | `/api/xray` | X-ray flux records (up to 2880, ASC) |
| `GET` | `/api/alerts` | Space weather alerts (up to 50, newest first) |
| `GET` | `/api/iss` | Latest ISS position |
| `GET` | `/api/apod` | Today's APOD entry |
| `GET` | `/api/neo` | Near-Earth objects (7-day feed, grouped by date) |
| `GET` | `/api/epic` | Latest EPIC Earth imagery set |
| `GET` | `/api/exoplanets` | Exoplanet catalog (up to 100, newest discoveries first) |
| `GET` | `/api/kp-forecast` | ML 3-hour Kp forecast with confidence interval |
| `GET` | `/api/anomalies` | Detected anomalies (up to 100, newest first) |

---

## Getting Started

### Prerequisites

- Rust 1.82 or later (uses Edition 2024 / `let-chains` stabilization)
- Python 3.11 or later
- Node.js 20 or later
- A NASA API key from [api.nasa.gov](https://api.nasa.gov)

### Backend

```bash
cd backend
cp ../.env.example .env   # fill in NASA_API_KEY and JWT_SECRET
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
uvicorn ml.serve:app --port 8000
```

The inference server must be running for `/api/kp-forecast` to return data. All other endpoints function independently.

### Frontend

```bash
cd frontend
npm install
npm run dev       # dev server on :5173, proxies /api/* to :3000
npm run build     # production build to dist/
```

---

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `NASA_API_KEY` | Yes | — | NASA Open APIs key. DEMO_KEY works for low-volume local use. |
| `JWT_SECRET` | Yes | — | Secret used to sign and verify JWT tokens. Minimum 32 characters recommended. |
| `ML_SERVICE_URL` | No | `http://localhost:8000` | Base URL of the FastAPI ML inference service. |
| `BIND_ADDR` | No | `0.0.0.0:3000` | Host and port the backend HTTP server binds to. |
| `RUST_LOG` | No | — | Tracing log filter, e.g. `backend=info,warn`. |

Place these in a `.env` file in the `backend/` directory. The backend loads it automatically via `dotenvy`.

---

## Deployment

Astraeusio is designed to run as three processes on a single host:

1. **Backend** (`cargo run --release`) — serves HTTP and manages all background polling
2. **ML service** (`uvicorn ml.serve:app --port 8000`) — inference-only, stateless after model load
3. **Frontend** — build with `npm run build`, serve `dist/` from any static file host or reverse proxy

Point your reverse proxy (nginx, Caddy) at port 3000 for API traffic and the `dist/` directory for static assets. The frontend build assumes API requests are served from the same origin under `/api/` and `/auth/`.

DuckDB stores all data in a single file (`astraeus.duckdb`). Back it up by copying the file while the backend is stopped, or use DuckDB's `EXPORT DATABASE` for a portable snapshot.

The ML model file (`ml/models/kp_lstm.pt`) should be kept alongside the deployed inference service. It is not required by the backend; if the ML service is unavailable, the `/api/kp-forecast` endpoint returns an error and all other endpoints continue to function normally.

---

## License

Astraeusio is licensed under the [Business Source License 1.1](LICENSE).

**Change Date:** 2029-04-20
**Change License:** Apache License, Version 2.0

Until the Change Date, production use requires a commercial license from Bytus. After the Change Date, the software is available under the Apache 2.0 license. Non-production use (evaluation, development, testing, research) is permitted under BSL 1.1 without restriction.
