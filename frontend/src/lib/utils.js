// ── Kp / Storm ────────────────────────────────────────────────────────────────

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
  if (!flux || flux <= 0) return { label: '—', cls: 'text-zinc-500' }
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
  const cutoff = Date.now() - 24 * 3600 * 1000
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

export function flattenNeo(feed) {
  if (!feed?.near_earth_objects) return []
  return Object.entries(feed.near_earth_objects)
    .flatMap(([date, neos]) =>
      neos.map(neo => {
        const ca = neo.close_approach_data?.[0] ?? {}
        return {
          id: neo.id,
          name: neo.name,
          date,
          hazardous: neo.is_potentially_hazardous_asteroid,
          diamMin: neo.estimated_diameter?.kilometers?.estimated_diameter_min ?? 0,
          diamMax: neo.estimated_diameter?.kilometers?.estimated_diameter_max ?? 0,
          lunar: parseFloat(ca.miss_distance?.lunar ?? 0),
          km: parseFloat(ca.miss_distance?.kilometers ?? 0),
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

// ── EPIC image URL ────────────────────────────────────────────────────────────

export function epicImageUrl(item) {
  const id = item.identifier           // "20260416162050"
  return `https://epic.gsfc.nasa.gov/archive/natural/${id.slice(0,4)}/${id.slice(4,6)}/${id.slice(6,8)}/jpg/${item.image}.jpg`
}

// ── Formatting ────────────────────────────────────────────────────────────────

export function fmtNum(n, decimals = 0) {
  if (n == null || !isFinite(n)) return '—'
  return n.toLocaleString('en-US', { maximumFractionDigits: decimals, minimumFractionDigits: decimals })
}
