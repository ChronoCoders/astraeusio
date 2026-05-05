export default function MetricCard({ label, value, unit, sub, valueCls = 'text-zinc-100' }) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-1 min-w-0">
      <span className="text-zinc-500 text-xs uppercase tracking-widest truncate">{label}</span>
      <span className={`font-mono text-2xl font-semibold leading-none truncate ${valueCls}`}>
        {value ?? '—'}
        {unit && <span className="text-sm font-normal text-zinc-500 ml-1">{unit}</span>}
      </span>
      {sub && <span className="text-zinc-400 text-xs truncate">{sub}</span>}
    </div>
  )
}
