import { useState, useEffect } from 'react'
import Navbar from './Navbar'

// ── Sidebar nav structure ─────────────────────────────────────────────────────

const NAV = [
  {
    id: 'getting-started', label: 'Getting Started',
    children: [
      { id: 'authentication', label: 'Authentication' },
      { id: 'base-url',       label: 'Base URL' },
      { id: 'quick-start',    label: 'Quick Start' },
    ],
  },
  {
    id: 'api-reference', label: 'API Reference',
    children: [
      { id: 'ref-space-weather', label: 'Space Weather' },
      { id: 'ref-forecasting',   label: 'Forecasting' },
      { id: 'ref-celestial',     label: 'Celestial' },
      { id: 'ref-anomalies',     label: 'Anomalies & Alerts' },
      { id: 'ref-account',       label: 'Account' },
    ],
  },
  { id: 'rate-limits',   label: 'Rate Limits',   children: [] },
  {
    id: 'webhooks', label: 'Webhooks',
    children: [
      { id: 'webhook-setup',   label: 'Setup' },
      { id: 'webhook-payload', label: 'Payload Format' },
      { id: 'webhook-hmac',    label: 'HMAC Verification' },
      { id: 'webhook-events',  label: 'Event Types' },
    ],
  },
  { id: 'email-alerts',  label: 'Email Alerts',  children: [] },
  { id: 'error-codes',   label: 'Error Codes',   children: [] },
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
  return (
    <pre className="bg-zinc-900 border border-zinc-800 rounded-md p-4 overflow-x-auto text-xs font-mono text-zinc-300 leading-relaxed my-3">
      {lang && <span className="block text-zinc-600 text-[10px] mb-2 uppercase tracking-widest">{lang}</span>}
      <code>{children}</code>
    </pre>
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
              <p className="text-zinc-500 text-[11px] font-mono uppercase tracking-widest mb-2">Parameters</p>
              <Table headers={['Name', 'Type', 'Description']} rows={params} />
            </>
          )}
          {response && (
            <>
              <p className="text-zinc-500 text-[11px] font-mono uppercase tracking-widest mb-1">Example Response</p>
              <Code lang="json">{response}</Code>
            </>
          )}
        </div>
      )}
    </div>
  )
}

// ── Sidebar ───────────────────────────────────────────────────────────────────

