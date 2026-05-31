import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import Navbar from './Navbar'
import Footer from './Footer'

// ── Sidebar nav structure ─────────────────────────────────────────────────────

const NAV = [
  {
    id: 'getting-started', labelKey: 'docs.navGettingStarted',
    children: [
      { id: 'authentication', labelKey: 'docs.navAuthentication' },
      { id: 'base-url',       labelKey: 'docs.navBaseUrl' },
      { id: 'quick-start',    labelKey: 'docs.navQuickStart' },
    ],
  },
  {
    id: 'api-reference', labelKey: 'docs.navApiReference',
    children: [
      { id: 'ref-space-weather', labelKey: 'docs.navSpaceWeather' },
      { id: 'ref-forecasting',   labelKey: 'docs.navForecasting' },
      { id: 'ref-celestial',     labelKey: 'docs.navCelestial' },
      { id: 'ref-anomalies',     labelKey: 'docs.navAnomalies' },
      { id: 'ref-account',       labelKey: 'docs.navAccount' },
    ],
  },
  { id: 'rate-limits',   labelKey: 'docs.navRateLimits',   children: [] },
  {
    id: 'webhooks', labelKey: 'docs.navWebhooks',
    children: [
      { id: 'webhook-setup',   labelKey: 'docs.navWebhookSetup' },
      { id: 'webhook-payload', labelKey: 'docs.navWebhookPayload' },
      { id: 'webhook-hmac',    labelKey: 'docs.navWebhookHmac' },
      { id: 'webhook-events',  labelKey: 'docs.navWebhookEvents' },
    ],
  },
  { id: 'email-alerts',  labelKey: 'docs.navEmailAlerts',  children: [] },
  { id: 'error-codes',   labelKey: 'docs.navErrorCodes',   children: [] },
  { id: 'glossary',      labelKey: 'docs.navGlossary',     children: [] },
  { id: 'changelog',     labelKey: 'docs.navChangelog',    children: [] },
]

// ── Helpers ───────────────────────────────────────────────────────────────────

function scrollTo(id) {
  document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}

function Badge({ method }) {
  const colors = {
    GET:    'text-green-400 border-green-800 bg-green-950/40',
    POST:   'text-blue-400  border-blue-800  bg-blue-950/40',
    DELETE: 'text-red-400   border-red-800   bg-red-950/40',
  }
  return (
    <span className={`inline-block text-[10px] font-mono border rounded px-1.5 py-0.5 mr-2 ${colors[method] ?? ''}`}>
      {method}
    </span>
  )
}

function Code({ children, lang }) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  const text = typeof children === 'string' ? children : String(children ?? '')
  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch {
      // clipboard API can fail in non-secure contexts; silently no-op
    }
  }
  return (
    <div className="relative my-3 group">
      <pre className="bg-zinc-900 border border-zinc-800 rounded-md p-4 overflow-x-auto text-xs font-mono text-zinc-300 leading-relaxed">
        {lang && <span className="block text-zinc-600 text-[10px] mb-2 uppercase tracking-widest">{lang}</span>}
        <code>{children}</code>
      </pre>
      <button
        type="button"
        onClick={onCopy}
        aria-label={t('docs.copy')}
        className={`absolute top-2 right-2 text-[10px] font-mono uppercase tracking-widest px-2 py-1 rounded border bg-zinc-900/80 backdrop-blur-sm transition-colors ${
          copied
            ? 'border-green-700 text-green-400'
            : 'border-zinc-700 text-zinc-500 hover:text-zinc-200 hover:border-zinc-500 opacity-0 group-hover:opacity-100 focus:opacity-100'
        }`}
      >
        {copied ? t('docs.copied') : t('docs.copy')}
      </button>
    </div>
  )
}

function Ic({ children }) {
  return (
    <code className="bg-zinc-800 text-zinc-200 rounded px-1.5 py-0.5 text-[11px] font-mono">{children}</code>
  )
}

function H2({ id, children }) {
  return (
    <h2
      id={id}
      data-section={id}
      className="text-xl font-semibold text-zinc-100 mt-14 mb-5 scroll-mt-20"
    >
      {children}
    </h2>
  )
}

