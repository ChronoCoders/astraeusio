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
  const planets = data ?? []

  const withRadius = planets.filter(p => p.pl_rade != null)
  const withMass   = planets.filter(p => p.pl_masse != null)
  const withPeriod = planets.filter(p => p.pl_orbper != null)

  const byYear = planets.reduce((acc, p) => {
    if (p.disc_year) acc[p.disc_year] = (acc[p.disc_year] ?? 0) + 1
    return acc
  }, {})
  const recentYear = Object.keys(byYear).sort().at(-1)

  const sample = planets.slice(0, 8)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-4">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">Exoplanet Catalog</span>

      {planets.length === 0 ? (
        <p className="text-zinc-600 text-sm">No data</p>
      ) : (
        <>
          <div className="grid grid-cols-3 gap-2">
            <StatBox label="Total"  value={fmtNum(planets.length)} sub="planets" />
            <StatBox
              label="Median radius"
              value={withRadius.length ? fmtNum(withRadius.map(p => p.pl_rade).sort((a,b) => a-b)[Math.floor(withRadius.length/2)], 1) : '—'}
              sub="Earth radii"
            />
            <StatBox
              label="Median mass"
              value={withMass.length ? fmtNum(withMass.map(p => p.pl_masse).sort((a,b) => a-b)[Math.floor(withMass.length/2)], 1) : '—'}
              sub="Earth masses"
            />
          </div>

          <div>
            <p className="text-zinc-500 text-xs uppercase tracking-widest mb-2">Sample — closest orbits</p>
            <table className="w-full text-xs border-collapse">
              <thead>
                <tr className="border-b border-zinc-800">
                  {['Planet', 'Host', 'Period (d)', 'Radius (R⊕)', 'Year'].map(h => (
                    <th key={h} className="text-left text-zinc-500 font-normal pb-1.5 pr-3 last:pr-0 whitespace-nowrap">{h}</th>
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
