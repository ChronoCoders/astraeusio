function severity(productId) {
  if (/WAR/.test(productId)) return { dot: 'bg-red-400',    label: 'WARNING' }
  if (/ALT/.test(productId)) return { dot: 'bg-orange-400', label: 'ALERT' }
  if (/SUM/.test(productId)) return { dot: 'bg-yellow-400', label: 'SUMMARY' }
  return                            { dot: 'bg-zinc-500',   label: 'INFO' }
}

function firstLine(msg) {
  // Extract the message code line from the NOAA alert text
  const m = msg.match(/Space Weather Message Code:\s*(\S+)/i)
  if (m) return m[1].replace(/_/g, ' ')
  return msg.split('\n').find(l => l.trim()) ?? msg.slice(0, 60)
}

function parseDate(dt) {
  try { return new Date(dt).toLocaleString('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false }) }
  catch { return dt }
}

export default function AlertsList({ data }) {
  const alerts = (data ?? []).slice(0, 8)

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">Space Weather Alerts</span>
        <span className="text-zinc-500 text-xs font-mono">{alerts.length}</span>
      </div>

      {alerts.length === 0 ? (
        <p className="text-zinc-600 text-sm">No active alerts</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {alerts.map((a, i) => {
            const sev = severity(a.product_id)
            return (
              <li key={i} className="flex gap-2 items-start border-b border-zinc-800/50 pb-2 last:border-0 last:pb-0">
                <span className={`mt-1.5 w-1.5 h-1.5 rounded-full flex-shrink-0 ${sev.dot}`} />
                <div className="min-w-0">
                  <div className="flex items-center gap-2 mb-0.5">
                    <span className="text-xs text-zinc-500 font-mono">{sev.label}</span>
                    <span className="text-xs text-zinc-600 font-mono">{a.product_id}</span>
                  </div>
                  <p className="text-zinc-300 text-xs leading-relaxed line-clamp-2">{firstLine(a.message)}</p>
                  <p className="text-zinc-600 text-xs mt-0.5">{parseDate(a.issue_datetime)}</p>
                </div>
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
