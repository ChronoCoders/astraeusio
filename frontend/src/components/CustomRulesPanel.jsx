import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import UpgradePrompt from './UpgradePrompt'

const METRICS = [
  { value: 'kp',               label: 'Kp Index',          unit: '(0–9)' },
  { value: 'solar_wind_speed', label: 'Solar Wind Speed',  unit: 'km/s' },
  { value: 'xray_flux',        label: 'X-ray Flux',        unit: 'W/m²' },
  { value: 'dst',              label: 'Dst Index',         unit: 'nT' },
  { value: 'imf_bz',           label: 'IMF Bz',            unit: 'nT' },
]

const OPERATORS = [
  { value: 'gt',  label: '> (above)' },
  { value: 'lt',  label: '< (below)' },
  { value: 'gte', label: '≥ (above or equal)' },
  { value: 'lte', label: '≤ (below or equal)' },
]

function opLabel(op) {
  return OPERATORS.find(o => o.value === op)?.label ?? op
}

function metricLabel(m) {
  const found = METRICS.find(x => x.value === m)
  return found ? `${found.label} (${found.unit})` : m
}

function Badge({ severity }) {
  return (
    <span className={`inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-mono font-semibold uppercase tracking-wider ${
      severity === 'critical'
        ? 'bg-red-900/60 text-red-300 border border-red-700/50'
        : 'bg-amber-900/50 text-amber-300 border border-amber-700/40'
    }`}>
      {severity}
    </span>
  )
}

