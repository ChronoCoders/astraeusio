import { useTranslation } from 'react-i18next'
import UpgradePrompt from './UpgradePrompt'

const TYPE_LABELS = {
  kp_storm:          'anomaly.typeKpStorm',
  solar_wind_speed:  'anomaly.typeSolarWind',
  xray_flare:        'anomaly.typeXrayFlare',
  asteroid_close:    'anomaly.typeAsteroid',
  ml_forecast_storm: 'anomaly.typeMlForecast',
}

function fmtTs(unixSec) {
  try {
    const d = new Date(unixSec * 1000)
    const p = n => String(n).padStart(2, '0')
    return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ` +
           `${p(d.getUTCHours())}:${p(d.getUTCMinutes())} UTC`
  } catch {
    return '—'
  }
}

function Badge({ severity }) {
  const { t } = useTranslation()
  const isCritical = severity === 'critical'
  return (
    <span className={`inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-mono font-semibold uppercase tracking-wider flex-shrink-0 ${
      isCritical
        ? 'bg-red-900/60 text-red-300 border border-red-700/50'
        : 'bg-amber-900/50 text-amber-300 border border-amber-700/40'
    }`}>
      {t(isCritical ? 'anomaly.critical' : 'anomaly.warning')}
    </span>
  )
}

export default function AnomalyPanel({ data, loading, error }) {
  const { t } = useTranslation()
  const items = (data ?? []).slice(0, 30)

  if (error === 'HTTP 403') {
    return (
      <div className="bg-zinc-900 border border-zinc-800 rounded p-4">
        <UpgradePrompt messageKey="plan.lockedAnomalies" requiredPlan="developer" />
      </div>
    )
  }

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('anomaly.title')}</span>
        {items.length > 0 && (
          <span className={`text-xs font-mono ${
            items.some(a => a.severity === 'critical') ? 'text-red-400' : 'text-amber-400'
          }`}>
            {items.length}
          </span>
        )}
      </div>

      {loading && <p className="text-zinc-600 text-sm">{t('common.loading')}</p>}
      {error   && <p className="text-red-500 text-sm">{t('common.unavailable')}</p>}

      {!loading && !error && items.length === 0 && (
        <p className="text-zinc-600 text-sm">{t('anomaly.none')}</p>
      )}

      {!loading && !error && items.length > 0 && (
        <ul className="flex flex-col gap-1.5 overflow-y-auto max-h-64">
          {items.map((a, i) => (
            <li
              key={i}
              className={`flex gap-2.5 items-start rounded px-2 py-1.5 border ${
                a.severity === 'critical'
                  ? 'bg-red-950/30 border-red-900/40'
                  : 'bg-amber-950/20 border-amber-900/30'
              }`}
            >
              <div className="flex flex-col gap-0.5 min-w-0 flex-1">
                <div className="flex items-center gap-2 flex-wrap">
                  <Badge severity={a.severity} />
                  <span className="text-zinc-400 text-xs font-mono">
                    {t(TYPE_LABELS[a.type] ?? a.type)}
                  </span>
                </div>
                <p className="text-zinc-200 text-xs leading-snug mt-0.5">{a.message}</p>
                <p className="text-zinc-600 text-[10px] font-mono mt-0.5">{fmtTs(a.detected_at)}</p>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
