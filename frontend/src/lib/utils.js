// ── Kp / Storm ────────────────────────────────────────────────────────────────

export function kpDesc(kp) {
  if (kp >= 9) return { key: 'metrics.desc.kp.g5',        cls: 'text-red-400' }
  if (kp >= 8) return { key: 'metrics.desc.kp.g4',        cls: 'text-red-400' }
  if (kp >= 7) return { key: 'metrics.desc.kp.g3',        cls: 'text-orange-400' }
  if (kp >= 6) return { key: 'metrics.desc.kp.g2',        cls: 'text-orange-400' }
  if (kp >= 5) return { key: 'metrics.desc.kp.g1',        cls: 'text-yellow-400' }
  if (kp >= 4) return { key: 'metrics.desc.kp.unsettled', cls: 'text-yellow-500' }
  if (kp >= 2) return { key: 'metrics.desc.kp.low',       cls: 'text-zinc-400' }
  return             { key: 'metrics.desc.kp.quiet',      cls: 'text-zinc-500' }
}

export function windDesc(speed) {
  if (!speed || speed <= 0) return { key: 'metrics.desc.wind.normal',   cls: 'text-zinc-500' }
  if (speed > 700)          return { key: 'metrics.desc.wind.storm',    cls: 'text-red-400' }
  if (speed > 500)          return { key: 'metrics.desc.wind.high',     cls: 'text-orange-400' }
  if (speed > 400)          return { key: 'metrics.desc.wind.elevated', cls: 'text-yellow-400' }
  return                           { key: 'metrics.desc.wind.normal',   cls: 'text-zinc-400' }
}

export function xrayDesc(flux) {
  if (!flux || flux <= 0) return { key: 'metrics.desc.xray.none', cls: 'text-zinc-500' }
  if (flux < 1e-7)        return { key: 'metrics.desc.xray.a',    cls: 'text-zinc-400' }
  if (flux < 1e-6)        return { key: 'metrics.desc.xray.b',    cls: 'text-zinc-300' }
  if (flux < 1e-5)        return { key: 'metrics.desc.xray.c',    cls: 'text-yellow-400' }
  if (flux < 1e-4)        return { key: 'metrics.desc.xray.m',    cls: 'text-orange-400' }
  return                         { key: 'metrics.desc.xray.x',    cls: 'text-red-400' }
}

export function stormDesc(kp) {
  if (kp >= 9) return { key: 'metrics.desc.storm.g5',        cls: 'text-red-400' }
  if (kp >= 8) return { key: 'metrics.desc.storm.g4',        cls: 'text-red-400' }
  if (kp >= 7) return { key: 'metrics.desc.storm.g3',        cls: 'text-orange-400' }
  if (kp >= 6) return { key: 'metrics.desc.storm.g2',        cls: 'text-orange-400' }
  if (kp >= 5) return { key: 'metrics.desc.storm.g1',        cls: 'text-yellow-400' }
  if (kp >= 4) return { key: 'metrics.desc.storm.unsettled', cls: 'text-yellow-500' }
  return             { key: 'metrics.desc.storm.quiet',      cls: 'text-zinc-500' }
}

export function stormInfo(kp) {
  if (kp >= 9) return { key: 'storm.g5',        cls: 'text-red-400' }
  if (kp >= 8) return { key: 'storm.g4',        cls: 'text-red-400' }
  if (kp >= 7) return { key: 'storm.g3',        cls: 'text-orange-400' }
  if (kp >= 6) return { key: 'storm.g2',        cls: 'text-orange-400' }
  if (kp >= 5) return { key: 'storm.g1',        cls: 'text-yellow-400' }
  if (kp >= 4) return { key: 'storm.unsettled', cls: 'text-yellow-500' }
  return             { key: 'storm.quiet',      cls: 'text-zinc-400' }
}

export function kpBarColor(kp) {
  if (kp >= 7) return '#f87171'   // red-400
  if (kp >= 5) return '#fb923c'   // orange-400
  if (kp >= 4) return '#facc15'   // yellow-400
  return '#4ade80'                // green-400
}

