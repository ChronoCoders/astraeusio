import Navbar from './Navbar'
import Footer from './Footer'

function Section({ id, children, className = '' }) {
  return (
    <section id={id} className={`max-w-3xl mx-auto px-6 py-20 ${className}`}>
      {children}
    </section>
  )
}

function Label({ children }) {
  return (
    <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase mb-4">{children}</p>
  )
}

function Divider() {
  return <div className="border-t border-zinc-800 max-w-3xl mx-auto px-6" />
}

export default function AboutPage({ onSignIn }) {
  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />

      {/* Hero */}
      <section className="max-w-3xl mx-auto px-6 pt-36 pb-16">
        <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase mb-4">About</p>
        <h1 className="text-4xl md:text-5xl font-thin tracking-tight text-zinc-100 leading-tight">
          Built for those who need to know what's happening in space before it affects Earth.
        </h1>
      </section>

      <Divider />

      {/* Mission */}
      <Section id="mission">
        <Label>Mission</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-6">Why Astraeusio exists</h2>
        <div className="space-y-4 text-zinc-400 text-sm leading-relaxed">
          <p>
            Space weather is not an abstract concern. Geomagnetic storms disrupt power grids, degrade GPS accuracy, and saturate satellite communications — sometimes with less than an hour's warning. Solar flares can knock out HF radio across entire hemispheres. A well-timed coronal mass ejection can bring a continent's infrastructure to a halt.
          </p>
          <p>
            The data to see all of this coming has always existed. NOAA's Space Weather Prediction Center, NASA's real-time feeds, the Kyoto World Data Center — they publish continuously. The problem is that the data is scattered, formatted for researchers, and arrives too raw for anyone who just needs a clear answer right now.
          </p>
          <p>
            Astraeusio is the layer between that raw data and the people who need to act on it: satellite operators, grid engineers, amateur radio operators, aviation teams, researchers, and anyone who takes infrastructure seriously. We ingest everything in real time, run it through an LSTM model trained on two decades of NOAA history, and surface what matters.
          </p>
        </div>
      </Section>

      <Divider />

      {/* How it's built */}
      <Section id="built">
        <Label>How it's built</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-6">Performance and reliability, not marketing</h2>
        <div className="space-y-4 text-zinc-400 text-sm leading-relaxed">
          <p>
            The backend is written in Rust. That's not a technology choice made for a job posting — it's the right tool when you need a process that ingests a dozen live data streams simultaneously, serves API responses under load, and does not have memory leaks or surprise crashes. We use Axum with Tokio, store everything in DuckDB, and keep the binary small and self-contained.
          </p>
          <p>
            The forecasting model is a PyTorch LSTM trained on over 20 years of NOAA Kp index measurements. It uses Monte Carlo Dropout to produce calibrated uncertainty estimates alongside every prediction — so when it says Kp 6.2 ± 0.8, that interval means something. The model runs as a separate FastAPI service; if it's ever unreachable, the API falls back to the last cached prediction and tells you so.
          </p>
          <p>
            Data is ingested continuously from NOAA, NASA, Celestrak, and Kyoto WDC. Each source runs on its own poll cycle. Nothing shares a connection pool with anything else. Inserts are incremental — we only write rows that aren't already in the database, so the system stays fast no matter how long it's been running.
          </p>
        </div>
      </Section>

      <Divider />

      {/* Data Sources */}
      <Section id="data-sources">
        <Label>Data Sources</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-8">Where the data comes from</h2>
        <div className="space-y-6">
          {[
            {
              name: 'NOAA Space Weather Prediction Center',
              url: 'https://www.swpc.noaa.gov',
              desc: 'Real-time Kp index, solar wind (DSCOVR satellite), X-ray flux (GOES), IMF Bz, Dst index, and space weather alerts. SWPC is the authoritative operational source for space weather in the US — when a G3 storm is in progress, this is where that classification comes from.',
            },
            {
              name: 'NASA APIs',
              url: 'https://api.nasa.gov',
              desc: 'Near-Earth Object feed (NeoWs), Astronomy Picture of the Day, and EPIC Earth imagery from the DSCOVR spacecraft. NASA\'s open API program makes this data available without rate restrictions for reasonable use.',
            },
            {
              name: 'Celestrak',
              url: 'https://celestrak.org',
              desc: 'Two-line element sets for the entire Starlink constellation, updated on each poll cycle. TLEs are the standard format for satellite tracking — Celestrak maintains one of the most reliable public mirrors.',
            },
            {
              name: 'Kyoto World Data Center for Geomagnetism',
              url: 'https://wdc.kugi.kyoto-u.ac.jp',
              desc: 'Dst (Disturbance Storm Time) index, a measure of the ring current intensity around Earth during geomagnetic storms. Kyoto WDC is the global reference for Dst — the index is computed there and distributed through NOAA.',
            },
          ].map(({ name, url, desc }) => (
            <div key={name} className="border border-zinc-800 rounded-lg p-5 bg-zinc-900/30">
              <div className="flex items-start justify-between gap-4 mb-3">
                <h3 className="text-sm font-medium text-zinc-200">{name}</h3>
                <a
                  href={url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[11px] font-mono text-zinc-500 hover:text-zinc-300 shrink-0 transition-colors"
                >
                  {url.replace('https://', '')} ↗
                </a>
              </div>
              <p className="text-zinc-400 text-sm leading-relaxed">{desc}</p>
            </div>
          ))}
        </div>
      </Section>

      <Divider />

      {/* Transparency */}
      <Section id="transparency">
        <Label>Transparency</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-6">Open by default</h2>
        <div className="space-y-4 text-zinc-400 text-sm leading-relaxed">
          <p>
            We publish our data sources, polling intervals, and model architecture openly. If you find an error in the data or a problem with the model, we want to know.
          </p>
        </div>
      </Section>

      <Divider />

      {/* Contact */}
      <Section id="contact">
        <Label>Contact</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-6">Get in touch</h2>
        <p className="text-zinc-400 text-sm leading-relaxed mb-6">
          For questions about the API, data accuracy, licensing, or anything else — email is the fastest path.
        </p>
        <a
          href="mailto:contact@chronocoder.dev"
          className="inline-block font-mono text-zinc-200 hover:text-white border-b border-zinc-700 hover:border-zinc-400 pb-0.5 transition-colors text-sm"
        >
          contact@chronocoder.dev
        </a>
      </Section>

      <Footer />
    </div>
  )
}
