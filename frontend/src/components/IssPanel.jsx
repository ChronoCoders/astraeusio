import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { orbitalProgress, fmtNum, isSunlit, reverseGeocode } from '../lib/utils'
import { useApi } from '../lib/useApi'

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

function Row({ label, value, unit }) {
  return (
    <div className="flex flex-col">
      <span className="text-zinc-500 text-xs">{label}</span>
      <span className="font-mono text-zinc-200">
        {value ?? '-'}{unit && <span className="text-zinc-500 text-xs ml-1">{unit}</span>}
      </span>
    </div>
  )
}

export default function IssPanel({ data }) {
  const { t } = useTranslation()
  const astros = useApi('/api/astros', 6 * 60 * 60 * 1000)
  const [location, setLocation] = useState(null)

  // Refresh location only when ISS has moved ~2° (~220 km), to avoid hammering
  // the reverse-geocode endpoint while the dashboard polls every 5 s.
  const latBucket = data ? Math.round(data.latitude / 2) : null
  const lonBucket = data ? Math.round(data.longitude / 2) : null
  useEffect(() => {
    if (!data) return
    let cancelled = false
    reverseGeocode(data.latitude, data.longitude).then(name => {
      if (!cancelled) setLocation({ name })
    })
    return () => { cancelled = true }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [latBucket, lonBucket])

  const sunlit = data ? isSunlit(data.latitude, data.longitude, data.altitude) : null
  const peopleCount = astros.data?.count ?? null
  const peoplePreview = astros.data?.people?.slice(0, 3).map(p => p.name).join(', ')

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
            <div className="ml-auto flex items-center gap-1.5">
              <span className={`inline-block w-2 h-2 rounded-full ${sunlit ? 'bg-amber-300' : 'bg-zinc-600'}`} />
              <span className="text-zinc-400 text-xs">
                {sunlit ? t('iss.sunlit') : t('iss.eclipsed')}
              </span>
            </div>
          </div>

          <div className="flex flex-col gap-0.5">
            <span className="text-zinc-500 text-xs">{t('iss.location')}</span>
            <span className="text-zinc-200 text-sm">{location?.name ?? '-'}</span>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <Row label={t('iss.latitude')}  value={fmtNum(data.latitude, 4)}  unit="°N" />
            <Row label={t('iss.longitude')} value={fmtNum(data.longitude, 4)} unit="°E" />
            <Row label={t('iss.altitude')}  value={fmtNum(data.altitude, 1)}  unit="km" />
            <Row label={t('iss.velocity')}  value={fmtNum(data.velocity, 0)}  unit="km/h" />
          </div>

          {peopleCount !== null && (
            <div className="flex flex-col gap-0.5 border-t border-zinc-800 pt-3">
              <span className="text-zinc-500 text-xs">
                {t('iss.peopleInSpace', { count: peopleCount })}
              </span>
              {peoplePreview && (
                <span className="text-zinc-400 text-xs" title={astros.data.people.map(p => p.name).join(', ')}>
                  {peoplePreview}{astros.data.people.length > 3 ? ` +${astros.data.people.length - 3}` : ''}
                </span>
              )}
            </div>
          )}

          <p className="text-zinc-600 text-xs border-t border-zinc-800 pt-2">
            {t('iss.updated', { time: new Date(data.timestamp * 1000).toLocaleTimeString('en-US', { hour12: false }) })}
          </p>
        </>
      )}
    </div>
  )
}
