# Get ML Kp Forecast

Retrieve multi-horizon Kp index predictions from Astraeusio's LSTM model, at 3, 6, 12 and 24 hours ahead, each with the model's spread across 50 Monte Carlo Dropout passes. That spread is not a calibrated interval: it carries no observation noise term, so outcomes fall outside it far more often than the name suggests.

## When to use

Use this skill when you need to anticipate geomagnetic activity over the next day, for example to advise on aurora viewing prospects, satellite maneuver windows, or power grid risk. Pick the horizon that matches the decision rather than reading the 3 hour value alone.

## Authentication

No authentication required for the public forecast endpoint. Authenticated endpoint returns the same data.

## Endpoint

### Public (no auth)

**GET** `https://astraeusio.com/api/public/forecast`

### Authenticated

**GET** `https://astraeusio.com/api/kp-forecast`

## Response fields

Each entry of `forecast[]` carries one horizon:

| Field | Description |
|-------|-------------|
| `horizon_hours` | 3, 6, 12 or 24 |
| `predicted_kp` | Mean predicted Kp (0-9 scale) |
| `ci_lower` | Mean minus 1.96 standard deviations across the passes |
| `ci_upper` | Mean plus 1.96 standard deviations across the passes |
| `uncertainty` | Standard deviation across 50 MC Dropout passes |
| `status` | `"ok"` or `"degraded"` (falls back to cached forecast if ML service is unreachable) |

The top-level `predicted_kp`, `ci_lower`, `ci_upper` and `uncertainty` mirror the 3 hour horizon and are kept for callers written before the other three existed.

## Model notes

- Architecture: LSTM with a four-output head, one per horizon, and Monte Carlo Dropout (50 inference passes).
- Input window: a fixed 16 readings of the three-hourly Kp series, which is 48 hours of history. Not a variable length window.
- Features, 19 in total: Kp, its seven previous values, a 24 hour rolling maximum and a 72 hour rolling mean, hour and month as sine and cosine pairs, solar cycle phase as a sine and cosine pair, and three solar drivers, the F10.7 adjusted radio flux, the daily sunspot number, and the change in F10.7 over the previous 24 hours.
- Training data: three-hourly Kp from GFZ Potsdam's Niemegk observatory, the definitive series, covering the last 20 years. It is not NOAA one-minute estimated Kp, and it does not start at a fixed year: the downloader takes the trailing 20 years from whenever it runs.
- Cannot predict sudden storm commencement from fast CMEs with no precursor. Pair with `/api/imf` and `/api/solar-wind` for the full picture.
- A wide spread means the model disagrees with itself; weight the forecast accordingly.
