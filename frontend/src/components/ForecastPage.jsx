import { useState, useEffect, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { fmtNum, stormInfo, stormProb, auroraLine } from '../lib/utils'
import { authedFetch } from '../lib/useApi'

const RANGES = ['24h', '7d', '30d']

export default function ForecastPage({ forecast }) {
  const { t } = useTranslation()
  const [range, setRange] = useState('7d')
  const [history, setHistory] = useState(null)
  const [metrics, setMetrics] = useState(null)
  const [loading, setLoading] = useState(false)
  const [error, setError]     = useState(null)

  const load = useCallback(async (r) => {
    setLoading(true)
    setError(null)
    try {
      const [h, m] = await Promise.all([
        authedFetch(`/api/forecast/history?range=${r}`).then(res => res.json()),
        authedFetch(`/api/forecast/metrics?range=${r}`).then(res => res.json()),
      ])
      setHistory(h)
      setMetrics(m)
    } catch (e) {
      setError(e.message || 'failed to load')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { load(range) }, [range, load])

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('forecastPage.title')}</span>
        <div className="flex items-center gap-1 bg-zinc-900 border border-zinc-800 rounded p-0.5">
          {RANGES.map(r => (
            <button
              key={r}
              onClick={() => setRange(r)}
              className={`px-3 py-1 text-xs font-mono rounded transition-colors ${
                range === r ? 'bg-zinc-700 text-zinc-100' : 'text-zinc-500 hover:text-zinc-300'
              }`}
            >
              {r}
            </button>
          ))}
        </div>
      </div>

      <CurrentForecastHero data={forecast} />

      <HistoryChart history={history} loading={loading} error={error} />

      <MetricsGrid metrics={metrics} loading={loading} />

      <div className="bg-zinc-900 border border-zinc-800 rounded p-4">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('forecastPage.modelInfo')}</span>
        <p className="text-zinc-400 text-sm mt-3 leading-relaxed">{t('forecastPage.modelDesc')}</p>
      </div>
    </div>
  )
}

/* ── Current forecast hero (multi-horizon) ────────────────────────────────── */

function CurrentForecastHero({ data }) {
  const { t } = useTranslation()
  if (!data) {
    return (
      <div className="bg-zinc-900 border border-zinc-800 rounded p-4">
        <p className="text-zinc-600 text-sm">{t('forecast.loading')}</p>
      </div>
    )
  }

  // Prefer the multi-horizon array; fall back to the flat 3h fields for
  // degraded or pre-multi-horizon cached responses.
  const horizons = Array.isArray(data.forecast) && data.forecast.length
    ? data.forecast
    : [{
        horizon_hours: data.horizon_hours ?? 3,
        predicted_kp:  data.predicted_kp,
        ci_lower:      data.ci_lower,
        ci_upper:      data.ci_upper,
        uncertainty:   data.uncertainty,
      }]

  const near   = horizons[0]
  const aurora = auroraLine(near.predicted_kp)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-5 flex flex-col gap-4">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('forecast.horizons')}</span>

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        {horizons.map(h => <HorizonCard key={h.horizon_hours} h={h} />)}
      </div>

      <div className="flex items-center gap-2 text-xs border-t border-zinc-800 pt-3">
        <span className="text-zinc-600 uppercase tracking-widest">{t('forecast.aurora')}</span>
        <span className="text-zinc-300">
          {aurora.visible ? t('aurora.visible', { lat: aurora.lat }) : t('aurora.notVisible')}
        </span>
      </div>
    </div>
  )
}

