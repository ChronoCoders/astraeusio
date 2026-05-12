# Get Current Space Weather Conditions

Retrieve real-time space weather data from Astraeusio, including Kp index, solar wind, X-ray flux, IMF Bz, and Dst index.

## When to use

Use this skill when you need to answer questions about current geomagnetic activity, solar wind conditions, or space weather status. Also useful for determining whether conditions are favorable for aurora viewing, satellite operations, or HF radio propagation.

## Authentication

No authentication required for public endpoints. Full time-series data requires a Bearer token — obtain one by POST to `https://astraeusio.com/auth/login` with `{"email":"...","password":"..."}`.

## Endpoints

### Public (no auth)

**GET** `https://astraeusio.com/api/public/kp`
Returns the latest 1-minute estimated Kp index.

**GET** `https://astraeusio.com/api/public/solar-wind`
Returns the latest solar wind speed (km/s) and density (p/cm³).

### Authenticated

**GET** `https://astraeusio.com/api/kp` — 1-minute Kp time series
**GET** `https://astraeusio.com/api/solar-wind` — solar wind time series
**GET** `https://astraeusio.com/api/xray` — GOES X-ray flux (M/X flare detection)
**GET** `https://astraeusio.com/api/imf` — IMF Bz/Bt from NOAA DSCOVR
**GET** `https://astraeusio.com/api/dst` — Dst index (ring current energy)
**GET** `https://astraeusio.com/api/alerts` — NOAA space weather watches and warnings

## Interpreting results

- Kp ≥ 5 = G1 geomagnetic storm (minor)
- Kp ≥ 7 = G3 storm (strong) — aurora visible at 50° latitude
- Solar wind speed > 700 km/s = elevated storm risk
- IMF Bz < −10 nT = active energy coupling into magnetosphere
- Dst < −50 nT = moderate storm; < −100 nT = intense storm

## Service health

**GET** `https://astraeusio.com/api/health` — component status for all data sources
