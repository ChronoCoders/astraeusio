# Get ML Kp Forecast

Retrieve a 3-hour ahead Kp index prediction from Astraeusio's LSTM model, including a 95% confidence interval.

## When to use

Use this skill when you need to predict geomagnetic activity over the next 3 hours - for example, to advise on aurora viewing prospects, satellite maneuver windows, or power grid risk. The model is trained on 20+ years of NOAA Kp data.

## Authentication

No authentication required for the public forecast endpoint. Authenticated endpoint returns the same data.

## Endpoint

### Public (no auth)

**GET** `https://astraeusio.com/api/public/forecast`

### Authenticated

**GET** `https://astraeusio.com/api/kp-forecast`

## Response fields

| Field | Description |
|-------|-------------|
| `predicted_kp` | Mean predicted Kp (0–9 scale) |
| `ci_lower` | 2.5th percentile (95% CI lower bound) |
| `ci_upper` | 97.5th percentile (95% CI upper bound) |
| `uncertainty` | Standard deviation across 50 MC Dropout passes |
| `status` | `"ok"` or `"degraded"` (falls back to cached forecast if ML service is unreachable) |

## Model notes

- Architecture: LSTM with Monte Carlo Dropout (50 inference passes)
- Features: Kp history (7–48 readings), hour sin/cos, month sin/cos, solar cycle phase
- Trained on NOAA 1-minute estimated Kp from 2000–present
- Cannot predict sudden storm commencement from fast CMEs with no precursor - pair with `/api/imf` and `/api/solar-wind` for full picture
- A wide confidence interval means the situation is ambiguous; weight the forecast accordingly
