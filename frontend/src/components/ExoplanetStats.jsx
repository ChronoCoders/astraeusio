import { useTranslation } from 'react-i18next'
import { fmtNum } from '../lib/utils'

function StatBox({ label, value, sub }) {
  return (
    <div className="bg-zinc-800/50 border border-zinc-800 rounded p-3 flex flex-col gap-0.5">
      <span className="text-zinc-500 text-xs">{label}</span>
      <span className="font-mono text-zinc-100 text-lg font-semibold leading-none">{value ?? '—'}</span>
      {sub && <span className="text-zinc-500 text-xs">{sub}</span>}
    </div>
  )
}

export default function ExoplanetStats({ data }) {
  const { t } = useTranslation()
  const planets = data ?? []

  const withRadius = planets.filter(p => p.pl_rade != null)
  const withMass   = planets.filter(p => p.pl_masse != null)

  const sample = planets.slice(0, 8)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-4">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('exo.title')}</span>

      {planets.length === 0 ? (
        <p className="text-zinc-600 text-sm">{t('common.noData')}</p>
      ) : (
        <>
          <div className="grid grid-cols-3 gap-2">
            <StatBox label={t('exo.total')} value={fmtNum(planets.length)} sub={t('exo.planets')} />
            <StatBox
              label={t('exo.medianRadius')}
              value={withRadius.length ? fmtNum(withRadius.map(p => p.pl_rade).sort((a,b) => a-b)[Math.floor(withRadius.length/2)], 1) : '—'}
              sub={t('exo.earthRadii')}
            />
            <StatBox
              label={t('exo.medianMass')}
              value={withMass.length ? fmtNum(withMass.map(p => p.pl_masse).sort((a,b) => a-b)[Math.floor(withMass.length/2)], 1) : '—'}
              sub={t('exo.earthMasses')}
            />
          </div>

          <div>
            <p className="text-zinc-500 text-xs uppercase tracking-widest mb-2">{t('exo.sample')}</p>
            <table className="w-full text-xs border-collapse">
              <thead>
                <tr className="border-b border-zinc-800">
                  {['exo.colPlanet', 'exo.colHost', 'exo.colPeriod', 'exo.colRadius', 'exo.colYear'].map(k => (
                    <th key={k} className="text-left text-zinc-500 font-normal pb-1.5 pr-3 last:pr-0 whitespace-nowrap">{t(k)}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {sample.map(p => (
                  <tr key={p.pl_name} className="border-b border-zinc-800/40">
                    <td className="py-1.5 pr-3 text-zinc-200 font-mono truncate max-w-[110px]" title={p.pl_name}>{p.pl_name}</td>
                    <td className="py-1.5 pr-3 text-zinc-400 truncate max-w-[80px]"           title={p.hostname}>{p.hostname}</td>
                    <td className="py-1.5 pr-3 text-zinc-300 font-mono text-right">{p.pl_orbper != null ? fmtNum(p.pl_orbper, 1) : '—'}</td>
                    <td className="py-1.5 pr-3 text-zinc-300 font-mono text-right">{p.pl_rade  != null ? fmtNum(p.pl_rade, 2)  : '—'}</td>
                    <td className="py-1.5        text-zinc-500 font-mono">{p.disc_year ?? '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  )
}
