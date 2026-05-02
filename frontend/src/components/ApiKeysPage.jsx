import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import UpgradePrompt from './UpgradePrompt'
import { planSatisfies } from '../lib/utils'

function authHeader() {
  return { Authorization: `Bearer ${localStorage.getItem('token')}` }
}

function fmtTs(ts) {
  if (ts == null) return '—'
  return new Date(ts * 1000).toISOString().replace('T', ' ').slice(0, 19) + ' UTC'
}

// ── Endpoint docs ──────────────────────────────────────────────────────────────

const ENDPOINTS = [
  { method: 'GET',  path: '/api/kp',               desc: 'NOAA 1-min Kp index (last 24 h)' },
  { method: 'GET',  path: '/api/kp-3h',             desc: 'NOAA 3-hour official Kp (last 7 days)' },
  { method: 'GET',  path: '/api/kp-forecast',       desc: 'ML Kp forecast (+3 h, LSTM)' },
  { method: 'GET',  path: '/api/solar-wind',        desc: 'Solar wind speed, density, temperature' },
  { method: 'GET',  path: '/api/xray',              desc: 'GOES X-ray flux (0.1-0.8 nm & 0.05-0.4 nm)' },
  { method: 'GET',  path: '/api/imf',               desc: 'IMF Bz/Bt — DSCOVR magnetometer' },
  { method: 'GET',  path: '/api/dst',               desc: 'Dst disturbance storm-time index' },
  { method: 'GET',  path: '/api/alerts',            desc: 'NOAA space weather alerts' },
  { method: 'GET',  path: '/api/anomalies',         desc: 'Locally detected anomalies (5 checks)' },
  { method: 'GET',  path: '/api/iss',               desc: 'ISS position (lat, lon, altitude, velocity)' },
  { method: 'GET',  path: '/api/apod',              desc: 'NASA Astronomy Picture of the Day' },
  { method: 'GET',  path: '/api/neo',               desc: 'Near-Earth objects — 7-day window' },
  { method: 'GET',  path: '/api/epic',              desc: 'NASA EPIC Earth imagery' },
  { method: 'GET',  path: '/api/exoplanets',        desc: 'NASA Exoplanet Archive (top 100)' },
  { method: 'GET',  path: '/api/starlink',          desc: 'Starlink TLE constellation data' },
  { method: 'GET',  path: '/api/reports/summary',   desc: 'Summary stats — ?range=24h|7d|30d' },
  { method: 'GET',  path: '/api/reports/export',    desc: 'CSV export — ?range=24h|7d|30d' },
]

// ── Component ─────────────────────────────────────────────────────────────────