function H3({ id, children }) {
  return (
    <h3
      id={id}
      data-section={id}
      className="text-sm font-mono uppercase tracking-widest text-zinc-500 mt-10 mb-4 scroll-mt-20"
    >
      {children}
    </h3>
  )
}

function H4({ children }) {
  return <h4 className="text-sm font-medium text-zinc-300 mt-6 mb-2">{children}</h4>
}

function P({ children }) {
  return <p className="text-zinc-400 text-sm leading-relaxed mb-3">{children}</p>
}

function Table({ headers, rows }) {
  return (
    <div className="overflow-x-auto my-4">
      <table className="w-full text-xs font-mono border border-zinc-800 rounded-md">
        <thead>
          <tr className="border-b border-zinc-800 bg-zinc-900">
            {headers.map(h => (
              <th key={h} className="text-left px-4 py-2 text-zinc-500 font-normal">{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, i) => (
            <tr key={i} className="border-b border-zinc-800/50 last:border-0 hover:bg-zinc-900/50">
              {row.map((cell, j) => (
                <td key={j} className="px-4 py-2.5 text-zinc-300 align-top">{cell}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function Endpoint({ method, path, plan, desc, params, response }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return (
    <div className="border border-zinc-800 rounded-md mb-3 overflow-hidden">
      <button
        onClick={() => setOpen(o => !o)}
        className="w-full flex items-start gap-3 px-4 py-3 bg-zinc-900/60 hover:bg-zinc-900 transition-colors text-left"
      >
        <span className="shrink-0 mt-0.5">
          <Badge method={method} />
        </span>
        <span className="font-mono text-zinc-200 text-sm">{path}</span>
        {plan && (
          <span className="ml-auto shrink-0 text-[10px] font-mono text-purple-400 border border-purple-800 rounded px-1.5 py-0.5">
            {plan}+
          </span>
        )}
        <span className={`shrink-0 ml-1 text-zinc-600 transition-transform duration-200 ${open ? 'rotate-180' : ''}`}>▾</span>
      </button>
      {open && (
        <div className="px-4 pb-4 pt-2 border-t border-zinc-800 bg-zinc-950/60">
          <p className="text-zinc-400 text-sm mb-3">{desc}</p>
          {params && (
            <>
              <p className="text-zinc-500 text-[11px] font-mono uppercase tracking-widest mb-2">{t('docs.paramLabel')}</p>
              <Table headers={[t('docs.thName'), t('docs.thType'), t('docs.thDesc')]} rows={params} />
            </>
          )}
          {response && (
            <>
              <p className="text-zinc-500 text-[11px] font-mono uppercase tracking-widest mb-1">{t('docs.responseLabel')}</p>
              <Code lang="json">{response}</Code>
            </>
          )}
        </div>
      )}
    </div>
  )
}

// ── Sidebar ───────────────────────────────────────────────────────────────────

function Sidebar({ active, t }) {
  return (
    <aside className="hidden lg:block w-52 shrink-0 sticky top-16 h-[calc(100vh-4rem)] overflow-y-auto py-8 pr-2 border-r border-zinc-800/60">
      <nav className="flex flex-col gap-0.5">
        {NAV.map(section => (
          <div key={section.id}>
            <button
              onClick={() => scrollTo(section.id)}
              className={`w-full text-left px-3 py-1.5 text-xs font-mono rounded transition-colors ${
                active === section.id
                  ? 'text-zinc-100 bg-zinc-800/60'
                  : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-900'
              }`}
            >
              {t(section.labelKey)}
            </button>
            {section.children.map(child => (
              <button
                key={child.id}
                onClick={() => scrollTo(child.id)}
                className={`w-full text-left pl-6 pr-3 py-1 text-[11px] font-mono rounded transition-colors ${
                  active === child.id
                    ? 'text-zinc-200'
                    : 'text-zinc-600 hover:text-zinc-400'
                }`}
              >
                {t(child.labelKey)}
              </button>
            ))}
          </div>
        ))}
      </nav>
    </aside>
  )
}

// ── Main component ────────────────────────────────────────────────────────────

export default function DocsPage({ onSignIn }) {
  const { t } = useTranslation()
  const [active, setActive] = useState('getting-started')

  useEffect(() => {
    const allIds = NAV.flatMap(s => [s.id, ...s.children.map(c => c.id)])
    function onScroll() {
      for (const id of [...allIds].reverse()) {
        const el = document.getElementById(id)
        if (el && el.getBoundingClientRect().top <= 100) {
          setActive(id)
          return
        }
      }
      setActive(allIds[0])
    }
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => window.removeEventListener('scroll', onScroll)
  }, [])

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />

      <div className="flex pt-16 max-w-7xl mx-auto">
        <Sidebar active={active} t={t} />

        {/* ── Content ───────────────────────────────────────────────────── */}
        <main className="flex-1 min-w-0 px-8 lg:px-12 py-10 max-w-4xl">

          {/* ── Getting Started ──────────────────────────────────────────── */}
          <H2 id="getting-started">{t('docs.navGettingStarted')}</H2>

          <H3 id="authentication">{t('docs.navAuthentication')}</H3>
          <P>{t('docs.authIntro')}</P>

          <H4>{t('docs.authJwtTitle')}</H4>
          <P>{t('docs.authJwtDesc')}</P>
          <Code lang="http">{`Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...`}</Code>

          <H4>{t('docs.authApiKeyTitle')}</H4>
          <P>{t('docs.authApiKeyDesc')}</P>
          <Code lang="http">{`Authorization: Bearer ast_a3f2c8e1d4b7...`}</Code>

          <P>{t('docs.authPublicNote')}</P>

          <H3 id="base-url">{t('docs.navBaseUrl')}</H3>
          <P>{t('docs.baseUrlIntro')}</P>
          <Code>{`https://your-domain.com`}</Code>
          <P>{t('docs.baseUrlDevNote')}</P>

          <H3 id="quick-start">{t('docs.navQuickStart')}</H3>
          <P>{t('docs.quickStartIntro')}</P>
          <Code lang="bash">{`# 1. Authenticate and get a token
curl -X POST https://your-domain.com/auth/login \\
  -H "Content-Type: application/json" \\
  -d '{"email":"you@example.com","password":"••••••••"}'

# Response:
# { "token": "eyJhbGci..." }

# 2. Query the Kp endpoint
curl -H "Authorization: Bearer eyJhbGci..." \\
  https://your-domain.com/api/kp`}</Code>

          <P>{t('docs.quickStartApiKeyNote')}</P>
          <Code lang="bash">{`curl -H "Authorization: Bearer ast_a3f2c8e1..." \\
  https://your-domain.com/api/kp`}</Code>

          {/* ── API Reference ────────────────────────────────────────────── */}
          <H2 id="api-reference">{t('docs.navApiReference')}</H2>
          <P>{t('docs.apiRefIntro')}</P>

          <H3 id="ref-space-weather">{t('docs.navSpaceWeather')}</H3>

          <Endpoint
            method="GET" path="/api/kp"
            desc={t('docs.epKp')}
            response={`[
  {
    "time_tag": "2026-05-02T18:00:00Z",
    "kp_index": 3,
    "estimated_kp": 3.33
  },
  ...
]`}
          />

          <Endpoint
            method="GET" path="/api/kp-3h"
            desc={t('docs.epKp3h')}
            response={`[
  {
    "time_tag": "2026-05-02T15:00:00Z",
    "kp":       2.67,
    "kp_index": 2
  },
  ...
]`}
          />

          <Endpoint
            method="GET" path="/api/solar-wind"
            desc={t('docs.epSolarWind')}
            response={`[
  {
    "time_tag":    "2026-05-02T18:01:00Z",
    "speed_km_s":  412.3,
    "density":     4.21,
    "temperature": 87450
  },
  ...
]`}
          />

          <Endpoint
            method="GET" path="/api/xray"
            desc={t('docs.epXray')}
            response={`[
  {
    "time_tag":  "2026-05-02T18:00:00Z",
    "energy":    "0.1-0.8nm",
    "satellite": 18,
    "flux":      1.23e-07
  },
  ...
]`}
          />

          <Endpoint
            method="GET" path="/api/imf"
            desc={t('docs.epImf')}
            response={`[
  {
    "time_tag": "2026-05-02T18:01:00Z",
    "bz_nT":   -4.12,
    "bt_nT":    7.83
  },
  ...
]`}
          />

          <Endpoint
            method="GET" path="/api/dst"
            desc={t('docs.epDst')}
            response={`[
  {
    "time_tag": "2026-05-02T18:00:00Z",
    "dst_nt":   -28
  },
  ...
]`}
          />

          <Endpoint
            method="GET" path="/api/alerts"
            desc={t('docs.epAlerts')}
            response={`[
  {
    "product_id":     "WATA20",
    "issue_datetime": "2026-05-02T16:12:00Z",
    "message":        "Space Weather Message Code: WATA20\n..."
  },
  ...
]`}
          />

          <H3 id="ref-forecasting">{t('docs.navForecasting')}</H3>

          <Endpoint
            method="GET" path="/api/kp-forecast"
            plan="developer"
            desc={t('docs.epForecast')}
            response={`{
  "predicted_kp": 4.2,
  "ci_lower":     3.1,
  "ci_upper":     5.4,
  "uncertainty":  0.62,
  "model_version": "kp_lstm_v1",
  "trained_date":  "2025-11-01"
}`}
          />

          <H3 id="ref-celestial">{t('docs.navCelestial')}</H3>

          <Endpoint
            method="GET" path="/api/iss"
            desc={t('docs.epIss')}
            response={`{
  "latitude":   51.6,
  "longitude": -12.3,
  "altitude_km":  418.2,
  "velocity_km_h": 27580
}`}
          />

          <Endpoint
            method="GET" path="/api/apod"
            desc={t('docs.epApod')}
            response={`{
  "date":        "2026-05-02",
  "title":       "The Pillars of Creation",
  "explanation": "...",
  "url":         "https://apod.nasa.gov/apod/image/...",
  "hdurl":       "https://apod.nasa.gov/apod/image/...",
  "media_type":  "image"
}`}
          />

          <Endpoint
            method="GET" path="/api/neo"
            desc={t('docs.epNeo')}
            response={`[
  {
    "id":                  "3542519",
    "name":                "(2010 PK9)",
    "close_approach_date": "2026-05-04",
    "is_hazardous":        false,
    "diameter_min_km":     0.12,
    "diameter_max_km":     0.27,
    "velocity_km_h":       42300,
    "miss_distance_km":    1842000,
    "miss_distance_ld":    4.79
  },
  ...
]`}
          />

          <Endpoint
            method="GET" path="/api/epic"
            desc={t('docs.epEpic')}
            response={`[
  {
    "identifier":    "20260502003412",
    "caption":       "This image was taken by the EPIC camera...",
    "image":         "epic_1b_20260502003412",
    "date":          "2026-05-02 00:34:12",
    "centroid_lat":  -2.14,
    "centroid_lon": 154.72
  },
  ...
]`}
          />

          <Endpoint
            method="GET" path="/api/exoplanets"
            desc={t('docs.epExoplanets')}
            response={`[
  {
    "pl_name":        "55 Cnc e",
    "hostname":       "55 Cnc",
    "orbital_period_days": 0.7365,
    "radius_earth":   1.92,
    "mass_earth":     8.08,
    "disc_year":      2004
  },
  ...
]`}
          />

          <Endpoint
            method="GET" path="/api/starlink"
            desc={t('docs.epStarlink')}
            response={`[
  {
    "norad_id": 44713,
    "name":     "STARLINK-1007",
    "tle_line1": "1 44713U 19074A   26...",
    "tle_line2": "2 44713  53.0..."
  },
  ...
]`}
          />

          <H3 id="ref-anomalies">{t('docs.navAnomalies')}</H3>

          <Endpoint
            method="GET" path="/api/anomalies"
            plan="developer"
            desc={t('docs.epAnomalies')}
            response={`[
  {
    "anomaly_type": "kp_storm",
    "source_ref":   "2026-05-02T18:00:00Z",
    "severity":     "warning",
    "message":      "Kp index 5.3 - geomagnetic storm in progress",
    "detected_at":  1746208800
  },
  ...
]`}
          />

          <H3 id="ref-account">{t('docs.navAccount')}</H3>

          <Endpoint
            method="GET" path="/api/reports/summary"
            params={[['range', 'string', '24h (default), 7d, or 30d']]}
            desc={t('docs.epSummary')}
            response={`{
  "range": "24h",
  "avg_kp":           2.8,
  "max_kp":           4.3,
  "max_solar_wind":   512,
  "max_xray_class":   "M1.2",
  "anomaly_count":    3,
  "asteroid_count":   7
}`}
          />

          <Endpoint
            method="GET" path="/api/reports/export"
            plan="developer"
            params={[['range', 'string', '24h (default), 7d, or 30d']]}
            desc={t('docs.epExport')}
            response={`time_tag,kp,solar_wind_km_s,xray_flux
2026-05-02T00:00:00Z,2.33,423.1,3.2e-08
2026-05-02T00:01:00Z,2.33,421.8,3.1e-08
...`}
          />

          {/* ── Rate Limits ──────────────────────────────────────────────── */}
          <H2 id="rate-limits">{t('docs.navRateLimits')}</H2>
          <P>{t('docs.rateLimitsIntro')}</P>

          <Table
            headers={[t('docs.thPlan'), t('docs.thLimit'), t('docs.thWindow'), t('docs.thReset')]}
            rows={[
              ['Free',       '100 requests',         'Daily',   'Midnight UTC'],
              ['Starter',    '100 requests',         'Daily',   'Midnight UTC'],
              ['Developer',  '10,000 requests',      'Monthly', '1st of month UTC'],
              ['Pro',        '100,000 requests',     'Monthly', '1st of month UTC'],
              ['Business',   '1,000,000 requests',   'Monthly', '1st of month UTC'],
              ['Enterprise', 'Unlimited',            '-',       '-'],
            ]}
          />

          <H4>{t('docs.rateLimitsWhatTitle')}</H4>
          <P>{t('docs.rateLimitsWhatDesc')}</P>

          <H4>{t('docs.rateLimitsHeaderTitle')}</H4>
          <P>{t('docs.rateLimitsHeaderDesc')}</P>
          <Code lang="json">{`{
  "error":        "rate_limit_exceeded",
  "limit":        10000,
  "used":         10000,
  "period_start": 1746057600,
  "period_end":   1748736000
}`}</Code>

          <P>{t('docs.rateLimitsMonitorNote')}</P>
          <Code lang="bash">{`curl -H "Authorization: Bearer ast_..." \\
  https://your-domain.com/api/usage`}</Code>

          {/* ── Webhooks ─────────────────────────────────────────────────── */}
          <H2 id="webhooks">{t('docs.navWebhooks')}</H2>
          <P>{t('docs.webhooksIntro')}</P>

          <H3 id="webhook-setup">{t('docs.navWebhookSetup')}</H3>
          <P>{t('docs.webhookSetupDesc')}</P>
          <Code lang="bash">{`curl -X POST https://your-domain.com/api/webhooks \\
  -H "Authorization: Bearer <token>" \\
  -H "Content-Type: application/json" \\
  -d '{
    "url":    "https://your-server.com/webhook",
    "events": ["kp_storm", "xray_flare"]
  }'`}</Code>

          <P>{t('docs.webhookSecretNote')}</P>
          <Code lang="json">{`{
  "id":     "a3f2c8e1d4b79012",
  "secret": "f1e2d3c4b5a69078...",
  "events": ["kp_storm", "xray_flare"]
}`}</Code>

          <H3 id="webhook-payload">{t('docs.navWebhookPayload')}</H3>
          <P>{t('docs.webhookPayloadNote')}</P>
          <Code lang="json">{`{
  "event":     "kp_storm",
  "severity":  "warning",
  "message":   "Kp index 5.3 - geomagnetic storm in progress",
  "timestamp": 1746208800,
  "data": {
    "source_ref": "2026-05-02T18:00:00Z"
  }
}`}</Code>

          <P>{t('docs.webhookHeadersNote')}</P>
          <Table
            headers={[t('docs.thHeader'), t('docs.thValue')]}
            rows={[
              ['Content-Type',          'application/json'],
              ['X-Astraeus-Signature',  'sha256=<hex-hmac>'],
              ['X-Astraeus-Event',      'kp_storm (the event name)'],
            ]}
          />

          <H3 id="webhook-hmac">{t('docs.navWebhookHmac')}</H3>
          <P>{t('docs.webhookHmacDesc')}</P>

          <Code lang="javascript">{`// Node.js verification example
const crypto = require('crypto')

function verifyWebhook(rawBody, signatureHeader, secret) {
  const expected = 'sha256=' + crypto
    .createHmac('sha256', secret)
    .update(rawBody) // rawBody must be the raw Buffer, not parsed JSON
    .digest('hex')

  // Use timingSafeEqual to prevent timing attacks
  const sigBuf = Buffer.from(signatureHeader)
  const expBuf = Buffer.from(expected)

  if (sigBuf.length !== expBuf.length) return false
  return crypto.timingSafeEqual(sigBuf, expBuf)
}

// Express example
app.post('/webhook', express.raw({ type: 'application/json' }), (req, res) => {
  const sig = req.headers['x-astraeus-signature']
  if (!verifyWebhook(req.body, sig, process.env.WEBHOOK_SECRET)) {
    return res.status(401).send('Invalid signature')
  }
  const event = JSON.parse(req.body)
  console.log('Received:', event.event, event.severity)
  res.status(200).send('OK')
})`}</Code>

          <Code lang="python">{`# Python verification example
import hmac, hashlib

def verify_webhook(raw_body: bytes, signature: str, secret: str) -> bool:
    expected = 'sha256=' + hmac.new(
        secret.encode(),
        raw_body,
        hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(signature, expected)`}</Code>

          <Code lang="go">{`// Go verification example
package main

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
)

func verifyWebhook(rawBody []byte, signature, secret string) bool {
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write(rawBody)
	expected := "sha256=" + hex.EncodeToString(mac.Sum(nil))
	return hmac.Equal([]byte(signature), []byte(expected))
}`}</Code>

          <P><strong>Common pitfall:</strong> verify the signature against the <em>raw, unparsed</em> request body. Frameworks that auto-decode JSON (Express, Flask, etc.) modify whitespace or key ordering when re-serialising, which breaks the HMAC. Capture the raw bytes before any parser touches them, then verify, then parse.</P>

          <H3 id="webhook-events">{t('docs.navWebhookEvents')}</H3>
          <Table
            headers={[t('docs.thEvent'), t('docs.thTrigger'), t('docs.thSeverity')]}
            rows={[
              ['kp_storm',          'Kp index ≥ 5.0 (G1 storm)',                'warning / critical (≥8.0)'],
              ['solar_wind_speed',  'Solar wind > 700 km/s',                    'warning / critical (>900 km/s)'],
              ['xray_flare',        'X-ray flux ≥ M-class (1×10⁻⁵ W/m²)',      'warning / critical (X-class)'],
              ['asteroid_close',    'NEO passing within 1 Lunar Distance',       'warning / critical (<0.5 LD)'],
              ['ml_forecast_storm', 'ML model forecasts Kp ≥ 5.0 within 3 hrs', 'warning / critical (≥8.0)'],
            ]}
          />

          <P>{t('docs.webhookTimeoutNote')}</P>

          {/* ── Email Alerts ─────────────────────────────────────────────── */}
          <H2 id="email-alerts">{t('docs.navEmailAlerts')}</H2>
          <P>{t('docs.emailAlertsIntro')}</P>

          <H4>{t('docs.emailAlertsDashTitle')}</H4>
          <P>{t('docs.emailAlertsDashDesc')}</P>

          <H4>{t('docs.emailAlertsApiTitle')}</H4>
          <Code lang="bash">{`# Get current settings
curl -H "Authorization: Bearer <token>" \\
  https://your-domain.com/api/email-alerts

# Update settings
curl -X POST https://your-domain.com/api/email-alerts \\
  -H "Authorization: Bearer <token>" \\
  -H "Content-Type: application/json" \\
  -d '{
    "enabled":        true,
    "kp_threshold":   5.0,
    "wind_threshold": 700
  }'`}</Code>

          <H4>{t('docs.emailAlertsThresholdTitle')}</H4>
          <Table
            headers={[t('docs.thParameter'), t('docs.thUnit'), t('docs.thRange'), t('docs.thDefault'), t('docs.thDesc')]}
            rows={[
              ['kp_threshold',   'Kp',   '1.0 – 9.0',   '5.0', 'Alert when Kp reaches or exceeds this value'],
              ['wind_threshold',  'km/s', '100 – 2000', '700',  'Alert when solar wind speed reaches or exceeds this value'],
            ]}
          />

          <H4>{t('docs.emailAlertsFormatTitle')}</H4>
          <P>{t('docs.emailAlertsFormatDesc')}</P>

          {/* ── Error Codes ──────────────────────────────────────────────── */}
          <H2 id="error-codes">{t('docs.navErrorCodes')}</H2>
          <P>{t('docs.errorCodesIntro')}</P>
          <Code lang="json">{`{ "error": "description of the error" }`}</Code>

          <Table
            headers={[t('docs.thStatus'), t('docs.thCode'), t('docs.thCause'), t('docs.thResolution')]}
            rows={[
              [
                '401',
                'Unauthorized',
                'Missing, malformed, or expired Authorization header',
                'Re-authenticate via POST /auth/login to get a fresh JWT token',
              ],
              [
                '403',
                'Forbidden / plan_required',
                'Your plan does not include the requested endpoint or feature',
                'Upgrade your plan. The response includes required_plan and your_plan fields',
              ],
              [
                '422',
                'Unprocessable Entity',
                'Request body failed validation (e.g. missing url for webhook, empty key name)',
                'Check the error field for specific validation message',
              ],
              [
                '429',
                'Too Many Requests',
                'API key quota exhausted for the current billing period',
                'Wait until period_end (Unix timestamp in response) or upgrade your plan',
              ],
              [
                '500',
                'Internal Server Error',
                'Unexpected backend error (database, external API timeout, etc.)',
                'Retry after a few seconds. If persistent, check status or contact support',
              ],
            ]}
          />

          <H4>403 plan_required response</H4>
          <Code lang="json">{`{
  "error":         "plan_required",
  "required_plan": "developer",
  "your_plan":     "starter"
}`}</Code>

          <H4>429 rate_limit_exceeded response</H4>
          <Code lang="json">{`{
  "error":    "rate_limit_exceeded",
  "plan":     "free",
  "limit":    100,
  "reset_at": 1746144000
}`}</Code>

          {/* ── API Changelog ────────────────────────────────────────────── */}
          <H2 id="changelog">{t('docs.navChangelog')}</H2>
          <P>{t('docs.changelogIntro')}</P>
          <dl className="flex flex-col gap-5 mt-6">
            {t('docs.changelog', { returnObjects: true }).map((entry, i) => (
              <div key={i} className="border-l-2 border-zinc-800 pl-4">
                <dt className="flex items-baseline gap-3 flex-wrap mb-1.5">
                  <span className="text-sm font-mono font-semibold text-zinc-100">{entry.version}</span>
                  <span className="text-xs font-mono text-zinc-500">{entry.date}</span>
                  {entry.url && (
                    <a
                      href={entry.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-xs font-mono text-zinc-500 hover:text-zinc-200 transition-colors"
                    >
                      release notes ↗
                    </a>
                  )}
                </dt>
                <dd className="text-zinc-400 text-sm leading-relaxed">{entry.summary}</dd>
              </div>
            ))}
          </dl>
          <P className="mt-6 text-zinc-500 text-xs">
            {t('docs.changelogFooter')}{' '}
            <a
              href="https://github.com/ChronoCoders/astraeusio/releases"
              target="_blank"
              rel="noopener noreferrer"
              className="text-zinc-300 hover:text-white underline underline-offset-2"
            >
              GitHub Releases
            </a>.
          </P>

          {/* ── Glossary ─────────────────────────────────────────────────── */}
          <H2 id="glossary">{t('docs.navGlossary')}</H2>
          <P>{t('docs.glossaryIntro')}</P>
          <dl className="flex flex-col gap-6 mt-6">
            {t('docs.glossary', { returnObjects: true }).map((item, i) => (
              <div key={i}>
                <dt className="text-sm font-mono uppercase tracking-widest text-zinc-300 mb-1.5">
                  {item.term}
                </dt>
                <dd className="text-zinc-400 text-sm leading-relaxed">{item.body}</dd>
              </div>
            ))}
          </dl>

          {/* bottom padding */}
          <div className="h-24" />
        </main>
      </div>
      <Footer />
    </div>
  )
}
