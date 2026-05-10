import { useState, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { flattenNeo, fmtNum } from '../lib/utils'

const PAGE_SIZE = 10
const TODAY = new Date().toISOString().slice(0, 10)
const IN_3_DAYS = new Date(Date.now() + 3 * 86400000).toISOString().slice(0, 10)

export default function AsteroidTable({ data }) {
  const { t } = useTranslation()
  const [page, setPage] = useState(1)
  const [distFilter, setDistFilter] = useState('all')
  const [dateFilter, setDateFilter] = useState('all')
  const [hazOnly, setHazOnly] = useState(false)

  const allRows = useMemo(() => flattenNeo(data), [data])

  const filtered = useMemo(() => {
    let rows = allRows
    if (hazOnly) rows = rows.filter(r => r.hazardous)
    if (distFilter === 'lt1') rows = rows.filter(r => r.lunar < 1)
    if (distFilter === 'lt05') rows = rows.filter(r => r.lunar < 0.5)
    if (dateFilter === 'today') rows = rows.filter(r => r.date === TODAY)
    if (dateFilter === '3d') rows = rows.filter(r => r.date <= IN_3_DAYS)
    return rows
  }, [allRows, hazOnly, distFilter, dateFilter])

  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE))
  const safePage = Math.min(page, totalPages)
  const rows = filtered.slice((safePage - 1) * PAGE_SIZE, safePage * PAGE_SIZE)

  function handleFilter(setter) {
    return (val) => { setter(val); setPage(1) }
  }

  const pageNums = () => {
    const pages = []
    for (let i = 1; i <= totalPages; i++) {
      if (i === 1 || i === totalPages || Math.abs(i - safePage) <= 1) pages.push(i)
      else if (pages[pages.length - 1] !== '…') pages.push('…')
    }
    return pages
  }

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">{t('neo.title')}</span>
        {data && <span className="text-zinc-500 text-xs font-mono">{t('neo.objects', { count: data.element_count })}</span>}
      </div>

      <div className="flex flex-wrap gap-2 items-center">
        <select
          value={dateFilter}
          onChange={e => handleFilter(setDateFilter)(e.target.value)}
          className="bg-zinc-800 border border-zinc-700 text-zinc-300 text-xs rounded px-2 py-1 focus:outline-none focus:border-zinc-500"
        >
          <option value="all">All dates</option>
          <option value="today">Today</option>
          <option value="3d">Next 3 days</option>
        </select>

        <select
          value={distFilter}
          onChange={e => handleFilter(setDistFilter)(e.target.value)}
          className="bg-zinc-800 border border-zinc-700 text-zinc-300 text-xs rounded px-2 py-1 focus:outline-none focus:border-zinc-500"
        >
          <option value="all">All distances</option>
          <option value="lt1">&lt; 1 LD</option>
          <option value="lt05">&lt; 0.5 LD</option>
        </select>

        <button
          onClick={() => handleFilter(setHazOnly)(!hazOnly)}
          className={`text-xs px-2 py-1 rounded border transition-colors ${hazOnly ? 'bg-red-900/40 border-red-700 text-red-400' : 'bg-zinc-800 border-zinc-700 text-zinc-400 hover:border-zinc-500'}`}
        >
          Hazardous only
        </button>

        <span className="text-zinc-600 text-xs ml-auto">{filtered.length} results</span>
      </div>

      {!rows.length ? (
        <p className="text-zinc-600 text-sm">No asteroids match the current filters.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm border-collapse">
            <thead>
              <tr className="border-b border-zinc-800">
                {['neo.colName', 'neo.colDate', 'neo.colDistance', 'neo.colDiameter', 'neo.colStatus'].map(k => (
                  <th key={k} className="text-left text-zinc-500 text-xs font-normal pb-2 pr-4 last:pr-0 whitespace-nowrap">{t(k)}</th>
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
                      ? <span className="text-xs font-medium text-red-400 border border-red-800 rounded px-1.5 py-0.5">{t('neo.hazardous')}</span>
                      : <span className="text-xs text-zinc-500 border border-zinc-700 rounded px-1.5 py-0.5">{t('neo.safe')}</span>
                    }
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {totalPages > 1 && (
        <div className="flex items-center justify-center gap-1 pt-1">
          <button
            onClick={() => setPage(p => Math.max(1, p - 1))}
            disabled={safePage === 1}
            className="text-xs px-2 py-1 rounded bg-zinc-800 border border-zinc-700 text-zinc-400 hover:border-zinc-500 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
          >
            ‹
          </button>

          {pageNums().map((n, i) =>
            n === '…'
              ? <span key={`ellipsis-${i}`} className="text-zinc-600 text-xs px-1">…</span>
              : <button
                  key={n}
                  onClick={() => setPage(n)}
                  className={`text-xs w-7 h-7 rounded border transition-colors ${safePage === n ? 'bg-orange-500/20 border-orange-600 text-orange-400' : 'bg-zinc-800 border-zinc-700 text-zinc-400 hover:border-zinc-500'}`}
                >
                  {n}
                </button>
          )}

          <button
            onClick={() => setPage(p => Math.min(totalPages, p + 1))}
            disabled={safePage === totalPages}
            className="text-xs px-2 py-1 rounded bg-zinc-800 border border-zinc-700 text-zinc-400 hover:border-zinc-500 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
          >
            ›
          </button>
        </div>
      )}
    </div>
  )
}
