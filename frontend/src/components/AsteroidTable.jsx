import { flattenNeo, fmtNum } from '../lib/utils'

export default function AsteroidTable({ data }) {
  const rows = flattenNeo(data).slice(0, 12)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">Near-Earth Objects (7-day)</span>
        {data && <span className="text-zinc-500 text-xs font-mono">{data.element_count} objects</span>}
      </div>

      {!rows.length ? (
        <p className="text-zinc-600 text-sm">No data</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm border-collapse">
            <thead>
              <tr className="border-b border-zinc-800">
                {['Name', 'Date', 'Distance (LD)', 'Diameter (km)', 'Status'].map(h => (
                  <th key={h} className="text-left text-zinc-500 text-xs font-normal pb-2 pr-4 last:pr-0 whitespace-nowrap">{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map(r => (
                <tr key={r.id} className="border-b border-zinc-800/50 hover:bg-zinc-800/30 transition-colors">
                  <td className="py-2 pr-4 text-zinc-200 font-mono text-xs max-w-[180px] truncate" title={r.name}>{r.name}</td>
                  <td className="py-2 pr-4 text-zinc-400 font-mono text-xs whitespace-nowrap">{r.date}</td>
                  <td className="py-2 pr-4 text-zinc-200 font-mono text-xs text-right">{fmtNum(r.lunar, 2)}</td>
                  <td className="py-2 pr-4 text-zinc-400 font-mono text-xs text-right whitespace-nowrap">
                    {fmtNum(r.diamMin * 1000, 0)}–{fmtNum(r.diamMax * 1000, 0)} m
                  </td>
                  <td className="py-2">
                    {r.hazardous
                      ? <span className="text-xs font-medium text-red-400 border border-red-800 rounded px-1.5 py-0.5">HAZARDOUS</span>
                      : <span className="text-xs text-zinc-500 border border-zinc-700 rounded px-1.5 py-0.5">safe</span>
                    }
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