function HorizonCard({ h }) {
  const { t } = useTranslation()
  const kp    = h.predicted_kp
  const storm = stormInfo(kp)
  const prob  = stormProb(kp, h.uncertainty)

  return (
    <div className="bg-zinc-950/40 border border-zinc-800 rounded p-4 flex flex-col gap-2">
      <div className="flex items-baseline justify-between">
        <span className="text-zinc-400 text-xs font-mono">{t('forecast.leadTime', { hours: h.horizon_hours })}</span>
        <span className={`text-xs font-medium ${storm.cls}`}>{t(storm.key)}</span>
      </div>
      <span className={`font-mono text-3xl font-semibold ${storm.cls}`}>{fmtNum(kp, 2)}</span>
      <div className="flex flex-col gap-1 mt-1">
        <Row label={t('forecast.ciShort')}   value={`${fmtNum(h.ci_lower, 2)} – ${fmtNum(h.ci_upper, 2)}`} mono />
        <Row label={t('forecast.probShort')} value={`${Math.round(prob * 100)}%`} mono />
        <Row label={t('forecast.uncShort')}  value={`${fmtNum(h.uncertainty, 3)} Kp`} mono />
      </div>
    </div>
  )
}

/* ── Predicted vs Actual chart ────────────────────────────────────────────── */

const W = 880, H = 280
const PAD = { t: 16, r: 16, b: 32, l: 32 }
const CW = W - PAD.l - PAD.r
const CH = H - PAD.t - PAD.b
const MAX_KP = 9

function HistoryChart({ history, loading, error }) {
  const { t } = useTranslation()

  if (loading && !history) return <ChartShell><p className="text-zinc-600 text-sm">{t('forecast.loading')}</p></ChartShell>
  if (error)               return <ChartShell><p className="text-red-500 text-sm">{error}</p></ChartShell>
  if (!history?.length)    return <ChartShell><p className="text-zinc-600 text-sm">{t('common.noData')}</p></ChartShell>

  const pts = history
  const tMin = pts[0].ts
  const tMax = pts[pts.length - 1].ts
  const tRange = Math.max(1, tMax - tMin)

  const toX = ts => PAD.l + ((ts - tMin) / tRange) * CW
  const toY = kp => PAD.t + CH - (Math.min(Math.max(kp, 0), MAX_KP) / MAX_KP) * CH

  const predPath = pts.map((p, i) => `${i === 0 ? 'M' : 'L'} ${toX(p.ts)} ${toY(p.predicted_kp)}`).join(' ')

  const ciPts = pts.filter(p => p.ci_lower != null && p.ci_upper != null)
  const ciPath = ciPts.length > 1
    ? (
        ciPts.map((p, i) => `${i === 0 ? 'M' : 'L'} ${toX(p.ts)} ${toY(p.ci_upper)}`).join(' ')
        + ' ' +
        [...ciPts].reverse().map(p => `L ${toX(p.ts)} ${toY(p.ci_lower)}`).join(' ')
        + ' Z'
      )
    : null

  const actuals = pts.filter(p => p.actual_kp != null)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('forecastPage.predVsActual')}</span>
        <div className="flex items-center gap-4 text-xs font-mono">
          <Legend color="#fb923c" label={t('forecastPage.legendPredicted')} />
          <Legend color="#fb923c" label={t('forecastPage.legendCi')} faded />
          <Legend color="#22d3ee" label={t('forecastPage.legendActual')} dot />
        </div>
      </div>

      <svg viewBox={`0 0 ${W} ${H}`} className="w-full" style={{ minHeight: H }}>
        {[1, 3, 5, 7, 9].map(kp => {
          const y = toY(kp)
          return (
            <g key={kp}>
              <line x1={PAD.l} y1={y} x2={PAD.l + CW} y2={y} stroke="#27272a" strokeWidth="1" />
              <text x={PAD.l - 6} y={y + 4} textAnchor="end" fill="#52525b" fontSize="11">{kp}</text>
            </g>
          )
        })}
        <line x1={PAD.l} y1={toY(5)} x2={PAD.l + CW} y2={toY(5)} stroke="#a16207" strokeWidth="1" strokeDasharray="4 3" />

        {ciPath && (
          <path d={ciPath} fill="#fb923c" fillOpacity="0.12" stroke="none" />
        )}

        <path d={predPath} fill="none" stroke="#fb923c" strokeWidth="1.5" strokeLinejoin="round" />

        {actuals.map((p, i) => (
          <circle key={i} cx={toX(p.ts)} cy={toY(p.actual_kp)} r="2.5" fill="#22d3ee" />
        ))}

        <TimeAxis tMin={tMin} tMax={tMax} toX={toX} />
      </svg>
    </div>
  )
}

