// Runs at module evaluation time — before React mounts — so tools are present
// on every page (landing, dashboard, etc.) immediately on load.

function authHeaders() {
  const token = localStorage.getItem('token')
  return token ? { Authorization: `Bearer ${token}` } : {}
}

async function apiFetch(path) {
  const res = await fetch(path, { headers: authHeaders() })
  if (!res.ok) throw new Error(`${path} returned ${res.status}`)
  return res.json()
}

const TOOLS = [
  {
    name: 'get_current_kp',
    title: 'Current Kp Index',
    description: 'Get the current Kp index and recent 1-minute Kp readings from NOAA. Kp ≥ 5 = geomagnetic storm.',
    inputSchema: { type: 'object', properties: {} },
    annotations: { readOnlyHint: true },
    execute: () => fetch('/api/public/kp').then(r => r.json()),
  },
  {
    name: 'get_solar_wind',
    title: 'Solar Wind Data',
    description: 'Get the latest solar wind speed (km/s) and density (p/cm³) from NOAA DSCOVR.',
    inputSchema: { type: 'object', properties: {} },
    annotations: { readOnlyHint: true },
    execute: () => fetch('/api/public/solar-wind').then(r => r.json()),
  },
  {
    name: 'get_kp_forecast',
    title: 'Kp Forecast',
    description: 'Get the 3-hour ML Kp forecast with 95% confidence interval from an LSTM model trained on 20+ years of NOAA data.',
    inputSchema: { type: 'object', properties: {} },
    annotations: { readOnlyHint: true },
    execute: () => fetch('/api/public/forecast').then(r => r.json()),
  },
  {
    name: 'get_health',
    title: 'Service Health',
    description: 'Get the operational health status of all Astraeusio data sources (NOAA, NASA, ML service, database).',
    inputSchema: { type: 'object', properties: {} },
    annotations: { readOnlyHint: true },
    execute: () => fetch('/api/health').then(r => r.json()),
  },
  {
    name: 'get_space_weather_summary',
    title: 'Space Weather Summary',
    description: 'Get a combined summary of current space weather: Kp index, solar wind, X-ray flux, and active alerts.',
    inputSchema: { type: 'object', properties: {} },
    annotations: { readOnlyHint: true },
    execute: async () => {
      const [kp, wind, forecast, alerts] = await Promise.all([
        fetch('/api/public/kp').then(r => r.json()).catch(() => null),
        fetch('/api/public/solar-wind').then(r => r.json()).catch(() => null),
        fetch('/api/public/forecast').then(r => r.json()).catch(() => null),
        apiFetch('/api/alerts').catch(() => null),
      ])
      return { kp, wind, forecast, alerts }
    },
  },
  {
    name: 'get_anomalies',
    title: 'Space Weather Anomalies',
    description: 'Get detected space weather anomalies: geomagnetic storms, solar flares, solar wind spikes, asteroid close approaches, and ML storm forecasts. Requires authentication.',
    inputSchema: { type: 'object', properties: {} },
    annotations: { readOnlyHint: true },
    execute: () => apiFetch('/api/anomalies'),
  },
  {
    name: 'get_neo_close_approaches',
    title: 'Near-Earth Object Approaches',
    description: 'Get NASA near-Earth object close approaches for the next 7 days with miss distance (lunar), diameter, velocity, and hazard flag. Requires authentication.',
    inputSchema: { type: 'object', properties: {} },
    annotations: { readOnlyHint: true },
    execute: () => apiFetch('/api/neo'),
  },
  {
    name: 'get_iss_position',
    title: 'ISS Position',
    description: 'Get the current position of the International Space Station (latitude, longitude, altitude, velocity). Requires authentication.',
    inputSchema: { type: 'object', properties: {} },
    annotations: { readOnlyHint: true },
    execute: () => apiFetch('/api/iss'),
  },
]

;(function registerWebMCP() {
  if (typeof navigator === 'undefined' || !navigator.modelContext) return
  const mc = navigator.modelContext

  // Chrome EPP API shape
  if (typeof mc.provideContext === 'function') {
    mc.provideContext({ tools: TOOLS }).catch(() => {})
    return
  }

  // W3C draft API shape
  if (typeof mc.registerTool === 'function') {
    for (const tool of TOOLS) {
      try { mc.registerTool(tool) } catch (_) { /* ignore */ }
    }
  }
})()