export default function ApiKeysPage({ plan }) {
  const { t } = useTranslation()
  const [{ keys, error, loadedSeq }, setListState] = useState({ keys: [], error: null, loadedSeq: -1 })
  const [name, setName]           = useState('')
  const [creating, setCreating]   = useState(false)
  const [createError, setCreateError] = useState(null)
  const [newKey, setNewKey]       = useState(null)   // { id, key, name } shown once
  const [copied, setCopied]       = useState(false)
  const [confirmed, setConfirmed] = useState(false)
  const [seq, setSeq]             = useState(0)
  const loading = loadedSeq !== seq

  useEffect(() => {
    let cancelled = false
    fetch('/api/keys', { headers: authHeader() })
      .then(r => r.json())
      .then(d => { if (!cancelled) setListState({ keys: Array.isArray(d) ? d : [], error: null, loadedSeq: seq }) })
      .catch(e => { if (!cancelled) setListState({ keys: [], error: e.message, loadedSeq: seq }) })
    return () => { cancelled = true }
  }, [seq])

  async function handleCreate(e) {
    e.preventDefault()
    const n = name.trim()
    if (!n) return
    setCreating(true)
    setCreateError(null)
    try {
      const r = await fetch('/api/keys', {
        method: 'POST',
        headers: { ...authHeader(), 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: n }),
      })
      if (!r.ok) {
        const d = await r.json().catch(() => ({}))
        setCreateError(d.error ?? t('apiKeys.createError'))
      } else {
        const d = await r.json()
        setNewKey(d)
        setName('')
        setCopied(false)
        setConfirmed(false)
        setSeq(n => n + 1)
      }
    } catch {
      setCreateError(t('apiKeys.createError'))
    } finally {
      setCreating(false)
    }
  }

  async function handleDelete(id) {
    const r = await fetch(`/api/keys/${id}`, {
      method: 'DELETE',
      headers: authHeader(),
    })
    if (r.ok || r.status === 204) {
      setSeq(n => n + 1)
    }
  }

  function handleCopy() {
    if (!newKey) return
    navigator.clipboard.writeText(newKey.key).then(() => setCopied(true))
  }

  function handleDismiss() {
    setNewKey(null)
    setCopied(false)
    setConfirmed(false)
  }

  // Null plan means still loading — show full UI optimistically.
  if (plan !== null && !planSatisfies(plan, 'developer')) {
    return (
      <div className="flex flex-col gap-6 max-w-4xl">
        <div className="bg-zinc-900 border border-zinc-800 rounded">
          <UpgradePrompt messageKey="plan.lockedApiKeys" requiredPlan="developer" />
        </div>
        <EmailAlertsSection plan={plan} />
        <WebhooksSection plan={plan} />
        <ApiDocsSection t={t} />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6 max-w-4xl">

      {/* ── New key banner (shown once after creation) ──────────────────── */}
      {newKey && (
        <div className="bg-amber-950/40 border border-amber-700 rounded p-4 flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <span className="text-amber-400 text-xs font-mono uppercase tracking-widest">
              {t('apiKeys.newKeyTitle')}
            </span>
          </div>
          <p className="text-amber-300 text-xs">{t('apiKeys.newKeyWarning')}</p>
          <div className="flex items-center gap-2">
            <code className="flex-1 bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-xs font-mono text-zinc-200 break-all">
              {newKey.key}
            </code>
            <button
              onClick={handleCopy}
              className="shrink-0 px-3 py-2 text-xs font-mono rounded border border-zinc-700 bg-zinc-900 text-zinc-300 hover:text-zinc-100 hover:border-zinc-500 transition-colors"
            >
              {copied ? t('apiKeys.copied') : t('apiKeys.copy')}
            </button>
          </div>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={e => setConfirmed(e.target.checked)}
              className="accent-amber-500"
            />
            <span className="text-zinc-400 text-xs">{t('apiKeys.confirmCopied')}</span>
          </label>
          <button
            onClick={handleDismiss}
            disabled={!confirmed}
            className={[
              'self-start px-4 py-1.5 text-xs font-mono rounded transition-colors',
              confirmed
                ? 'bg-amber-700 text-amber-100 hover:bg-amber-600'
                : 'bg-zinc-800 text-zinc-600 cursor-not-allowed',
            ].join(' ')}
          >
            {t('apiKeys.dismiss')}
          </button>
        </div>
      )}

      {/* ── Create new key ─────────────────────────────────────────────── */}
      <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
        <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
          {t('apiKeys.createTitle')}
        </span>
        <form onSubmit={handleCreate} className="flex items-center gap-3">
          <input
            type="text"
            value={name}
            onChange={e => setName(e.target.value)}
            placeholder={t('apiKeys.namePlaceholder')}
            maxLength={64}
            className="flex-1 bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 placeholder-zinc-600 font-mono focus:outline-none focus:border-zinc-500"
          />
          <button
            type="submit"
            disabled={creating || !name.trim()}
            className="px-4 py-2 text-xs font-mono rounded bg-zinc-700 text-zinc-100 hover:bg-zinc-600 disabled:bg-zinc-800 disabled:text-zinc-600 transition-colors"
          >
            {creating ? t('common.loading') : t('apiKeys.generate')}
          </button>
        </form>
        {createError && (
          <p className="text-red-400 text-xs font-mono">{createError}</p>
        )}
      </div>

      {/* ── Key list ───────────────────────────────────────────────────── */}
      <div className="bg-zinc-900 border border-zinc-800 rounded flex flex-col">
        <div className="px-4 py-3 border-b border-zinc-800 flex items-center justify-between">
          <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
            {t('apiKeys.listTitle')}
          </span>
          <span className="text-zinc-700 text-xs font-mono">{keys.length} {t('apiKeys.keys')}</span>
        </div>

        {loading ? (
          <p className="text-zinc-600 text-xs font-mono p-4">{t('common.loading')}</p>
        ) : error ? (
          <p className="text-red-400 text-xs font-mono p-4">{error}</p>
        ) : keys.length === 0 ? (
          <p className="text-zinc-600 text-xs font-mono p-4">{t('apiKeys.noKeys')}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-xs font-mono">
              <thead>
                <tr className="border-b border-zinc-800 text-zinc-600">
                  <th className="text-left px-4 py-2 font-normal">{t('apiKeys.colName')}</th>
                  <th className="text-left px-4 py-2 font-normal">{t('apiKeys.colCreated')}</th>
                  <th className="text-left px-4 py-2 font-normal">{t('apiKeys.colLastUsed')}</th>
                  <th className="text-right px-4 py-2 font-normal">{t('apiKeys.colRequests')}</th>
                  <th className="px-4 py-2" />
                </tr>
              </thead>
              <tbody>
                {keys.map(k => (
                  <tr key={k.id} className="border-b border-zinc-800/50 hover:bg-zinc-800/30">
                    <td className="px-4 py-2 text-zinc-300">{k.name}</td>
                    <td className="px-4 py-2 text-zinc-500">{fmtTs(k.created_at)}</td>
                    <td className="px-4 py-2 text-zinc-500">{fmtTs(k.last_used_at)}</td>
                    <td className="px-4 py-2 text-right text-zinc-400 tabular-nums">{k.request_count}</td>
                    <td className="px-4 py-2 text-right">
                      <button
                        onClick={() => handleDelete(k.id)}
                        className="text-zinc-600 hover:text-red-400 transition-colors"
                        title={t('apiKeys.delete')}
                      >
                        <svg width="14" height="14" viewBox="0 0 14 14" fill="none"
                          stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                          <line x1="1" y1="1" x2="13" y2="13" />
                          <line x1="13" y1="1" x2="1" y2="13" />
                        </svg>
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <EmailAlertsSection plan={plan} />
      <WebhooksSection plan={plan} />
      <ApiDocsSection t={t} />

    </div>
  )
}

// ── Email Alerts ──────────────────────────────────────────────────────────────

function EmailAlertsSection({ plan }) {
  const { t } = useTranslation()
  const isDev = plan === null || planSatisfies(plan, 'developer')

  const [settings, setSettings]     = useState(null)
  const [enabled, setEnabled]       = useState(false)
  const [kpThreshold, setKp]        = useState('5.0')
  const [windThreshold, setWind]    = useState('700')
  const [saving, setSaving]         = useState(false)
  const [feedback, setFeedback]     = useState(null)

  useEffect(() => {
    if (!isDev) return
    fetch('/api/email-alerts', { headers: authHeader() })
      .then(r => r.json())
      .then(d => {
        setEnabled(d.enabled ?? false)
        setKp(String(d.kp_threshold ?? 5.0))
        setWind(String(d.wind_threshold ?? 700))
        setSettings(d)
      })
      .catch(() => setSettings({}))
  }, [isDev])

  if (!isDev) {
    return (
      <div className="bg-zinc-900 border border-zinc-800 rounded">
        <UpgradePrompt messageKey="plan.lockedEmailAlerts" requiredPlan="developer" />
      </div>
    )
  }

  if (settings === null) {
    return (
      <div className="bg-zinc-900 border border-zinc-800 rounded p-4">
        <p className="text-zinc-600 text-xs font-mono">{t('common.loading')}</p>
      </div>
    )
  }

  async function handleSave(e) {
    e.preventDefault()
    setSaving(true)
    setFeedback(null)
    try {
      const r = await fetch('/api/email-alerts', {
        method: 'POST',
        headers: { ...authHeader(), 'Content-Type': 'application/json' },
        body: JSON.stringify({
          enabled,
          kp_threshold:   parseFloat(kpThreshold)   || 5.0,
          wind_threshold: parseFloat(windThreshold)  || 700,
        }),
      })
      setFeedback(r.ok ? 'saved' : 'error')
    } catch {
      setFeedback('error')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
        {t('emailAlerts.title')}
      </span>
      <p className="text-zinc-600 text-xs">{t('emailAlerts.note')}</p>
      <form onSubmit={handleSave} className="flex flex-col gap-4">
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={enabled}
            onChange={e => setEnabled(e.target.checked)}
            className="accent-indigo-500"
          />
          <span className="text-zinc-300 text-xs font-mono">{t('emailAlerts.enable')}</span>
        </label>
        <div className="flex flex-wrap items-end gap-4">
          <div className="flex flex-col gap-1">
            <label className="text-zinc-500 text-[11px] font-mono">{t('emailAlerts.kpThreshold')}</label>
            <input
              type="number"
              min="1"
              max="9"
              step="0.1"
              value={kpThreshold}
              onChange={e => setKp(e.target.value)}
              className="w-24 bg-zinc-950 border border-zinc-700 rounded px-3 py-1.5 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500"
            />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-500 text-[11px] font-mono">{t('emailAlerts.windThreshold')}</label>
            <input
              type="number"
              min="100"
              max="2000"
              step="10"
              value={windThreshold}
              onChange={e => setWind(e.target.value)}
              className="w-28 bg-zinc-950 border border-zinc-700 rounded px-3 py-1.5 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500"
            />
          </div>
        </div>
        <div className="flex items-center gap-3">
          <button
            type="submit"
            disabled={saving}
            className="px-4 py-2 text-xs font-mono rounded bg-zinc-700 text-zinc-100 hover:bg-zinc-600 disabled:bg-zinc-800 disabled:text-zinc-600 transition-colors"
          >
            {saving ? t('common.loading') : t('emailAlerts.save')}
          </button>
          {feedback === 'saved' && (
            <span className="text-green-400 text-xs font-mono">{t('emailAlerts.saved')}</span>
          )}
          {feedback === 'error' && (
            <span className="text-red-400 text-xs font-mono">{t('emailAlerts.saveError')}</span>
          )}
        </div>
      </form>
    </div>
  )
}

// ── Webhooks ──────────────────────────────────────────────────────────────────

const WEBHOOK_EVENTS = [
  { id: 'kp_storm',          label: 'Kp Storm (≥G1)' },
  { id: 'solar_wind_speed',  label: 'High Solar Wind' },
  { id: 'xray_flare',        label: 'X-Ray Flare' },
  { id: 'asteroid_close',    label: 'Asteroid Close Approach' },
  { id: 'ml_forecast_storm', label: 'ML Storm Forecast' },
]

function WebhooksSection({ plan }) {
  const { t } = useTranslation()
  const isPro = plan === null || planSatisfies(plan, 'pro')

  if (!isPro) {
    return (
      <div className="bg-zinc-900 border border-zinc-800 rounded">
        <UpgradePrompt messageKey="plan.lockedWebhooks" requiredPlan="pro" />
      </div>
    )
  }

  return <WebhooksCrud t={t} />
}

function WebhooksCrud({ t }) {
  const [{ hooks, loadedSeq }, setListState] = useState({ hooks: [], loadedSeq: -1 })
  const [seq, setSeq]                       = useState(0)
  const [url, setUrl]                       = useState('')
  const [events, setEvents]                 = useState([])
  const [creating, setCreating]             = useState(false)
  const [createError, setCreateError]       = useState(null)
  const [newSecret, setNewSecret]           = useState(null)
  const [secretCopied, setSecretCopied]     = useState(false)
  const loading = loadedSeq !== seq

  useEffect(() => {
    let cancelled = false
    fetch('/api/webhooks', { headers: authHeader() })
      .then(r => r.json())
      .then(d => { if (!cancelled) setListState({ hooks: Array.isArray(d) ? d : [], loadedSeq: seq }) })
      .catch(() => { if (!cancelled) setListState({ hooks: [], loadedSeq: seq }) })
    return () => { cancelled = true }
  }, [seq])

  function toggleEvent(id) {
    setEvents(prev => prev.includes(id) ? prev.filter(e => e !== id) : [...prev, id])
  }

  async function handleCreate(e) {
    e.preventDefault()
    const u = url.trim()
    if (!u || events.length === 0) return
    setCreating(true)
    setCreateError(null)
    try {
      const r = await fetch('/api/webhooks', {
        method: 'POST',
        headers: { ...authHeader(), 'Content-Type': 'application/json' },
        body: JSON.stringify({ url: u, events }),
      })
      if (!r.ok) {
        const d = await r.json().catch(() => ({}))
        setCreateError(d.error ?? t('webhooks.createError'))
      } else {
        const d = await r.json()
        setNewSecret({ id: d.id, secret: d.secret })
        setSecretCopied(false)
        setUrl('')
        setEvents([])
        setSeq(n => n + 1)
      }
    } catch {
      setCreateError(t('webhooks.createError'))
    } finally {
      setCreating(false)
    }
  }

  async function handleDelete(id) {
    await fetch(`/api/webhooks/${id}`, { method: 'DELETE', headers: authHeader() })
    setSeq(n => n + 1)
  }

  return (
    <div className="flex flex-col gap-3">

      {/* ── Secret banner (shown once after creation) ───────────────── */}
      {newSecret && (
        <div className="bg-amber-950/40 border border-amber-700 rounded p-4 flex flex-col gap-3">
          <span className="text-amber-400 text-xs font-mono uppercase tracking-widest">
            {t('webhooks.secretTitle')}
          </span>
          <p className="text-amber-300 text-xs">{t('webhooks.secretWarning')}</p>
          <div className="flex items-center gap-2">
            <code className="flex-1 bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-xs font-mono text-zinc-200 break-all">
              {newSecret.secret}
            </code>
            <button
              onClick={() => navigator.clipboard.writeText(newSecret.secret).then(() => setSecretCopied(true))}
              className="shrink-0 px-3 py-2 text-xs font-mono rounded border border-zinc-700 bg-zinc-900 text-zinc-300 hover:text-zinc-100 transition-colors"
            >
              {secretCopied ? t('apiKeys.copied') : t('apiKeys.copy')}
            </button>
          </div>
          <button
            onClick={() => { setNewSecret(null); setSecretCopied(false) }}
            className="self-start px-4 py-1.5 text-xs font-mono rounded bg-amber-700 text-amber-100 hover:bg-amber-600 transition-colors"
          >
            {t('webhooks.secretDismiss')}
          </button>
        </div>
      )}

      {/* ── Create form ─────────────────────────────────────────────── */}
      <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
        <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
          {t('webhooks.createTitle')}
        </span>
        <form onSubmit={handleCreate} className="flex flex-col gap-3">
          <input
            type="url"
            value={url}
            onChange={e => setUrl(e.target.value)}
            placeholder={t('webhooks.urlPlaceholder')}
            className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 placeholder-zinc-600 font-mono focus:outline-none focus:border-zinc-500"
          />
          <div className="flex flex-wrap gap-x-4 gap-y-2">
            {WEBHOOK_EVENTS.map(ev => (
              <label key={ev.id} className="flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  checked={events.includes(ev.id)}
                  onChange={() => toggleEvent(ev.id)}
                  className="accent-indigo-500"
                />
                <span className="text-zinc-400 text-xs font-mono">{ev.label}</span>
              </label>
            ))}
          </div>
          <div className="flex items-center gap-3">
            <button
              type="submit"
              disabled={creating || !url.trim() || events.length === 0}
              className="px-4 py-2 text-xs font-mono rounded bg-zinc-700 text-zinc-100 hover:bg-zinc-600 disabled:bg-zinc-800 disabled:text-zinc-600 transition-colors"
            >
              {creating ? t('common.loading') : t('webhooks.create')}
            </button>
            {createError && <p className="text-red-400 text-xs font-mono">{createError}</p>}
          </div>
        </form>
      </div>

      {/* ── Webhook list ────────────────────────────────────────────── */}
      <div className="bg-zinc-900 border border-zinc-800 rounded flex flex-col">
        <div className="px-4 py-3 border-b border-zinc-800">
          <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
            {t('webhooks.listTitle')}
          </span>
        </div>
        {loading ? (
          <p className="text-zinc-600 text-xs font-mono p-4">{t('common.loading')}</p>
        ) : hooks.length === 0 ? (
          <p className="text-zinc-600 text-xs font-mono p-4">{t('webhooks.noHooks')}</p>
        ) : (
          <div className="flex flex-col divide-y divide-zinc-800">
            {hooks.map(h => (
              <div key={h.id} className="px-4 py-3 flex items-start justify-between gap-3">
                <div className="flex flex-col gap-1 min-w-0">
                  <code className="text-zinc-200 text-xs font-mono truncate">{h.url}</code>
                  <div className="flex flex-wrap gap-1 mt-0.5">
                    {h.events.map(ev => (
                      <span key={ev} className="text-[10px] font-mono border border-zinc-700 text-zinc-500 rounded px-1.5 py-0.5">
                        {ev}
                      </span>
                    ))}
                  </div>
                  <span className="text-zinc-700 text-[11px] font-mono">{fmtTs(h.created_at)}</span>
                </div>
                <button
                  onClick={() => handleDelete(h.id)}
                  className="shrink-0 text-zinc-600 hover:text-red-400 transition-colors mt-0.5"
                  title={t('apiKeys.delete')}
                >
                  <svg width="14" height="14" viewBox="0 0 14 14" fill="none"
                    stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                    <line x1="1" y1="1" x2="13" y2="13" />
                    <line x1="13" y1="1" x2="1" y2="13" />
                  </svg>
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

    </div>
  )
}

// ── Endpoint documentation ────────────────────────────────────────────────────

function ApiDocsSection({ t }) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded flex flex-col">
      <div className="px-4 py-3 border-b border-zinc-800">
        <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">
          {t('apiKeys.docsTitle')}
        </span>
      </div>
      <div className="p-4 flex flex-col gap-1">
        <p className="text-zinc-600 text-xs mb-3">{t('apiKeys.docsNote')}</p>
        {ENDPOINTS.map(({ method, path, desc }) => (
          <div key={path} className="flex flex-col gap-0.5 py-2 border-b border-zinc-800/50 last:border-0">
            <div className="flex items-center gap-2">
              <span className="text-green-500 text-[11px] font-mono w-10 shrink-0">{method}</span>
              <code className="text-zinc-200 text-xs font-mono">{path}</code>
            </div>
            <p className="text-zinc-600 text-xs pl-12">{desc}</p>
            <details className="pl-12">
              <summary className="text-zinc-700 text-[11px] cursor-pointer hover:text-zinc-500 select-none">
                curl
              </summary>
              <pre className="mt-1 bg-zinc-950 border border-zinc-800 rounded p-2 text-[11px] text-zinc-400 overflow-x-auto whitespace-pre-wrap break-all">
{`curl -H "Authorization: Bearer YOUR_TOKEN" \\
  https://your-domain.com${path}`}
              </pre>
            </details>
          </div>
        ))}
      </div>
    </div>
  )
}