function TimeAxis({ tMin, tMax, toX }) {
  const span = tMax - tMin
  const ticks = 5
  const out = []
  for (let i = 0; i <= ticks; i++) {
    const ts = tMin + (span * i) / ticks
    const d = new Date(ts * 1000)
    const label = span < 2 * 86400
      ? `${String(d.getUTCHours()).padStart(2, '0')}:${String(d.getUTCMinutes()).padStart(2, '0')}`
      : `${d.getUTCMonth() + 1}/${d.getUTCDate()}`
    out.push(
      <text key={i} x={toX(ts)} y={H - 10} textAnchor="middle" fill="#52525b" fontSize="11">{label}</text>
    )
  }
  return <>{out}</>
}

function Legend({ color, label, faded, dot }) {
  return (
    <span className="flex items-center gap-1.5 text-zinc-500">
      {dot
        ? <span className="w-2 h-2 rounded-full" style={{ background: color }} />
        : <span className="w-3 h-px" style={{ background: color, opacity: faded ? 0.3 : 1 }} />
      }
      {label}
    </span>
  )
}

function ChartShell({ children }) {
  const { t } = useTranslation()
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-2 min-h-[200px]">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('forecastPage.predVsActual')}</span>
      <div className="flex-1 flex items-center justify-center">{children}</div>
    </div>
  )
}

/* ── Metrics grid ─────────────────────────────────────────────────────────── */

function MetricsGrid({ metrics, loading }) {
  const { t } = useTranslation()
  if (loading && !metrics) return null
  const n = metrics?.n_samples ?? 0

  return (
    <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
      <MetricCard label={t('forecastPage.accuracy')}>
        <Row label={t('forecastPage.rmse')} value={metrics?.rmse != null ? fmtNum(metrics.rmse, 2) : '—'} />
        <Row label={t('forecastPage.mae')}  value={metrics?.mae  != null ? fmtNum(metrics.mae,  2) : '—'} />
        <Row label={t('forecastPage.samples')} value={n} mono />
      </MetricCard>

      <MetricCard label={t('forecastPage.stormCatch')}>
        <Row label={t('forecastPage.hitRate')}
             value={metrics?.hit_rate != null ? `${Math.round(metrics.hit_rate * 100)}%` : '—'} />
        <Row label={t('forecastPage.falsePos')} value={metrics?.n_false_pos ?? 0} mono />
        <Row label={t('forecastPage.nStorms')}  value={metrics?.n_storms ?? 0}    mono />
      </MetricCard>

      <MetricCard label={t('forecastPage.uncertaintyCard')}>
        <Row label={t('forecastPage.meanSigma')}
             value={metrics?.mean_unc != null ? `${fmtNum(metrics.mean_unc, 2)} Kp` : '—'} />
        <Row label={t('forecastPage.window')} value="3 h" mono />
        <Row label={t('forecastPage.passes')} value="50"  mono />
      </MetricCard>
    </div>
  )
}

function MetricCard({ label, children }) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{label}</span>
      <div className="flex flex-col gap-2">{children}</div>
    </div>
  )
}

function Row({ label, value, mono }) {
  return (
    <div className="flex items-baseline justify-between">
      <span className="text-zinc-500 text-xs">{label}</span>
      <span className={`text-zinc-200 text-sm ${mono ? 'font-mono' : ''}`}>{value}</span>
    </div>
  )
}
