export const posts = [
  {
    slug: 'how-we-predict-geomagnetic-storms',
    title: 'How We Predict Geomagnetic Storms',
    date: '2025-03-18',
    author: 'Altug Tatlisu',
    tags: ['ML', 'Forecasting', 'Kp Index'],
    image: 'https://assets.science.nasa.gov/dynamicimage/assets/science/esd/eo/images/imagerecords/43000/43191/SolarFlare_Ste_2010045.jpg',
    excerpt:
      'Geomagnetic storms don\'t arrive without warning. The physics gives us a window — sometimes hours, sometimes days. Here\'s how we turn raw magnetometer readings into calibrated probability forecasts.',
    content: `
Geomagnetic storms don't arrive without warning. The physics gives us a window — sometimes hours, sometimes days — between a solar event and its impact on Earth. The question is whether you can read that window accurately enough to act on it. That's what the forecasting model in Astraeusio is built to do.

## The Signal: Kp Index

The Kp index is a global measure of geomagnetic activity, updated every three hours by the Potsdam GFZ research centre and published in near-real-time by NOAA's Space Weather Prediction Center. It runs from 0 (completely quiet) to 9 (extreme storm), and the scale is quasi-logarithmic — a Kp of 7 is roughly ten times more disturbed than Kp 5.

NOAA also publishes an estimated Kp at one-minute resolution, derived from a network of ground magnetometer stations. That's the signal we ingest continuously: a time series of geomagnetic disturbance measurements going back decades.

The reason Kp works as a forecast target is that it has structure. Storms don't materialize at Kp 8 without passing through Kp 4 and 6 first. The index has temporal autocorrelation — knowing the last few hours of readings tells you something real about the next few hours. That's what makes it learnable.

## The Model: LSTM with Monte Carlo Dropout

We use a Long Short-Term Memory network (LSTM) trained on over 20 years of NOAA Kp data downloaded directly from SWPC archives. LSTMs are a class of recurrent neural network designed specifically for sequential data. Unlike a standard feed-forward network, an LSTM maintains a cell state across time steps, which means it can capture dependencies that span hours or days — not just the most recent reading.

The input to the model is a window of recent Kp readings (7 to 48 readings, depending on availability), along with cyclical time features: hour of day encoded as sine and cosine, month of year encoded as sine and cosine, and solar cycle phase. The solar cycle feature matters because background geomagnetic activity varies systematically over the 11-year cycle — solar maximum years have a different baseline than solar minimum years.

The output is a single predicted Kp value for the next three hours.

## Uncertainty: Monte Carlo Dropout

A point prediction without an uncertainty estimate is not very useful in an operational context. If the model says Kp 6.2, you need to know whether that means "somewhere between 5.8 and 6.6" or "somewhere between 4.0 and 8.5". Those are very different operational situations.

We use Monte Carlo Dropout to produce calibrated confidence intervals. During inference, instead of disabling dropout layers (as you normally would after training), we keep them active and run the same input through the model 50 times. Each run samples a slightly different subset of the network, producing a slightly different prediction. The spread across those 50 predictions gives us an empirical distribution.

From that distribution we report:
- **predicted_kp**: the mean across all 50 passes
- **ci_lower / ci_upper**: the 2.5th and 97.5th percentiles (a 95% confidence interval)
- **uncertainty**: the standard deviation

A narrow interval means the model is confident — recent Kp history points clearly in one direction. A wide interval means the situation is ambiguous, and you should weight the forecast accordingly.

## What the Model Doesn't Know

The model has no direct access to solar wind data, interplanetary magnetic field measurements, or coronagraph imagery from SOHO or STEREO. It sees only the ground-level magnetic record and time features. This means it cannot predict a sudden storm onset caused by a fast CME that hasn't yet affected the magnetometer network.

What it can do is recognise precursor patterns — the subtle increase in Kp that often precedes a major storm — and extend trends that are already underway. For storms that develop gradually, it performs well. For sudden commencement events with no precursor, you still need real-time solar wind data from DSCOVR, which Astraeusio also ingests separately.

The forecast is one input among several. The IMF Bz, solar wind speed, and X-ray flux data all tell complementary parts of the story.

## Graceful Degradation

The ML service runs as a separate process. If it's unreachable — during a restart, during an update, during any transient failure — the API falls back to the most recently cached forecast and annotates the response with \`"status": "degraded", "source": "cache"\`. The frontend displays this state explicitly. No silent failures.

The cache-backed forecast may be minutes or hours old depending on when the ML service last responded. Treat it accordingly.
    `.trim(),
  },

  {
    slug: 'space-weather-satellite-operators',
    title: 'Space Weather Risk for Satellite Operators',
    date: '2025-04-02',
    author: 'Altug Tatlisu',
    tags: ['Operations', 'Satellites', 'Risk'],
    image: 'https://svs.gsfc.nasa.gov/vis/a000000/a005600/a005615/nisar_orbit.02415.jpg',
    excerpt:
      'Geomagnetic storms are not abstract events for satellite operators. They change atmospheric drag, accelerate component degradation, and in severe cases have ended missions. Here\'s what the data actually shows.',
    content: `
Geomagnetic storms are not abstract events for satellite operators. They change the operational environment of every satellite in low Earth orbit, and the effects scale with storm intensity in ways that are predictable enough to plan around — if you have the data in front of you.

## Atmospheric Drag and Orbital Decay

The most immediate operational effect of a geomagnetic storm at LEO altitudes is increased atmospheric drag. During a storm, energy deposited in the upper atmosphere causes it to heat and expand. The thermosphere at 400 km altitude can increase in density by a factor of 10 during a major storm. Satellites encounter more air molecules per orbit, lose altitude faster, and require more frequent propulsion to maintain station.

The 2003 Halloween storms — which peaked at Kp 9, the maximum — caused the orbits of thousands of tracked objects to decay measurably within hours. Operators who hadn't budgeted propellant for anomalous drag had a difficult week. The ISS performed emergency boosts. Several smaller satellites that lacked propulsion lost significant altitude and never recovered it.

Starlink maintains a large constellation at LEO specifically because individual satellite lifetime is short and replacement is routine. But during the February 2022 geomagnetic storm (Kp reached 5-6), SpaceX lost 38 of 49 newly launched Starlink satellites to elevated drag during their orbit-raising phase. They were at 210 km — below operational altitude, where the drag effect is amplified — when a moderate storm hit. SpaceX estimated the atmospheric density was up to 50% higher than predicted. The satellites entered safe mode, increased drag in that configuration, and the orbits decayed before they could raise.

That wasn't a catastrophic storm by historical standards. Kp 5 is a G1 event. It happens several times a year.

## Surface Charging and Component Degradation

In geostationary orbit (GEO), the primary mechanism is different. During geomagnetic storms, energetic electrons trapped in the outer Van Allen belt are injected into GEO and can accumulate on spacecraft surfaces. If the charging differential becomes high enough, it discharges — a transient pulse that can damage electronics, corrupt memory, or permanently degrade solar arrays.

The 1994 failure of Canada's Anik-E1 and Anik-E2 satellites occurred during elevated electron flux. Anik-E1 lost attitude control, resulting in loss of service to remote communities in Canada's north. The investigation attributed the failure to electrostatic discharge triggered by high-energy electrons. At the time, the connection between satellite anomalies and the space weather environment was less systematically tracked than it is today.

Insurance claims for satellite anomalies are noticeably correlated with geomagnetic storm periods. The industry knows this. Most large operators now include space weather clauses in their satellite design requirements.

## Single-Event Upsets

High-energy protons — particularly during solar energetic particle (SEP) events following X-class flares — can penetrate shielding and flip bits in memory. These are single-event upsets (SEUs): individually benign, but capable of causing software corruption, false commands, or attitude control failures if they hit the wrong register.

Deep-space missions are particularly exposed. The Voyager probes, Cassini, and New Horizons all experienced SEUs over their operational lives. For LEO operators, the South Atlantic Anomaly is a permanent source of enhanced SEU rates regardless of solar activity — but geomagnetic storms increase particle penetration and raise the baseline risk everywhere.

GPS satellite anomalies are well-correlated with SEP events. The aviation industry relies on GPS-based navigation systems that declare themselves unavailable during high SEP flux, which translates directly to flight delays and reroutings.

## What to Watch

For LEO operators, the key metrics are:
- **Kp index**: values ≥ 5 indicate conditions worth monitoring for drag effects
- **Kp forecast**: 3-hour prediction lets you plan propulsion windows or delay launches
- **Solar wind speed**: >600 km/s indicates a fast solar wind stream that will likely elevate Kp
- **IMF Bz**: strongly negative Bz (< -10 nT) is the primary driver of energy coupling into the magnetosphere

For GEO operators:
- **Outer electron belt flux** (not directly available from NOAA's SWPC free data, but proxy indicators are)
- **Kp history**: sustained Kp > 5 for multiple hours indicates injection events likely in progress
- **Dst index**: deeply negative Dst (< -100 nT) indicates major ring current enhancement and elevated GEO charging risk

Astraeusio ingests all of these in real time. The anomaly detection layer fires alerts when thresholds are crossed. The forecast gives you a 3-hour look-ahead. None of this replaces a dedicated space weather operations team for a multi-satellite constellation, but for operators who can't afford one full-time, it's a serious operational aid.

## The Honest Assessment

Space weather is probabilistic. A Kp 7 forecast doesn't mean your satellite will fail — it means the environment is hostile and your risk is elevated. Most satellites survive most storms. The goal of space weather monitoring isn't to predict individual failures; it's to understand the environment well enough to reduce exposure during the highest-risk periods and to have a coherent explanation when something does go wrong.

The operators who got caught by the 2022 Starlink losses didn't have bad technology. They had a forecast that was off by 50% in atmospheric density. Better forecasting tools don't eliminate that uncertainty, but they reduce it.
    `.trim(),
  },

  {
    slug: 'understanding-kp-index',
    title: 'Understanding the Kp Index',
    date: '2025-04-21',
    author: 'Altug Tatlisu',
    tags: ['Explainer', 'Kp Index', 'Space Weather'],
    image: 'https://assets.science.nasa.gov/dynamicimage/assets/science/esd/eo/images/imagerecords/151000/151043/naaurora_vir_2023058.jpg',
    excerpt:
      'The Kp index is the most widely cited number in space weather. Here\'s what it actually measures, how it\'s computed, what the scale means, and why it matters for everything from aurora to power grids.',
    content: `
The Kp index is the most widely cited number in space weather. It appears in NOAA storm alerts, satellite operator briefings, aurora forecast apps, and ham radio propagation guides. But what exactly does it measure, how is it computed, and what does a given value actually mean for the world below?

## What Kp Measures

Kp is a global measure of geomagnetic activity. More specifically, it measures how disturbed Earth's magnetic field is, as observed by a network of ground-based magnetometer stations distributed across the planet at mid-latitudes (roughly 44° to 60° magnetic latitude).

Each station computes a local K index every three hours by measuring the range of variation in the horizontal component of the magnetic field. The range is compared to a baseline for that station and converted to a quasi-logarithmic scale from 0 to 9. The planet-wide Kp is then derived by averaging the K indices from the contributing stations, with a weighting scheme to account for geographic distribution.

The three-hour averaging period is both a strength and a limitation. It smooths out short-duration disturbances that don't represent sustained storm conditions. It also means Kp lags real-time conditions by up to three hours — the value published at any given moment reflects the previous three-hour window.

NOAA also publishes an estimated real-time Kp (updated every minute) derived from a slightly different algorithm using a smaller network of stations. This is the value Astraeusio ingests and displays at one-minute resolution. It's noisier than the official three-hour product but much more timely.

## The Scale: 0 to 9

The Kp scale runs from 0 to 9, in steps of one-third: 0, 0.33, 0.67, 1, 1.33, and so on up to 9. In practice you'll often see this written as 0o, 0+, 1-, 1o, 1+, and so on — the o/+/- notation indicates the third within each integer step.

The scale is quasi-logarithmic. A Kp of 5 corresponds to roughly three times the magnetic variation of Kp 4, not just one unit more. This means comparisons at the high end of the scale are particularly significant — the difference between Kp 7 and Kp 9 represents an enormous increase in storm energy.

| Kp | Condition | Effect |
|----|-----------|--------|
| 0–1 | Quiet | No significant activity |
| 2–3 | Unsettled | Minor fluctuations |
| 4 | Active | Aurora visible at high latitudes (>60°) |
| 5 | G1 Minor Storm | Aurora at 60°+ latitude; minor power grid fluctuations possible |
| 6 | G2 Moderate Storm | Aurora at 55°+ latitude; HF radio disruption at high latitudes |
| 7 | G3 Strong Storm | Aurora at 50°+ latitude; intermittent navigation errors |
| 8 | G4 Severe Storm | Aurora at 45°+ latitude; widespread HF disruption; pipeline corrosion currents |
| 9 | G5 Extreme Storm | Aurora at 40° and below; widespread power grid problems; satellite damage possible |

## The G-Scale

NOAA uses the Kp index as the basis for its geomagnetic storm classification (the G scale, G1 through G5). This is the scale you'll see in official storm watches, warnings, and alerts.

G1 begins at Kp 5. G5 requires sustained Kp 9. The G scale is specifically designed for communicating operational impact, so each level comes with a standardised description of effects on power systems, spacecraft operations, radio propagation, and aurora visibility.

The mapping from Kp to G level is direct and fixed. G1 = Kp 5, G2 = Kp 6, G3 = Kp 7, G4 = Kp 8, G5 = Kp 9.

## Aurora Visibility

Aurora visibility is one of the most concrete ways the Kp index manifests for most people. The relationship between Kp and equatorward boundary of the auroral oval is well established, though it depends on local time and geographic longitude as well.

At Kp 5, aurora can be seen at magnetic latitudes of around 60° — roughly the latitude of Fairbanks, Alaska, or Tromsø, Norway. At Kp 7, that boundary drops to around 50°, covering much of Canada, Scandinavia, and northern Germany. At Kp 9, the oval expands to 40° and below — low enough to see aurora from New York, Rome, or Beijing, which happens only a few times per solar cycle.

The 1989 Quebec blackout (caused by a G5 storm, Kp 9) was accompanied by aurora visible as far south as Texas and Florida. The 2003 Halloween storms produced aurora visible from the southern United States and parts of Europe well outside typical aurora zones.

## What Kp Doesn't Tell You

Kp is a useful summary statistic, but it compresses a lot of information into a single number. A few things it doesn't capture:

**Direction matters.** The primary driver of geomagnetic storm intensity isn't the magnitude of the solar wind magnetic field — it's the southward component (negative Bz). When the interplanetary magnetic field (IMF) points southward, it can couple efficiently with Earth's northward-pointing magnetic field through a process called magnetic reconnection. A solar wind with high speed but northward IMF will produce relatively little storm activity. The same speed with strongly southward IMF can drive a major storm. The Kp index reflects the outcome of this process, but doesn't tell you which factor drove it.

**Duration matters.** A sustained Kp 7 for six hours is far more damaging to infrastructure than a brief Kp 7 spike followed by recovery. The Dst index (Disturbance Storm Time) captures this better — it measures the total energy injected into the ring current over the storm's lifetime and doesn't reset until recovery is complete.

**Recovery phase matters.** After the peak of a storm, Kp can drop sharply while the ring current — and its associated effects on the radiation belts — takes hours to days to dissipate. Satellite operators and power grid engineers think about the full storm lifecycle, not just the peak.

For most purposes, Kp is the right number to watch. For a deeper operational picture, pair it with solar wind speed, IMF Bz, and Dst index together.
    `.trim(),
  },
]

export function getPost(slug) {
  return posts.find(p => p.slug === slug) ?? null
}