function Sidebar({ active }) {
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
              {section.label}
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
                {child.label}
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
        <Sidebar active={active} />

        {/* ── Content ───────────────────────────────────────────────────── */}
        <main className="flex-1 min-w-0 px-8 lg:px-12 py-10 max-w-4xl">

          {/* ── Getting Started ──────────────────────────────────────────── */}
          <H2 id="getting-started">Getting Started</H2>

          <H3 id="authentication">Authentication</H3>
          <P>
            All API endpoints (except public ones) require authentication. Astraeusio supports
            two authentication methods.
          </P>

          <H4>JWT Bearer Token</H4>
          <P>
            Obtained via <Ic>POST /auth/login</Ic>. Valid for 24 hours. Use for dashboard access
            and personal scripts. JWT requests do <strong className="text-zinc-300">not</strong> consume your API quota.
          </P>
          <Code lang="http">{`Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...`}</Code>

          <H4>API Key</H4>
          <P>
            Generated from the API Keys page in your dashboard. Prefixed with <Ic>ast_</Ic>.
            API key requests count against your plan quota. Suitable for server-to-server integrations.
          </P>
          <Code lang="http">{`Authorization: Bearer ast_a3f2c8e1d4b7...`}</Code>

          <P>
            Public endpoints (<Ic>/api/public/*</Ic>) require no authentication and are not
            rate-limited — they return single latest readings for live widgets.
          </P>

          <H3 id="base-url">Base URL</H3>
          <P>All API requests are made to:</P>
          <Code>{`https://your-domain.com`}</Code>
          <P>During local development the server runs on <Ic>http://localhost:3000</Ic>.</P>

          <H3 id="quick-start">Quick Start</H3>
          <P>Fetch the latest Kp index using curl with a JWT token:</P>
          <Code lang="bash">{`# 1. Authenticate and get a token
curl -X POST https://your-domain.com/auth/login \\
  -H "Content-Type: application/json" \\
  -d '{"email":"you@example.com","password":"••••••••"}'

# Response:
# { "token": "eyJhbGci..." }

# 2. Query the Kp endpoint
curl -H "Authorization: Bearer eyJhbGci..." \\
  https://your-domain.com/api/kp`}</Code>

          <P>Using an API key instead:</P>
          <Code lang="bash">{`curl -H "Authorization: Bearer ast_a3f2c8e1..." \\
  https://your-domain.com/api/kp`}</Code>

          {/* ── API Reference ────────────────────────────────────────────── */}
          <H2 id="api-reference">API Reference</H2>
          <P>
            All data endpoints return JSON. Timestamps in <Ic>time_tag</Ic> fields are ISO-8601 UTC strings.
            Numeric values use scaled integers to avoid floating-point storage — see field descriptions below.
          </P>

          <H3 id="ref-space-weather">Space Weather</H3>

          <Endpoint
            method="GET" path="/api/kp"
            desc="Returns the last 24 hours of 1-minute Kp index readings from NOAA SWPC. kp_index is the integer class; estimated_kp is the precise value (÷100 = Kp)."
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
            desc="Official NOAA 3-hour Kp values for the last 7 days. These are the definitive planetary Kp values, revised as new data arrives."
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
            desc="1-minute real-time solar wind measurements from the NOAA DSCOVR/ACE spacecraft at L1. Speed in km/s, density in p/cm³, temperature in K."
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
            desc="GOES X-ray flux from the primary satellite, 2-minute cadence. Two energy channels: 0.1–0.8 nm (long) and 0.05–0.4 nm (short). Flux in W/m²."
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
            desc="Interplanetary Magnetic Field measurements from DSCOVR. Bz is the north-south component (negative = southward = geoeffective). Values in nT."
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
            desc="Dst (Disturbance Storm Time) index from the Kyoto World Data Center via NOAA. Measures ring current strength. Values in nT; negative = geomagnetic storm."
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
            desc="NOAA SWPC space weather alerts, watches, and warnings. Returns all active products from the last 7 days."
            response={`[
  {
    "product_id":     "WATA20",
    "issue_datetime": "2026-05-02T16:12:00Z",
    "message":        "Space Weather Message Code: WATA20\n..."
  },
  ...
]`}
          />

          <H3 id="ref-forecasting">Forecasting</H3>

          <Endpoint
            method="GET" path="/api/kp-forecast"
            plan="developer"
            desc="ML-generated Kp forecast for the next 3 hours. Uses an LSTM neural network with Monte Carlo Dropout for uncertainty estimation (50 inference passes). Returns predicted Kp with 95% confidence interval."
            response={`{
  "predicted_kp": 4.2,
  "ci_lower":     3.1,
  "ci_upper":     5.4,
  "uncertainty":  0.62,
  "model_version": "kp_lstm_v1",
  "trained_date":  "2025-11-01"
}`}
          />

          <H3 id="ref-celestial">Celestial</H3>

          <Endpoint
            method="GET" path="/api/iss"
            desc="Current ISS position from Open Notify. Updated every 5 seconds. Altitude in km, velocity in km/h."
            response={`{
  "latitude":   51.6,
  "longitude": -12.3,
  "altitude_km":  418.2,
  "velocity_km_h": 27580
}`}
          />

          <Endpoint
            method="GET" path="/api/apod"
            desc="NASA Astronomy Picture of the Day. Returns the current day's image or video with title and explanation."
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
            desc="Near-Earth objects from NASA NeoWs for the current 7-day window. Includes close approach distance (in lunar distances and km), diameter, velocity, and hazard status."
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
            desc="Latest NASA EPIC (Earth Polychromatic Imaging Camera) images from DSCOVR. Returns up to 20 most recent frames with centroid coordinates."
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
            desc="Top 100 confirmed exoplanets from the NASA Exoplanet Archive, ordered by shortest orbital period. Includes host star, orbital period, radius, and mass."
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
            desc="Full Starlink constellation TLE data from Celestrak. Returns NORAD catalog ID, name, and two TLE lines for orbital propagation. Updated hourly."
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

          <H3 id="ref-anomalies">Anomalies & Alerts</H3>

          <Endpoint
            method="GET" path="/api/anomalies"
            plan="developer"
            desc="Locally detected space weather anomalies. Five checks run every 60 seconds: Kp storm (≥G1/G4), solar wind speed (>700/>900 km/s), X-ray flare (M/X class), asteroid close approach (<1 LD), and ML storm forecast."
            response={`[
  {
    "anomaly_type": "kp_storm",
    "source_ref":   "2026-05-02T18:00:00Z",
    "severity":     "warning",
    "message":      "Kp index 5.3 — geomagnetic storm in progress",
    "detected_at":  1746208800
  },
  ...
]`}
          />

          <H3 id="ref-account">Account</H3>

          <Endpoint
            method="GET" path="/api/reports/summary"
            params={[['range', 'string', '24h (default), 7d, or 30d']]}
            desc="Aggregated space weather statistics for the selected time range. Includes average and peak Kp, peak solar wind, X-ray class summary, anomaly count, and asteroid approaches."
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
            desc="CSV export of raw Kp, solar wind, and X-ray readings for the selected time range. Returns Content-Type: text/csv with a filename attachment header."
            response={`time_tag,kp,solar_wind_km_s,xray_flux
2026-05-02T00:00:00Z,2.33,423.1,3.2e-08
2026-05-02T00:01:00Z,2.33,421.8,3.1e-08
...`}
          />

          {/* ── Rate Limits ──────────────────────────────────────────────── */}
          <H2 id="rate-limits">Rate Limits</H2>
          <P>
            Rate limits apply only to requests authenticated with an <strong className="text-zinc-300">API key</strong> (<Ic>ast_</Ic> prefix).
            JWT dashboard requests are unmetered and do not consume quota.
          </P>

          <Table
            headers={['Plan', 'Limit', 'Window', 'Reset']}
            rows={[
              ['Free',       '100 requests',         'Daily',   'Midnight UTC'],
              ['Starter',    '100 requests',         'Daily',   'Midnight UTC'],
              ['Developer',  '10,000 requests',      'Monthly', '1st of month UTC'],
              ['Pro',        '100,000 requests',     'Monthly', '1st of month UTC'],
              ['Business',   '1,000,000 requests',   'Monthly', '1st of month UTC'],
              ['Enterprise', 'Unlimited',            '—',       '—'],
            ]}
          />

          <H4>What counts as a request?</H4>
          <P>
            Every successful API call authenticated with an <Ic>ast_</Ic> API key increments your counter.
            Requests that return <Ic>401</Ic> or <Ic>403</Ic> before reaching the route handler do not count.
            The <Ic>/api/public/*</Ic> endpoints are always free and unmetered.
          </P>

          <H4>Rate limit headers</H4>
          <P>When a limit is exceeded the server returns <Ic>429 Too Many Requests</Ic>:</P>
          <Code lang="json">{`{
  "error":        "rate_limit_exceeded",
  "limit":        10000,
  "used":         10000,
  "period_start": 1746057600,
  "period_end":   1748736000
}`}</Code>

          <P>Monitor your usage at any time:</P>
          <Code lang="bash">{`curl -H "Authorization: Bearer ast_..." \\
  https://your-domain.com/api/usage`}</Code>

          {/* ── Webhooks ─────────────────────────────────────────────────── */}
          <H2 id="webhooks">Webhooks</H2>
          <P>
            Webhooks let you receive real-time HTTP POST notifications when space weather anomalies
            are detected. Requires a <strong className="text-zinc-300">Pro</strong> plan or higher.
          </P>

          <H3 id="webhook-setup">Setup</H3>
          <P>Register a webhook endpoint from the API Keys page in your dashboard, or via the API:</P>
          <Code lang="bash">{`curl -X POST https://your-domain.com/api/webhooks \\
  -H "Authorization: Bearer <token>" \\
  -H "Content-Type: application/json" \\
  -d '{
    "url":    "https://your-server.com/webhook",
    "events": ["kp_storm", "xray_flare"]
  }'`}</Code>

          <P>Response includes your signing secret — <strong className="text-red-400">save it immediately</strong>, it is shown only once:</P>
          <Code lang="json">{`{
  "id":     "a3f2c8e1d4b79012",
  "secret": "f1e2d3c4b5a69078...",
  "events": ["kp_storm", "xray_flare"]
}`}</Code>

          <H3 id="webhook-payload">Payload Format</H3>
          <P>Astraeusio sends a JSON POST to your endpoint with the following structure:</P>
          <Code lang="json">{`{
  "event":     "kp_storm",
  "severity":  "warning",
  "message":   "Kp index 5.3 — geomagnetic storm in progress",
  "timestamp": 1746208800,
  "data": {
    "source_ref": "2026-05-02T18:00:00Z"
  }
}`}</Code>

          <P>Headers sent with each delivery:</P>
          <Table
            headers={['Header', 'Value']}
            rows={[
              ['Content-Type',          'application/json'],
              ['X-Astraeus-Signature',  'sha256=<hex-hmac>'],
              ['X-Astraeus-Event',      'kp_storm (the event name)'],
            ]}
          />

          <H3 id="webhook-hmac">HMAC Verification</H3>
          <P>
            Every delivery is signed with HMAC-SHA256 using your webhook secret.
            Always verify the signature before processing the payload.
          </P>

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

          <H3 id="webhook-events">Event Types</H3>
          <Table
            headers={['Event', 'Trigger', 'Severity']}
            rows={[
              ['kp_storm',          'Kp index ≥ 5.0 (G1 storm)',                'warning / critical (≥8.0)'],
              ['solar_wind_speed',  'Solar wind > 700 km/s',                    'warning / critical (>900 km/s)'],
              ['xray_flare',        'X-ray flux ≥ M-class (1×10⁻⁵ W/m²)',      'warning / critical (X-class)'],
              ['asteroid_close',    'NEO passing within 1 Lunar Distance',       'warning / critical (<0.5 LD)'],
              ['ml_forecast_storm', 'ML model forecasts Kp ≥ 5.0 within 3 hrs', 'warning / critical (≥8.0)'],
            ]}
          />

          <P>
            Webhook deliveries use a 5-second timeout. Failed deliveries are not retried automatically.
            You can re-register the endpoint if needed.
          </P>

          {/* ── Email Alerts ─────────────────────────────────────────────── */}
          <H2 id="email-alerts">Email Alerts</H2>
          <P>
            Email alerts send you a notification when live space weather readings exceed your personal
            thresholds. Requires a <strong className="text-zinc-300">Developer</strong> plan or higher.
            Alerts are throttled to at most <strong className="text-zinc-300">one email per hour</strong> regardless
            of how many thresholds are exceeded simultaneously.
          </P>

          <H4>Configure via Dashboard</H4>
          <P>
            Go to <strong className="text-zinc-300">API Keys → Email Alerts</strong> in your dashboard.
            Enable the toggle, set your Kp and solar wind thresholds, then click Save.
          </P>

          <H4>Configure via API</H4>
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

          <H4>Threshold reference</H4>
          <Table
            headers={['Parameter', 'Unit', 'Range', 'Default', 'Description']}
            rows={[
              ['kp_threshold',   'Kp',   '1.0 – 9.0',   '5.0', 'Alert when Kp reaches or exceeds this value'],
              ['wind_threshold',  'km/s', '100 – 2000', '700',  'Alert when solar wind speed reaches or exceeds this value'],
            ]}
          />

          <H4>Email format</H4>
          <P>Alert emails are sent from the configured <Ic>SMTP_FROM</Ic> address with subject
            <Ic>Astraeusio Space Weather Alert</Ic> and list which thresholds were exceeded along with the measured values.</P>

          {/* ── Error Codes ──────────────────────────────────────────────── */}
          <H2 id="error-codes">Error Codes</H2>
          <P>All error responses have a consistent JSON body:</P>
          <Code lang="json">{`{ "error": "description of the error" }`}</Code>

          <Table
            headers={['Status', 'Code', 'Cause', 'Resolution']}
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
  "error":        "rate_limit_exceeded",
  "limit":        100,
  "used":         100,
  "period_start": 1746057600,
  "period_end":   1746144000
}`}</Code>

          {/* bottom padding */}
          <div className="h-24" />
        </main>
      </div>
    </div>
  )
}
