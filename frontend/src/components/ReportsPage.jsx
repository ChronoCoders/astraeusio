import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { flattenNeo, fmtNum } from '../lib/utils'

const RANGES = ['24h', '7d', '30d']
const RANGE_SECS = { '24h': 86_400, '7d': 604_800, '30d': 2_592_000 }
const BOOT_SEC = Math.floor(Date.now() / 1000)

const TYPE_LABELS = {
  kp_storm:          'anomaly.typeKpStorm',
  solar_wind_speed:  'anomaly.typeSolarWind',
  xray_flare:        'anomaly.typeXrayFlare',
  asteroid_close:    'anomaly.typeAsteroid',
  ml_forecast_storm: 'anomaly.typeMlForecast',
}

function authFetch(url) {
  return fetch(url, {
    headers: { Authorization: `Bearer ${localStorage.getItem('token')}` },
  })
}

// ── Inline SVG line chart ──────────────────────────────────────────────────────

const VW = 520, VH = 110
const PL = 38, PR = 10, PT = 10, PB = 24
const IW = VW - PL - PR, IH = VH - PT - PB

function xTicks(tMin, tMax, n) {
  const span = tMax - tMin || 1
  return Array.from({ length: n }, (_, i) => {
    const ms = tMin + (i / (n - 1)) * span
    const d = new Date(ms)
    let label
    if (span <= 86_400_000) {
      label = `${String(d.getUTCHours()).padStart(2, '0')}:00`
    } else if (span <= 8 * 86_400_000) {
      label = ['Sun','Mon','Tue','Wed','Thu','Fri','Sat'][d.getUTCDay()]
    } else {
      label = `${d.getUTCMonth() + 1}/${d.getUTCDate()}`
    }
    return { ms, label }
  })
}

function LineChart({ title, pts, yFixed, stroke, thresholds = [], loading }) {
  const { t } = useTranslation()
  const W = VW + PL + PR, H = VH + PT + PB

  if (loading) return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col">
      <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">{title}</span>
      <div className="flex items-center justify-center" style={{ height: H }}><span className="text-zinc-600 text-xs">{t('common.loading')}</span></div>
    </div>
  )

  if (!pts || pts.length < 2) return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col">
      <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">{title}</span>
      <div className="flex items-center justify-center" style={{ height: H }}><span className="text-zinc-600 text-xs">{t('common.noData')}</span></div>
    </div>
  )

  const vals = pts.map(p => p.v)
  const dataLo = Math.min(...vals), dataHi = Math.max(...vals)
  const pad = (dataHi - dataLo) * 0.08 || 0.5
  const yLo = yFixed ? yFixed[0] : Math.max(0, dataLo - pad)
  const yHi = yFixed ? yFixed[1] : dataHi + pad
  const yRange = yHi - yLo || 1

  const tArr = pts.map(p => p.ms)
  const tMin = tArr[0], tMax = tArr[tArr.length - 1]
  const tRange = tMax - tMin || 1

  const toX = ms => PL + ((ms - tMin) / tRange) * IW
  const toY = v  => PT + IH - ((v - yLo) / yRange) * IH

  const polyline = pts.map(p => `${toX(p.ms).toFixed(1)},${toY(p.v).toFixed(1)}`).join(' ')

  const yMid = (yLo + yHi) / 2
  const fmtY = v => v >= 1000 ? `${Math.round(v / 100) / 10}k` : Number.isInteger(Math.round(v * 10) / 10) ? String(Math.round(v)) : v.toFixed(1)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col">
      <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest mb-1">{title}</span>
      <svg viewBox={`0 0 ${W} ${H}`} className="w-full" style={{ minHeight: `${H}px` }}>
        {[yLo, yMid, yHi].map(v => {
          const y = toY(v)
          return (
            <g key={v}>
              <line x1={PL} y1={y} x2={PL + IW} y2={y} stroke="#27272a" strokeWidth="1" />
              <text x={PL - 4} y={y + 4} textAnchor="end" fill="#52525b" fontSize="10">{fmtY(v)}</text>
            </g>
          )
        })}
        {thresholds.map(({ v, color }) => {
          const y = toY(v)
          if (y < PT - 2 || y > PT + IH + 2) return null
          return <line key={v} x1={PL} y1={y} x2={PL + IW} y2={y} stroke={color} strokeWidth="1" strokeDasharray="4 2" />
        })}
        <polyline points={polyline} fill="none" stroke={stroke} strokeWidth="1.5" strokeLinejoin="round" />
        {xTicks(tMin, tMax, 5).map(({ ms, label }) => (
          <text key={ms} x={toX(ms).toFixed(1)} y={H - 5} textAnchor="middle" fill="#52525b" fontSize="10">{label}</text>
        ))}
      </svg>
    </div>
  )
}

