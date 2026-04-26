import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import UpgradePrompt from './UpgradePrompt'

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
  // Starter plan is locked.
  if (plan === 'starter') {
    return (
      <div className="flex flex-col gap-6 max-w-4xl">
        <div className="bg-zinc-900 border border-zinc-800 rounded">
          <UpgradePrompt messageKey="plan.lockedApiKeys" requiredPlan="pro" />
        </div>
        <WebhooksCard />
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

      <WebhooksCard />
      <ApiDocsSection t={t} />

    </div>
  )
}

// ── Webhooks placeholder ───────────────────────────────────────────────────────

function WebhooksCard() {
  const { t } = useTranslation()
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-5 flex items-center justify-between gap-4">
      <div className="flex flex-col gap-1">
        <span className="text-zinc-400 text-xs font-mono uppercase tracking-widest">Webhooks</span>
        <p className="text-zinc-600 text-xs">{t('plan.lockedWebhooks')}</p>
      </div>
      <span className="shrink-0 text-[10px] font-mono border border-purple-800 text-purple-400 rounded px-2 py-0.5">
        {t('plan.comingSoon')}
      </span>
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