export default function CustomRulesPanel({ plan, onNavigate }) {
  useTranslation()
  const [rules, setRules]       = useState([])
  const [loading, setLoading]   = useState(true)  // true = loading on mount
  const [error, setError]       = useState(null)
  const [showForm, setShowForm] = useState(false)
  const [saving, setSaving]     = useState(false)
  const [formErr, setFormErr]   = useState(null)

  const [form, setForm] = useState({
    name: '', metric: 'kp', operator: 'gt', threshold: '', severity: 'warning',
  })

  const token = localStorage.getItem('token')

  useEffect(() => {
    let cancelled = false
    fetch('/api/custom-rules', { headers: { Authorization: `Bearer ${token}` } })
      .then(r => {
        if (r.status === 403) throw new Error('HTTP 403')
        if (!r.ok) throw new Error('fetch failed')
        return r.json()
      })
      .then(d => { if (!cancelled) { setRules(d); setError(null); setLoading(false) } })
      .catch(e => { if (!cancelled) { setError(e.message); setLoading(false) } })
    return () => { cancelled = true }
  }, [token])

  const isEnterprise = plan === 'enterprise'

  if (!isEnterprise) {
    return (
      <div className="bg-zinc-900 border border-zinc-800 rounded p-4">
        <p className="text-zinc-500 text-xs uppercase tracking-widest mb-3">Custom Anomaly Rules</p>
        <UpgradePrompt
          messageKey="plan.lockedCustomRules"
          requiredPlan="enterprise"
          onUpgrade={onNavigate ? () => onNavigate('settings') : undefined}
        />
      </div>
    )
  }

  async function handleCreate(e) {
    e.preventDefault()
    setFormErr(null)
    const thresh = parseFloat(form.threshold)
    if (!form.name.trim()) { setFormErr('Name is required'); return }
    if (isNaN(thresh)) { setFormErr('Threshold must be a number'); return }
    setSaving(true)
    try {
      const res = await fetch('/api/custom-rules', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
        body: JSON.stringify({ ...form, threshold: thresh }),
      })
      if (!res.ok) {
        const body = await res.json().catch(() => ({}))
        setFormErr(body.error ?? 'Failed to create rule')
        return
      }
      const rule = await res.json()
      setRules(r => [...r, rule])
      setShowForm(false)
      setForm({ name: '', metric: 'kp', operator: 'gt', threshold: '', severity: 'warning' })
    } catch {
      setFormErr('Network error')
    } finally {
      setSaving(false)
    }
  }

  async function handleDelete(id) {
    await fetch(`/api/custom-rules/${id}`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${token}` },
    })
    setRules(r => r.filter(x => x.id !== id))
  }

  async function handleToggle(id, enabled) {
    await fetch(`/api/custom-rules/${id}/toggle`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
      body: JSON.stringify({ enabled }),
    })
    setRules(r => r.map(x => x.id === id ? { ...x, enabled } : x))
  }

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <span className="text-zinc-500 text-xs uppercase tracking-widest">Custom Anomaly Rules</span>
        {!showForm && (
          <button
            onClick={() => setShowForm(true)}
            className="text-xs font-mono text-orange-400 hover:text-orange-300 transition-colors"
          >
            + Add Rule
          </button>
        )}
      </div>

      {showForm && (
        <form onSubmit={handleCreate} className="flex flex-col gap-2 border border-zinc-700 rounded p-3 bg-zinc-800/50">
          <p className="text-xs text-zinc-400 font-mono mb-1">New Rule</p>
          <input
            type="text"
            placeholder="Rule name"
            value={form.name}
            onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
            maxLength={80}
            className="bg-zinc-900 border border-zinc-700 rounded px-2 py-1.5 text-sm text-zinc-100 placeholder-zinc-600 focus:outline-none focus:border-zinc-500"
          />
          <div className="flex gap-2 flex-wrap">
            <select
              value={form.metric}
              onChange={e => setForm(f => ({ ...f, metric: e.target.value }))}
              className="bg-zinc-900 border border-zinc-700 rounded px-2 py-1.5 text-sm text-zinc-100 focus:outline-none focus:border-zinc-500 flex-1 min-w-0"
            >
              {METRICS.map(m => (
                <option key={m.value} value={m.value}>{m.label}</option>
              ))}
            </select>
            <select
              value={form.operator}
              onChange={e => setForm(f => ({ ...f, operator: e.target.value }))}
              className="bg-zinc-900 border border-zinc-700 rounded px-2 py-1.5 text-sm text-zinc-100 focus:outline-none focus:border-zinc-500"
            >
              {OPERATORS.map(o => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
            </select>
            <input
              type="number"
              step="any"
              placeholder="Threshold"
              value={form.threshold}
              onChange={e => setForm(f => ({ ...f, threshold: e.target.value }))}
              className="bg-zinc-900 border border-zinc-700 rounded px-2 py-1.5 text-sm text-zinc-100 placeholder-zinc-600 focus:outline-none focus:border-zinc-500 w-32"
            />
            <select
              value={form.severity}
              onChange={e => setForm(f => ({ ...f, severity: e.target.value }))}
              className="bg-zinc-900 border border-zinc-700 rounded px-2 py-1.5 text-sm text-zinc-100 focus:outline-none focus:border-zinc-500"
            >
              <option value="warning">Warning</option>
              <option value="critical">Critical</option>
            </select>
          </div>
          {formErr && <p className="text-red-400 text-xs">{formErr}</p>}
          <div className="flex gap-2 justify-end">
            <button
              type="button"
              onClick={() => { setShowForm(false); setFormErr(null) }}
              className="text-xs text-zinc-500 hover:text-zinc-300 font-mono px-2 py-1"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={saving}
              className="text-xs font-mono bg-orange-500/20 text-orange-300 border border-orange-500/30 rounded px-3 py-1 hover:bg-orange-500/30 transition-colors disabled:opacity-50"
            >
              {saving ? 'Saving…' : 'Create'}
            </button>
          </div>
        </form>
      )}

      {loading && <p className="text-zinc-600 text-sm">Loading…</p>}
      {error && error !== 'HTTP 403' && <p className="text-red-500 text-sm">Failed to load rules</p>}

      {!loading && !error && rules.length === 0 && (
        <p className="text-zinc-600 text-sm">No custom rules yet. Add one to trigger anomaly alerts on your own thresholds.</p>
      )}

      {!loading && !error && rules.length > 0 && (
        <ul className="flex flex-col gap-1.5">
          {rules.map(rule => (
            <li
              key={rule.id}
              className={`flex items-center gap-3 rounded px-3 py-2 border ${
                rule.severity === 'critical'
                  ? 'bg-red-950/20 border-red-900/30'
                  : 'bg-amber-950/10 border-amber-900/20'
              } ${!rule.enabled ? 'opacity-40' : ''}`}
            >
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 flex-wrap">
                  <Badge severity={rule.severity} />
                  <span className="text-zinc-200 text-xs font-medium truncate">{rule.name}</span>
                </div>
                <p className="text-zinc-500 text-[11px] font-mono mt-0.5">
                  {metricLabel(rule.metric)} {opLabel(rule.operator)} {rule.threshold}
                </p>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <button
                  onClick={() => handleToggle(rule.id, !rule.enabled)}
                  title={rule.enabled ? 'Disable' : 'Enable'}
                  className={`text-xs font-mono px-2 py-0.5 rounded border transition-colors ${
                    rule.enabled
                      ? 'border-zinc-600 text-zinc-400 hover:text-zinc-200'
                      : 'border-zinc-700 text-zinc-600 hover:text-zinc-400'
                  }`}
                >
                  {rule.enabled ? 'On' : 'Off'}
                </button>
                <button
                  onClick={() => handleDelete(rule.id)}
                  title="Delete rule"
                  className="text-zinc-600 hover:text-red-400 text-xs font-mono transition-colors"
                >
                  ✕
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}

      <p className="text-zinc-700 text-[10px] font-mono text-center">
        Rules run every 60 s · {rules.length}/20 used
      </p>
    </div>
  )
}
