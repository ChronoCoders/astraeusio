# Get Space Weather Anomaly Detections

Retrieve automatically detected space weather anomalies from Astraeusio's real-time monitoring system, covering geomagnetic storms, solar flares, solar wind spikes, asteroid close approaches, and ML forecast alerts.

## When to use

Use this skill when you need a summary of significant space weather events, active alerts, or to check whether any thresholds have been crossed recently. Anomalies are detected every 60 seconds.

## Authentication

Requires a Bearer token - obtain one by POST to `https://astraeusio.com/auth/login` with `{"email":"...","password":"..."}`.

## Endpoint

**GET** `https://astraeusio.com/api/anomalies`

**Authorization:** `Bearer <token>`

## Anomaly types

| type | Trigger |
|------|---------|
| `kp_storm` | Kp ≥ 5.0 (warning) / ≥ 8.0 (critical) |
| `solar_wind_speed` | Speed > 700 km/s (warning) / > 900 km/s (critical) |
| `xray_flare` | M-class ≥ 1×10⁻⁵ W/m² (warning) / X-class ≥ 1×10⁻⁴ W/m² (critical) |
| `asteroid_close` | NEO passing within 1 LD (warning) / 0.5 LD (critical) |
| `ml_forecast_storm` | ML model predicts Kp ≥ 5.0 within 3 hours |

## Response fields

Each anomaly object includes:
- `type` - anomaly type (see table above)
- `severity` - `"warning"` or `"critical"`
- `message` - human-readable description
- `detected_at` - Unix timestamp (seconds)
- `source_ref` - reference to the triggering data point