// ── Sub-components ────────────────────────────────────────────────────────────

function StatCard({ label, value, unit }) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-1">
      <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">{label}</span>
      <span className="text-zinc-100 text-2xl font-thin tabular-nums">
        {value ?? <span className="text-zinc-700">—</span>}
        {value != null && unit && <span className="text-zinc-500 text-sm ml-1">{unit}</span>}
      </span>
    </div>
  )
}

function fmtTs(unixSec) {
  const d = new Date(unixSec * 1000)
  const p = n => String(n).padStart(2, '0')
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ${p(d.getUTCHours())}:${p(d.getUTCMinutes())} UTC`
}

// ── Main component ────────────────────────────────────────────────────────────

export default function ReportsPage() {
  const { t } = useTranslation()
  const [range, setRange] = useState('24h')

  // Summary stats
  const [{ data: summary, error: summaryErr, loadedFor: summaryFor }, setSummary] =
    useState({ data: null, error: null, loadedFor: null })

  // Kp chart
  const [{ data: kpData, loadedFor: kpFor }, setKp] =
    useState({ data: null, loadedFor: null })

  // Solar wind chart
  const [{ data: windData, loadedFor: windFor }, setWind] =
    useState({ data: null, loadedFor: null })

  // Anomalies (fetched once, filtered client-side)
  const [anomalies, setAnomalies] = useState(null)

  // NEO (fetched once)
  const [neo, setNeo] = useState(null)

  useEffect(() => {
    let cancelled = false
    authFetch(`/api/reports/summary?range=${range}`)
      .then(r => r.json())
      .then(d => { if (!cancelled) setSummary({ data: d, error: null, loadedFor: range }) })
      .catch(e => { if (!cancelled) setSummary({ data: null, error: e.message, loadedFor: range }) })
    return () => { cancelled = true }
  }, [range])

  useEffect(() => {
    let cancelled = false
    authFetch(`/api/reports/kp?range=${range}`)
      .then(r => r.json())
      .then(d => { if (!cancelled) setKp({ data: Array.isArray(d) ? d : [], loadedFor: range }) })
      .catch(() => { if (!cancelled) setKp({ data: [], loadedFor: range }) })
    return () => { cancelled = true }
  }, [range])

  useEffect(() => {
    let cancelled = false
    authFetch(`/api/reports/solar-wind?range=${range}`)
      .then(r => r.json())
      .then(d => { if (!cancelled) setWind({ data: Array.isArray(d) ? d : [], loadedFor: range }) })
      .catch(() => { if (!cancelled) setWind({ data: [], loadedFor: range }) })
    return () => { cancelled = true }
  }, [range])

  useEffect(() => {
    let cancelled = false
    authFetch('/api/anomalies')
      .then(r => r.json())
      .then(d => { if (!cancelled) setAnomalies(Array.isArray(d) ? d : []) })
      .catch(() => { if (!cancelled) setAnomalies([]) })
    return () => { cancelled = true }
  }, [])

  useEffect(() => {
    let cancelled = false
    authFetch('/api/neo')
      .then(r => r.json())
      .then(d => { if (!cancelled) setNeo(d) })
      .catch(() => { if (!cancelled) setNeo(null) })
    return () => { cancelled = true }
  }, [])

  // Derived data
  const rangeSecs = RANGE_SECS[range]
  const summaryLoading = summaryFor !== range

  const kpPts = (kpFor === range ? kpData : null)?.map(r => ({
    ms: new Date(r.time_tag).getTime(),
    v: r.estimated_kp,
  })).filter(p => isFinite(p.ms) && isFinite(p.v)) ?? null

  const windPts = (windFor === range ? windData : null)?.map(r => ({
    ms: new Date(r.time_tag).getTime(),
    v: r.proton_speed,
  })).filter(p => isFinite(p.ms) && isFinite(p.v)) ?? null

  const cutoffSec = BOOT_SEC - rangeSecs
  const filteredAnomalies = (anomalies ?? []).filter(a => a.detected_at >= cutoffSec)

  // NEO: filter by close_approach_date within range
  const todayStr = new Date(BOOT_SEC * 1000).toISOString().slice(0, 10)
  const endStr   = new Date((BOOT_SEC + rangeSecs) * 1000).toISOString().slice(0, 10)
  const neoRows = flattenNeo(neo).filter(r => r.date >= todayStr && r.date <= endStr)

  const fmtKp  = v => (v != null ? v.toFixed(2) : null)
  const fmtSpd = v => (v != null ? Math.round(v).toString() : null)

  function handleExport() {
    const a = document.createElement('a')
    a.href = `/api/reports/export?range=${range}`
    a.setAttribute('download', `astraeus-report-${range}.csv`)
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
  }

  return (
    <div className="flex flex-col gap-4">

      {/* ── Header ──────────────────────────────────────────────────── */}
      <div className="flex items-center justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest font-mono">
          {t('reports.title')}
        </span>
        <div className="flex items-center gap-2">
          <div className="flex gap-1">
            {RANGES.map(r => (
              <button
                key={r}
                onClick={() => setRange(r)}
                className={[
                  'text-xs font-mono px-3 py-1 rounded transition-colors',
                  range === r ? 'bg-zinc-700 text-zinc-100' : 'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800',
                ].join(' ')}
              >
                {r}
              </button>
            ))}
          </div>
          <button
            onClick={handleExport}
            className="text-xs font-mono px-3 py-1 rounded border border-zinc-700 text-zinc-400 hover:text-zinc-200 hover:border-zinc-500 transition-colors"
          >
            {t('reports.exportCsv')} ↓
          </button>
        </div>
      </div>

      {/* ── Summary stats ────────────────────────────────────────────── */}
      {summaryLoading && <p className="text-zinc-600 text-xs font-mono">{t('common.loading')}</p>}
      {summaryErr && <p className="text-red-500 text-xs font-mono">{summaryErr}</p>}
      {summary && !summaryLoading && (
        <div className="grid grid-cols-3 gap-3">
          <StatCard label={t('reports.avgKp')}             value={fmtKp(summary.kp_avg)} />
          <StatCard label={t('reports.maxKp')}             value={fmtKp(summary.kp_max)} />
          <StatCard label={t('reports.maxSolarWind')}      value={fmtSpd(summary.solar_wind_max_kms)} unit="km/s" />
          <StatCard label={t('reports.maxXray')}           value={summary.xray_max_class !== '—' ? summary.xray_max_class : null} />
          <StatCard label={t('reports.anomalies')}         value={summary.anomaly_count != null ? summary.anomaly_count.toString() : null} />
          <StatCard label={t('reports.asteroidApproaches')} value={summary.asteroid_approaches != null ? summary.asteroid_approaches.toString() : null} />
        </div>
      )}

      {/* ── Charts ───────────────────────────────────────────────────── */}
      <div className="grid grid-cols-2 gap-3">
        <LineChart
          title={t('reports.kpChartTitle')}
          pts={kpPts}
          yFixed={[0, 9]}
          stroke="#60a5fa"
          thresholds={[{ v: 5, color: '#fb923c' }, { v: 7, color: '#f87171' }]}
          loading={kpFor !== range}
        />
        <LineChart
          title={t('reports.windChartTitle')}
          pts={windPts}
          stroke="#34d399"
          thresholds={[{ v: 700, color: '#fb923c' }]}
          loading={windFor !== range}
        />
      </div>

      {/* ── Anomaly events table ─────────────────────────────────────── */}
      <div className="bg-zinc-900 border border-zinc-800 rounded flex flex-col">
        <div className="px-4 py-3 border-b border-zinc-800 flex items-center justify-between">
          <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
            {t('reports.anomaliesTitle')}
          </span>
          <span className="text-zinc-700 text-xs font-mono">{filteredAnomalies.length}</span>
        </div>
        {anomalies == null ? (
          <p className="text-zinc-600 text-xs font-mono p-4">{t('common.loading')}</p>
        ) : filteredAnomalies.length === 0 ? (
          <p className="text-zinc-600 text-xs font-mono p-4">{t('reports.noAnomalies')}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-xs font-mono">
              <thead>
                <tr className="border-b border-zinc-800 text-zinc-600">
                  <th className="text-left px-4 py-2 font-normal whitespace-nowrap">{t('reports.colTime')}</th>
                  <th className="text-left px-4 py-2 font-normal">{t('reports.colType')}</th>
                  <th className="text-left px-4 py-2 font-normal">{t('reports.colSeverity')}</th>
                  <th className="text-left px-4 py-2 font-normal">{t('reports.colMessage')}</th>
                </tr>
              </thead>
              <tbody>
                {filteredAnomalies.map((a, i) => (
                  <tr key={i} className="border-b border-zinc-800/50 hover:bg-zinc-800/30">
                    <td className="px-4 py-2 text-zinc-500 whitespace-nowrap">{fmtTs(a.detected_at)}</td>
                    <td className="px-4 py-2 text-zinc-400 whitespace-nowrap">{t(TYPE_LABELS[a.type] ?? a.type)}</td>
                    <td className="px-4 py-2 whitespace-nowrap">
                      <span className={a.severity === 'critical' ? 'text-red-400' : 'text-amber-400'}>
                        {t(a.severity === 'critical' ? 'anomaly.critical' : 'anomaly.warning')}
                      </span>
                    </td>
                    <td className="px-4 py-2 text-zinc-300 max-w-xs truncate" title={a.message}>{a.message}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* ── Asteroid close approaches ─────────────────────────────────── */}
      <div className="bg-zinc-900 border border-zinc-800 rounded flex flex-col">
        <div className="px-4 py-3 border-b border-zinc-800 flex items-center justify-between">
          <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
            {t('reports.neoTitle')}
          </span>
          <span className="text-zinc-700 text-xs font-mono">{neoRows.length}</span>
        </div>
        {neo == null ? (
          <p className="text-zinc-600 text-xs font-mono p-4">{t('common.loading')}</p>
        ) : neoRows.length === 0 ? (
          <p className="text-zinc-600 text-xs font-mono p-4">{t('reports.noApproaches')}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-xs font-mono">
              <thead>
                <tr className="border-b border-zinc-800 text-zinc-600">
                  <th className="text-left px-4 py-2 font-normal">{t('neo.colName')}</th>
                  <th className="text-left px-4 py-2 font-normal">{t('neo.colDate')}</th>
                  <th className="text-right px-4 py-2 font-normal">{t('neo.colDistance')}</th>
                  <th className="text-right px-4 py-2 font-normal">{t('neo.colDiameter')}</th>
                  <th className="px-4 py-2 font-normal">{t('neo.colStatus')}</th>
                </tr>
              </thead>
              <tbody>
                {neoRows.map(r => (
                  <tr key={r.id} className="border-b border-zinc-800/50 hover:bg-zinc-800/30">
                    <td className="px-4 py-2 text-zinc-200 max-w-[200px] truncate" title={r.name}>{r.name}</td>
                    <td className="px-4 py-2 text-zinc-500 whitespace-nowrap">{r.date}</td>
                    <td className="px-4 py-2 text-right text-zinc-200 tabular-nums">{fmtNum(r.lunar, 2)}</td>
                    <td className="px-4 py-2 text-right text-zinc-400 whitespace-nowrap tabular-nums">
                      {fmtNum(r.diamMin * 1000, 0)}–{fmtNum(r.diamMax * 1000, 0)} m
                    </td>
                    <td className="px-4 py-2">
                      {r.hazardous
                        ? <span className="text-red-400 border border-red-800 rounded px-1.5 py-0.5">{t('neo.hazardous')}</span>
                        : <span className="text-zinc-500 border border-zinc-700 rounded px-1.5 py-0.5">{t('neo.safe')}</span>
                      }
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <p className="text-zinc-700 text-xs">{t('reports.note')}</p>

    </div>
  )
}