export function auroraLine(kp) {
  const lat = [90, 87, 83, 79, 72, 67, 62, 58, 52, 47]
  const idx = Math.min(Math.max(Math.floor(kp), 0), 9)
  return idx < 2 ? { visible: false } : { visible: true, lat: lat[idx] }
}

// ── X-ray ─────────────────────────────────────────────────────────────────────

export function xrayClass(flux) {
  if (!flux || flux <= 0) return { label: '-', cls: 'text-zinc-500' }
  if (flux < 1e-7) return { label: 'A' + fmt(flux / 1e-8, 1), cls: 'text-zinc-400' }
  if (flux < 1e-6) return { label: 'B' + fmt(flux / 1e-7, 1), cls: 'text-zinc-300' }
  if (flux < 1e-5) return { label: 'C' + fmt(flux / 1e-6, 1), cls: 'text-yellow-400' }
  if (flux < 1e-4) return { label: 'M' + fmt(flux / 1e-5, 1), cls: 'text-orange-400' }
  return                  { label: 'X' + fmt(flux / 1e-4, 1), cls: 'text-red-400' }
}

function fmt(n, d) { return isFinite(n) ? n.toFixed(d) : '' }

// ── Storm probability from MC forecast ───────────────────────────────────────

function erf(x) {
  const t = 1 / (1 + 0.3275911 * Math.abs(x))
  const p = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))))
  const r = 1 - p * Math.exp(-x * x)
  return x >= 0 ? r : -r
}

export function stormProb(predictedKp, uncertainty) {
  if (!uncertainty || uncertainty < 0.01) return predictedKp >= 5 ? 1 : 0
  const z = (5 - predictedKp) / uncertainty
  return Math.max(0, Math.min(1, 0.5 * (1 - erf(z / Math.SQRT2))))
}

// ── Kp chart data ─────────────────────────────────────────────────────────────

export function processKpBuckets(records) {
  if (!records?.length) return []
  const sorted = [...records].sort((a, b) => new Date(a.time_tag) - new Date(b.time_tag))
  const latestMs = new Date(sorted[sorted.length - 1].time_tag).getTime()
  const cutoff = latestMs - 24 * 3600 * 1000
  const buckets = {}
  records
    .filter(r => new Date(r.time_tag).getTime() >= cutoff)
    .forEach(r => {
      const d = new Date(r.time_tag)
      const h = Math.floor(d.getUTCHours() / 3) * 3
      const key = `${d.toISOString().slice(0, 10)}-${h}`
      buckets[key] = { h, date: d.toISOString().slice(0, 10), kp: r.estimated_kp ?? r.kp_index }
    })
  return Object.values(buckets)
    .sort((a, b) => a.date === b.date ? a.h - b.h : a.date.localeCompare(b.date))
    .slice(-8)
}

// ── NEO helpers ───────────────────────────────────────────────────────────────

const KM_PER_LD = 384_400

export function flattenNeo(feed) {
  if (!feed?.near_earth_objects) return []
  return Object.entries(feed.near_earth_objects)
    .flatMap(([date, neos]) =>
      neos.map(neo => {
        const ca = neo.close_approach_data?.[0] ?? {}
        const km = parseFloat(ca.miss_distance?.kilometers ?? 0)
        return {
          id: neo.id,
          name: neo.name,
          date,
          hazardous: neo.is_potentially_hazardous_asteroid,
          diamMin: neo.estimated_diameter?.kilometers?.estimated_diameter_min ?? 0,
          diamMax: neo.estimated_diameter?.kilometers?.estimated_diameter_max ?? 0,
          lunar: km / KM_PER_LD,
          km,
        }
      })
    )
    .sort((a, b) => a.lunar - b.lunar)
}

// ── ISS ───────────────────────────────────────────────────────────────────────

export function orbitalProgress(timestamp) {
  const PERIOD_MS = 92.68 * 60 * 1000
  return (timestamp * 1000 % PERIOD_MS) / PERIOD_MS
}

