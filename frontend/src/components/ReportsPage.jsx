import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'

const RANGES = ['24h', '7d', '30d']

function StatCard({ label, value, unit }) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-1">
      <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">{label}</span>
      <span className="text-zinc-100 text-2xl font-thin tabular-nums">
        {value ?? <span className="text-zinc-700">—</span>}
        {value != null && unit && (
          <span className="text-zinc-500 text-sm ml-1">{unit}</span>
        )}
      </span>
    </div>
  )
}

export default function ReportsPage() {
  const { t } = useTranslation()
  const [range, setRange]   = useState('24h')
  const [data,  setData]    = useState(null)
  const [loading, setLoad]  = useState(false)
  const [error,  setError]  = useState(null)

  useEffect(() => {
    let cancelled = false
    setLoad(true)
    setError(null)
    fetch(`/api/reports/summary?range=${range}`, {
      headers: { Authorization: `Bearer ${localStorage.getItem('token')}` },
    })
      .then(r => r.json())
      .then(d => { if (!cancelled) { setData(d); setLoad(false) } })
      .catch(e => { if (!cancelled) { setError(e.message); setLoad(false) } })
    return () => { cancelled = true }
  }, [range])

  function handleExport() {
    const a = document.createElement('a')
    a.href = `/api/reports/export?range=${range}`
    a.setAttribute('download', `astraeus-report-${range}.csv`)
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
  }

  const fmtKp = v => (v != null ? v.toFixed(2) : null)
  const fmtSpd = v => (v != null ? Math.round(v).toString() : null)

  return (
    <div className="flex flex-col gap-4">
      {/* Header row */}
      <div className="flex items-center justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest font-mono">
          {t('reports.title')}
        </span>

        <div className="flex items-center gap-2">
          {/* Range selector */}
          <div className="flex gap-1">
            {RANGES.map(r => (
              <button
                key={r}
                onClick={() => setRange(r)}
                className={[
                  'text-xs font-mono px-3 py-1 rounded transition-colors',
                  range === r
                    ? 'bg-zinc-700 text-zinc-100'
                    : 'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800',
                ].join(' ')}
              >
                {r}
              </button>
            ))}
          </div>

          {/* Export button */}
          <button
            onClick={handleExport}
            className="text-xs font-mono px-3 py-1 rounded border border-zinc-700 text-zinc-400 hover:text-zinc-200 hover:border-zinc-500 transition-colors"
          >
            {t('reports.exportCsv')} ↓
          </button>
        </div>
      </div>

      {/* Stats grid */}
      {loading && (
        <p className="text-zinc-600 text-xs font-mono">{t('common.loading')}</p>
      )}
      {error && (
        <p className="text-red-500 text-xs font-mono">{error}</p>
      )}
      {data && !loading && (
        <div className="grid grid-cols-3 gap-3">
          <StatCard
            label={t('reports.avgKp')}
            value={fmtKp(data.kp_avg)}
          />
          <StatCard
            label={t('reports.maxKp')}
            value={fmtKp(data.kp_max)}
          />
          <StatCard
            label={t('reports.maxSolarWind')}
            value={fmtSpd(data.solar_wind_max_kms)}
            unit="km/s"
          />
          <StatCard
            label={t('reports.maxXray')}
            value={data.xray_max_class !== '—' ? data.xray_max_class : null}
          />
          <StatCard
            label={t('reports.anomalies')}
            value={data.anomaly_count != null ? data.anomaly_count.toString() : null}
          />
          <StatCard
            label={t('reports.asteroidApproaches')}
            value={data.asteroid_approaches != null ? data.asteroid_approaches.toString() : null}
          />
        </div>
      )}

      {/* Note */}
      <p className="text-zinc-700 text-xs">{t('reports.note')}</p>
    </div>
  )
}
