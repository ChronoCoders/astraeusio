import { useEffect } from 'react'

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
    description: 'Get the current Kp index and recent 1-minute Kp readings from NOAA. Kp ≥ 5 = geomagnetic storm.',
    inputSchema: { type: 'object', properties: {}, required: [] },
    execute: () => fetch('/api/public/kp').then(r => r.json()),
  },
  {
    name: 'get_solar_wind',
    description: 'Get the latest solar wind speed (km/s) and density (p/cm³) from NOAA DSCOVR.',
    inputSchema: { type: 'object', properties: {}, required: [] },
    execute: () => fetch('/api/public/solar-wind').then(r => r.json()),
  },
  {
    name: 'get_kp_forecast',
    description: 'Get the 3-hour ML Kp forecast with 95% confidence interval from an LSTM model.',
    inputSchema: { type: 'object', properties: {}, required: [] },
    execute: () => fetch('/api/public/forecast').then(r => r.json()),
  },
  {
    name: 'get_health',
    description: 'Get the operational health status of all Astraeusio data sources (NOAA, NASA, ML service, database).',
    inputSchema: { type: 'object', properties: {}, required: [] },
    execute: () => fetch('/api/health').then(r => r.json()),
  },
  {
    name: 'get_space_weather_summary',
    description: 'Get a combined summary of current space weather: Kp index, solar wind, IMF Bz, X-ray flux, and active alerts.',
    inputSchema: { type: 'object', properties: {}, required: [] },
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
    description: 'Get detected space weather anomalies: geomagnetic storms, solar flares, solar wind spikes, asteroid close approaches, and ML storm forecasts. Requires authentication.',
    inputSchema: { type: 'object', properties: {}, required: [] },
    execute: () => apiFetch('/api/anomalies'),
  },
  {
    name: 'get_neo_close_approaches',
    description: 'Get NASA near-Earth object close approaches for the next 7 days with miss distance (lunar), diameter, velocity, and hazard flag. Requires authentication.',
    inputSchema: { type: 'object', properties: {}, required: [] },
    execute: () => apiFetch('/api/neo'),
  },
  {
    name: 'get_iss_position',
    description: 'Get the current position of the International Space Station (latitude, longitude, altitude, velocity). Requires authentication.',
    inputSchema: { type: 'object', properties: {}, required: [] },
    execute: () => apiFetch('/api/iss'),
  },
]

export function useWebMCP() {
  useEffect(() => {
    if (typeof navigator === 'undefined' || !navigator.modelContext?.provideContext) return

    navigator.modelContext.provideContext({ tools: TOOLS }).catch(() => {})
  }, [])
}
