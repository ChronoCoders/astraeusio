# Astraeusio — Real-Time Space Weather & Astronomy Dashboard

Astraeusio monitors solar activity, geomagnetic conditions, and near-Earth objects in real time, with an ML-powered 3-hour Kp forecast powered by an LSTM model trained on 20+ years of NOAA data.

## What It Does

- **Live Kp index** — 1-minute estimated Kp from NOAA's ground magnetometer network
- **Solar wind** — speed and density from NOAA DSCOVR satellite
- **Geomagnetic storm prediction** — LSTM + Monte Carlo Dropout, 95% confidence intervals
- **X-ray flux** — GOES satellite primary channel (M/X flare detection)
- **IMF Bz** — interplanetary magnetic field southward component (key storm driver)
- **Dst index** — ring current energy (storm severity indicator)
- **Auroral oval forecast** — NOAA SWPC northern and southern hemisphere images, updated every 30 min
- **ISS live position** — latitude, longitude, altitude, velocity
- **NASA APOD** — Astronomy Picture of the Day
- **NASA EPIC** — Earth Polychromatic Imaging Camera imagery
- **Near-Earth Objects** — NASA NeoWs 7-day close approach data with hazard flags
- **Exoplanet catalog** — NASA Exoplanet Archive
- **Starlink constellation** — live TLE positions from Celestrak
- **Anomaly detection** — automated alerts for Kp storms, solar wind spikes, X-ray flares, asteroid close approaches, and ML-forecast storms

## API

The Astraeusio API provides authenticated access to all data streams. See [API documentation](/docs) for endpoint reference, authentication, and rate limits.

Base URL: `https://astraeusio.com/api/`  
Authentication: Bearer token (JWT)  
API catalog: `/.well-known/api-catalog`

### Public endpoints (no auth)
- `GET /api/public/kp` — latest Kp reading
- `GET /api/public/solar-wind` — latest solar wind reading
- `GET /api/public/forecast` — latest ML Kp forecast

### Data endpoints (auth required)
- `GET /api/kp` — 1-minute Kp time series
- `GET /api/solar-wind` — solar wind time series
- `GET /api/xray` — X-ray flux time series
- `GET /api/imf` — IMF magnetometer data
- `GET /api/dst` — Dst index
- `GET /api/alerts` — space weather alerts
- `GET /api/kp-forecast` — ML storm forecast with confidence intervals
- `GET /api/neo` — near-Earth object close approaches
- `GET /api/iss` — ISS live position
- `GET /api/apod` — NASA APOD
- `GET /api/epic` — NASA EPIC images
- `GET /api/anomalies` — detected anomalies
- `GET /api/reports/summary?range=24h|7d|30d` — summary statistics
- `GET /api/health` — service health status

## Plans

- **Free** — public data, limited history
- **Developer** — full API access, CSV export, anomaly feed
- **Professional** — extended history, higher rate limits
- **Enterprise** — custom SLAs, dedicated support

See [pricing](/pricing) for details.

## Links

- Homepage: https://astraeusio.com/
- API docs: https://astraeusio.com/docs
- Pricing: https://astraeusio.com/pricing
- Status: https://astraeusio.com/status
- Sitemap: https://astraeusio.com/sitemap.xml
- API catalog: https://astraeusio.com/.well-known/api-catalog
