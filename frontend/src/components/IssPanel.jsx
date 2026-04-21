import { useTranslation } from 'react-i18next'
import { orbitalProgress, fmtNum } from '../lib/utils'

function Ring({ progress }) {
  const r = 28
  const circ = 2 * Math.PI * r
  const dash = circ * Math.min(progress, 1)
  return (
    <svg width="72" height="72" className="flex-shrink-0">
      <circle cx="36" cy="36" r={r} fill="none" stroke="#27272a" strokeWidth="4" />
      <circle
        cx="36" cy="36" r={r}
        fill="none" stroke="#3b82f6" strokeWidth="4"
        strokeDasharray={`${dash} ${circ}`}
        strokeLinecap="round"
        transform="rotate(-90 36 36)"
      />
      <text x="36" y="40" textAnchor="middle" fill="#a1a1aa" fontSize="11" fontFamily="monospace">
        {Math.round(progress * 100)}%
      </text>
    </svg>
  )
}

export default function IssPanel({ data }) {
  const { t } = useTranslation()

  const Row = ({ label, value, unit }) => (
    <div className="flex flex-col">
      <span className="text-zinc-500 text-xs">{label}</span>
      <span className="font-mono text-zinc-200">
        {value ?? '—'}{unit && <span className="text-zinc-500 text-xs ml-1">{unit}</span>}
      </span>
    </div>
  )

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-4">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('iss.title')}</span>

      {!data ? (
        <p className="text-zinc-600 text-sm">{t('common.noData')}</p>
      ) : (
        <>
          <div className="flex items-center gap-4">
            <Ring progress={orbitalProgress(data.timestamp)} />
            <div className="flex flex-col gap-1">
              <span className="text-zinc-500 text-xs">{t('iss.orbitProgress')}</span>
              <span className="text-zinc-400 text-xs">{t('iss.period')}</span>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <Row label={t('iss.latitude')}  value={fmtNum(data.latitude, 4)}  unit="°N" />
            <Row label={t('iss.longitude')} value={fmtNum(data.longitude, 4)} unit="°E" />
            <Row label={t('iss.altitude')}  value={fmtNum(data.altitude, 1)}  unit="km" />
            <Row label={t('iss.velocity')}  value={fmtNum(data.velocity, 0)}  unit="km/h" />
          </div>

          <p className="text-zinc-600 text-xs border-t border-zinc-800 pt-2">
            {t('iss.updated', { time: new Date(data.timestamp * 1000).toLocaleTimeString('en-US', { hour12: false }) })}
          </p>
        </>
      )}
    </div>
  )
}