// Returns true when a satellite at (lat, lon, altKm) is in direct sunlight at
// the given UTC date. Subsolar point computed from solar position; sunlit when
// the great-circle angle from sub-satellite point to subsolar point is less
// than 90° + horizon angle for the given altitude.
export function isSunlit(lat, lon, altKm, date = new Date()) {
  const D2R = Math.PI / 180
  const d = date.getTime() / 86400000 - 10957.5  // days since J2000
  const M = (357.5291 + 0.98560028 * d) * D2R
  const L = (280.4665 + 0.98564736 * d) * D2R
  const lambda = L + (1.9148 * Math.sin(M) + 0.0200 * Math.sin(2 * M)) * D2R
  const eps = 23.4393 * D2R
  const subsolarLat = Math.asin(Math.sin(eps) * Math.sin(lambda)) / D2R
  const eqTimeMin = 4 * ((L - lambda) / D2R) - 2.466 * Math.sin(2 * L) + 0.053 * Math.sin(4 * L)
  const utcMin = date.getUTCHours() * 60 + date.getUTCMinutes() + date.getUTCSeconds() / 60
  const subsolarLon = -((utcMin + eqTimeMin - 720) / 4)
  const phi1 = lat * D2R, phi2 = subsolarLat * D2R
  const dlon = (lon - subsolarLon) * D2R
  const cosAngle = Math.sin(phi1) * Math.sin(phi2) + Math.cos(phi1) * Math.cos(phi2) * Math.cos(dlon)
  const angleDeg = Math.acos(Math.max(-1, Math.min(1, cosAngle))) / D2R
  const R = 6371
  const horizonDeg = Math.acos(R / (R + altKm)) / D2R
  return angleDeg < 90 + horizonDeg
}

// Reverse-geocode (lat, lon) to a human-readable location name. Land uses
// BigDataCloud's free client endpoint (no key, CORS-enabled). Ocean uses a
// coarse lat/lon bucket fallback.
export async function reverseGeocode(lat, lon) {
  try {
    const r = await fetch(
      `https://api.bigdatacloud.net/data/reverse-geocode-client?latitude=${lat}&longitude=${lon}&localityLanguage=en`,
    )
    if (r.ok) {
      const d = await r.json()
      if (d.countryName) {
        return d.principalSubdivision
          ? `${d.principalSubdivision}, ${d.countryName}`
          : d.countryName
      }
    }
  } catch { /* fall through to ocean */ }
  return oceanName(lat, lon)
}

function oceanName(lat, lon) {
  if (lat < -60) return 'Southern Ocean'
  if (lat > 66) return 'Arctic Ocean'
  if (lon >= 20 && lon < 145 && lat <= 30) return 'Indian Ocean'
  if (lon >= 145 || lon < -70) return 'Pacific Ocean'
  return 'Atlantic Ocean'
}

// ── EPIC image URL ────────────────────────────────────────────────────────────

export function epicImageUrl(item) {
  const id = item.identifier           // "20260416162050"
  return `https://epic.gsfc.nasa.gov/archive/natural/${id.slice(0,4)}/${id.slice(4,6)}/${id.slice(6,8)}/jpg/${item.image}.jpg`
}

// ── Plan hierarchy ────────────────────────────────────────────────────────────

const PLAN_RANK = { free: 0, starter: 0, developer: 1, pro: 2, business: 3, enterprise: 4 }

export function planSatisfies(userPlan, required) {
  return (PLAN_RANK[userPlan] ?? 0) >= (PLAN_RANK[required] ?? 0)
}

// ── Formatting ────────────────────────────────────────────────────────────────

export function fmtNum(n, decimals = 0) {
  if (n == null || !isFinite(n)) return '-'
  return n.toLocaleString('en-US', { maximumFractionDigits: decimals, minimumFractionDigits: decimals })
}

// Format a flux value (W/m²) as "1.55 × 10⁻⁵" instead of "1.55e-5".
const SUP_DIGITS = { '0':'⁰','1':'¹','2':'²','3':'³','4':'⁴','5':'⁵','6':'⁶','7':'⁷','8':'⁸','9':'⁹' }
export function fmtFlux(w) {
  if (w == null || !isFinite(w) || w <= 0) return '0'
  const exp = Math.floor(Math.log10(w))
  const mantissa = w / Math.pow(10, exp)
  const sign = exp < 0 ? '⁻' : ''
  const sup = sign + String(Math.abs(exp)).split('').map(d => SUP_DIGITS[d]).join('')
  return `${mantissa.toFixed(2)} × 10${sup}`
}
