import { stormProb, auroraLine, stormInfo, fmtNum } from '../lib/utils'

function Bar({ value, max = 1, cls }) {
  const pct = Math.round(Math.min(value / max, 1) * 100)
  return (
    <div className="flex items-center gap-2">
      <div className="flex-1 h-1.5 bg-zinc-800 rounded-full overflow-hidden">
        <div className={`h-full rounded-full ${cls}`} style={{ width: `${pct}%` }} />
      </div>
      <span className="font-mono text-xs text-zinc-300 w-8 text-right">{pct}%</span>
    </div>
  )
}

export default function ForecastPanel({ data, loading, error }) {
  const Row = ({ label, children }) => (
    <div className="flex flex-col gap-1">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">{label}</span>
      {children}
    </div>
  )

  if (loading) return <Panel><p className="text-zinc-600 text-sm">Loading forecast…</p></Panel>
  if (error)   return <Panel><p className="text-red-500 text-sm">{error}</p></Panel>
  if (!data)   return <Panel><p className="text-zinc-600 text-sm">No forecast data</p></Panel>

  const kp      = data.predicted_kp
  const prob    = stormProb(kp, data.uncertainty)
  const storm   = stormInfo(kp)
  const aurora  = auroraLine(kp)

  return (
    <Panel>
      <Row label="Predicted Kp (+3 h)">
        <span className={`font-mono text-3xl font-semibold ${storm.cls}`}>{fmtNum(kp, 2)}</span>
      </Row>

      <Row label="Confidence interval (95%)">
        <div className="flex items-center gap-1 font-mono text-sm">
          <span className="text-zinc-300">{fmtNum(data.ci_lower, 2)}</span>
          <span className="text-zinc-600 px-1">—</span>
          <span className="text-zinc-300">{fmtNum(data.ci_upper, 2)}</span>
          <span className="text-zinc-500 text-xs ml-1">Kp</span>
        </div>
      </Row>

      <Row label="Storm probability (Kp ≥ 5)">
        <Bar value={prob} cls={prob > 0.5 ? 'bg-orange-400' : prob > 0.2 ? 'bg-yellow-400' : 'bg-zinc-400'} />
      </Row>

      <Row label="Storm level">
        <span className={`text-sm font-medium ${storm.cls}`}>{storm.label}</span>
      </Row>

      <Row label="Aurora visibility">
        <span className="text-zinc-300 text-sm">{aurora}</span>
      </Row>

      <Row label="Uncertainty (1σ)">
        <span className="font-mono text-sm text-zinc-400">{fmtNum(data.uncertainty, 4)} Kp</span>
      </Row>

      <p className="text-zinc-600 text-xs mt-auto pt-2 border-t border-zinc-800">
        LSTM · {data.n_mc_samples} MC passes · trained {data.trained_through}
      </p>
    </Panel>
  )
}

function Panel({ children }) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-4 h-full">
      <span className="text-zinc-500 text-xs uppercase tracking-widest">ML Kp Forecast</span>
      {children}
    </div>
  )
}
