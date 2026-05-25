export const posts = [
  {
    slug: 'how-we-predict-geomagnetic-storms',
    title: 'How We Predict Geomagnetic Storms',
    titleTr: 'Jeomanyetik Fırtınaları Nasıl Tahmin Ediyoruz',
    date: '2025-03-18',
    author: 'Altug Tatlisu',
    tags: ['ML', 'Forecasting', 'Kp Index'],
    image: 'https://assets.science.nasa.gov/dynamicimage/assets/science/esd/eo/images/imagerecords/43000/43191/SolarFlare_Ste_2010045.jpg',
    excerpt:
      'Geomagnetic storms don\'t arrive without warning. The physics gives us a window — sometimes hours, sometimes days. Here\'s how we turn raw magnetometer readings into calibrated probability forecasts.',
    excerptTr:
      'Jeomanyetik fırtınalar önceden haber vermeden gelmez. Fizik bize bir pencere sunar — bazen saatler, bazen günler. Ham manyetometre okumalarını kalibreli olasılık tahminlerine nasıl dönüştürdüğümüzü anlatıyoruz.',
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
    contentTr: `
Jeomanyetik fırtınalar önceden haber vermeden gelmez. Fizik bize bir güneş olayı ile Dünya üzerindeki etkisi arasında bazen saatler, bazen günler süren bir pencere sunar. Soru, bu pencereyi üzerine hareket edecek kadar doğru okuyup okuyamayacağınızdır. İşte Astraeusio'daki tahmin modelinin yapmak için inşa edildiği şey budur.

## Sinyal: Kp Endeksi

Kp endeksi, Potsdam GFZ araştırma merkezi tarafından her üç saatte bir güncellenen ve NOAA'nın Uzay Hava Tahmin Merkezi tarafından gerçek zamanlıya yakın yayımlanan küresel bir jeomanyetik aktivite ölçüsüdür. 0 (tamamen sakin) ile 9 (aşırı fırtına) arasında değişir ve ölçek yarı-logaritmiktir — Kp 7, Kp 5'ten yaklaşık on kat daha rahatsız edicidir.

NOAA aynı zamanda yer manyetometre istasyonları ağından türetilen, dakikalık çözünürlükte tahmini bir Kp yayımlar. Bu, sürekli aldığımız sinyal: onlarca yıla uzanan jeomanyetik rahatsızlık ölçümlerinin zaman serisi.

Kp'nin tahmin hedefi olarak işe yaramasının nedeni yapıya sahip olmasıdır. Fırtınalar, Kp 4 ve 6'dan geçmeden Kp 8'de ortaya çıkmaz. Endeksin zamansal otokorelasyonu vardır — son birkaç saatin okumalarını bilmek, sonraki birkaç saat hakkında gerçek bir şey söyler. Bu, öğrenilebilir olmasını sağlayan şeydir.

## Model: Monte Carlo Dropout'lu LSTM

NOAA Kp verisi üzerinde eğitilmiş bir Uzun Kısa Süreli Bellek ağı (LSTM) kullanıyoruz; veriler doğrudan SWPC arşivlerinden indirilmiş, 20 yılı aşkın geçmişe sahiptir. LSTM'ler sıralı veriler için özel olarak tasarlanmış bir tekrarlayan sinir ağı sınıfıdır. Standart bir ileri besleme ağının aksine, LSTM zaman adımları boyunca bir hücre durumu korur; bu da saatler veya günler boyunca uzanan bağımlılıkları yakalayabileceği anlamına gelir.

Modelin girdisi, en son Kp okumalarından oluşan bir penceredir (mevcudiyete bağlı olarak 7 ile 48 okuma), döngüsel zaman özellikleriyle birlikte: sinüs ve kosinüs olarak kodlanmış günün saati, yılın ayı ve güneş döngüsü fazı. Güneş döngüsü özelliği önemlidir çünkü arka plan jeomanyetik aktivitesi 11 yıllık döngü boyunca sistematik olarak değişir.

Çıktı, sonraki üç saat için tek bir tahmin edilen Kp değeridir.

## Belirsizlik: Monte Carlo Dropout

Belirsizlik tahmini olmayan bir nokta tahmini, operasyonel bağlamda pek kullanışlı değildir. Model Kp 6.2 diyorsa, bunun "5.8 ile 6.6 arasında" mı yoksa "4.0 ile 8.5 arasında" mı olduğunu bilmeniz gerekir. Bunlar çok farklı operasyonel durumlardır.

Kalibreli güven aralıkları üretmek için Monte Carlo Dropout kullanıyoruz. Çıkarım sırasında dropout katmanlarını devre dışı bırakmak yerine aktif tutuyoruz ve aynı girişi model üzerinden 50 kez çalıştırıyoruz. Her çalışma ağın biraz farklı bir alt kümesini örnekler ve biraz farklı bir tahmin üretir. Bu 50 tahmin genelindeki yayılım bize deneysel bir dağılım verir.

Bu dağılımdan şunları bildiriyoruz:
- **predicted_kp**: 50 geçiş genelinde ortalama
- **ci_lower / ci_upper**: 2,5. ve 97,5. yüzdelikler (%95 güven aralığı)
- **uncertainty**: standart sapma

Dar bir aralık modelin güvenli olduğu anlamına gelir. Geniş bir aralık durumun belirsiz olduğunu ve tahmini buna göre ağırlıklandırmanız gerektiğini gösterir.

## Modelin Bilmediği

Modelin güneş rüzgarı verilerine, gezegenlerarası manyetik alan ölçümlerine veya SOHO ya da STEREO'dan koronagraf görüntülerine doğrudan erişimi yoktur. Yalnızca yer düzeyi manyetik kaydı ve zaman özelliklerini görür. Bu, manyetometre ağını henüz etkilememiş hızlı bir CME'nin neden olduğu ani fırtına başlangıcını tahmin edemeyeceği anlamına gelir.

Yapabildiği şey, öncü kalıpları tanımaktır — büyük bir fırtınadan önce Kp'deki ince artış — ve halihazırda devam eden eğilimleri uzatmak. Kademeli gelişen fırtınalarda iyi performans gösterir. Anlık başlangıç olayları için, Astraeusio'nun ayrıca aldığı DSCOVR'dan gerçek zamanlı güneş rüzgarı verisine ihtiyacınız var.

Tahmin, birkaç girdiden biridir. IMF Bz, güneş rüzgarı hızı ve X-ışını akısı verileri tablonun tamamlayıcı bölümlerini anlatır.

## Zarif Bozulma

ML servisi ayrı bir süreç olarak çalışır. Erişilemez hale gelirse API en son önbelleğe alınmış tahmine geri döner ve yanıtı \`"status": "degraded", "source": "cache"\` ile işaretler. Ön yüz bu durumu açıkça gösterir. Sessiz hatalar yoktur.

Önbelleğe alınmış tahmin, ML servisinin son yanıt verdiğine bağlı olarak dakikalar veya saatler öncesine ait olabilir. Buna göre değerlendirin.
    `.trim(),
  },

  {
    slug: 'space-weather-satellite-operators',
    title: 'Space Weather Risk for Satellite Operators',
    titleTr: 'Uydu Operatörleri için Uzay Hava Riski',
    date: '2025-04-02',
    author: 'Altug Tatlisu',
    tags: ['Operations', 'Satellites', 'Risk'],
    image: 'https://svs.gsfc.nasa.gov/vis/a000000/a005600/a005615/nisar_orbit.02415.jpg',
    excerpt:
      'Geomagnetic storms are not abstract events for satellite operators. They change atmospheric drag, accelerate component degradation, and in severe cases have ended missions. Here\'s what the data actually shows.',
    excerptTr:
      'Jeomanyetik fırtınalar uydu operatörleri için soyut olaylar değildir. Atmosferik sürüklemeyi değiştirirler, bileşen bozunmasını hızlandırırlar ve şiddetli vakalarda görevleri sona erdirmişlerdir. Verilerin gerçekte ne gösterdiğini inceliyoruz.',
    content: `
Geomagnetic storms are not abstract events for satellite operators. They change the operational environment of every satellite in low Earth orbit, and the effects scale with storm intensity in ways that are predictable enough to plan around — if you have the data in front of you.

## Atmospheric Drag and Orbital Decay

The most immediate operational effect of a geomagnetic storm at LEO altitudes is increased atmospheric drag. During a storm, energy deposited in the upper atmosphere causes it to heat and expand. The thermosphere at 400 km altitude can increase in density by a factor of 10 during a major storm. Satellites encounter more air molecules per orbit, lose altitude faster, and require more frequent propulsion to maintain station.

The 2003 Halloween storms — which peaked at Kp 9, the maximum — caused the orbits of thousands of tracked objects to decay measurably within hours. Operators who hadn't budgeted propellant for anomalous drag had a difficult week. The ISS performed emergency boosts. Several smaller satellites that lacked propulsion lost significant altitude and never recovered it.

Starlink maintains a large constellation at LEO specifically because individual satellite lifetime is short and replacement is routine. But during the February 2022 geomagnetic storm (Kp reached 5-6), SpaceX lost 38 of 49 newly launched Starlink satellites to elevated drag during their orbit-raising phase. They were at 210 km — below operational altitude, where the drag effect is amplified — when a moderate storm hit. SpaceX estimated the atmospheric density was up to 50% higher than predicted. The satellites entered safe mode, increased drag in that configuration, and the orbits decayed before they could raise.

That wasn't a catastrophic storm by historical standards. Kp 5 is a G1 event. It happens several times a year.

## Surface Charging and Component Degradation

In geostationary orbit (GEO), the primary mechanism is different. During geomagnetic storms, energetic electrons trapped in the outer Van Allen belt are injected into GEO and can accumulate on spacecraft surfaces. If the charging differential becomes high enough, it discharges — a transient pulse that can damage electronics, corrupt memory, or permanently degrade solar arrays.

The 1994 failure of Canada's Anik-E1 and Anik-E2 satellites occurred during elevated electron flux. Anik-E1 lost attitude control, resulting in loss of service to remote communities in Canada's north. The investigation attributed the failure to electrostatic discharge triggered by high-energy electrons.

Insurance claims for satellite anomalies are noticeably correlated with geomagnetic storm periods. The industry knows this. Most large operators now include space weather clauses in their satellite design requirements.

## Single-Event Upsets

High-energy protons — particularly during solar energetic particle (SEP) events following X-class flares — can penetrate shielding and flip bits in memory. These are single-event upsets (SEUs): individually benign, but capable of causing software corruption, false commands, or attitude control failures if they hit the wrong register.

GPS satellite anomalies are well-correlated with SEP events. The aviation industry relies on GPS-based navigation systems that declare themselves unavailable during high SEP flux, which translates directly to flight delays and reroutings.

## What to Watch

For LEO operators, the key metrics are:
- **Kp index**: values ≥ 5 indicate conditions worth monitoring for drag effects
- **Kp forecast**: 3-hour prediction lets you plan propulsion windows or delay launches
- **Solar wind speed**: >600 km/s indicates a fast solar wind stream that will likely elevate Kp
- **IMF Bz**: strongly negative Bz (< -10 nT) is the primary driver of energy coupling into the magnetosphere

For GEO operators:
- **Kp history**: sustained Kp > 5 for multiple hours indicates injection events likely in progress
- **Dst index**: deeply negative Dst (< -100 nT) indicates major ring current enhancement and elevated GEO charging risk

Astraeusio ingests all of these in real time. The anomaly detection layer fires alerts when thresholds are crossed. The forecast gives you a 3-hour look-ahead.

## The Honest Assessment

Space weather is probabilistic. A Kp 7 forecast doesn't mean your satellite will fail — it means the environment is hostile and your risk is elevated. Most satellites survive most storms. The goal of space weather monitoring isn't to predict individual failures; it's to understand the environment well enough to reduce exposure during the highest-risk periods and to have a coherent explanation when something does go wrong.
    `.trim(),
    contentTr: `
Jeomanyetik fırtınalar uydu operatörleri için soyut olaylar değildir. Alçak Dünya yörüngesindeki her uyduyu etkilerler ve etkiler, önünüzde veri varsa etrafında plan kurabileceğiniz tahmin edilebilir yollarla fırtına yoğunluğuyla ölçeklenir.

## Atmosferik Sürükleme ve Yörünge Bozunması

LEO yüksekliklerinde jeomanyetik fırtınanın en anlık operasyonel etkisi artan atmosferik sürüklemedir. Fırtına sırasında üst atmosfere biriktirilen enerji onu ısıtır ve genişletir. 400 km yükseklikte termosfer yoğunluğu büyük bir fırtına sırasında 10 kat artabilir. Uydular yörünge başına daha fazla hava molekülüyle karşılaşır, daha hızlı alçalır ve istasyon tutundurma için daha sık tahrik gerektirir.

2003 Halloween fırtınaları — maksimum olan Kp 9'da zirve yaptı — takip edilen binlerce nesnenin yörüngesinin saatler içinde ölçülebilir şekilde bozunmasına neden oldu. Anormal sürükleme için yakıt bütçelemeyenler için zor bir hafta oldu. ISS acil takviyeler yaptı. İtme sistemleri olmayan bazı küçük uydular önemli yükseklik kaybetti ve bunu bir daha telafi edemedi.

Şubat 2022 jeomanyetik fırtınası sırasında (Kp 5-6'ya ulaştı) SpaceX, yeni fırlatılan 49 Starlink uydusunun 38'ini yörünge yükseltme aşamasında yüksek sürüklemeden kaybetti. Ilımlı bir fırtına vurduğunda 210 km'de, sürükleme etkisinin yoğunlaştığı operasyonel yüksekliğin altında bulunuyorlardı. SpaceX, atmosfer yoğunluğunun tahmin edilenden %50 daha yüksek olduğunu tahmin etti.

Bu tarihin standartlarına göre felaket bir fırtına değildi. Kp 5 bir G1 olayıdır. Yılda birkaç kez gerçekleşir.

## Yüzey Yüklemesi ve Bileşen Bozunması

Jeostasoner yörüngede (GEO) birincil mekanizma farklıdır. Jeomanyetik fırtınalar sırasında dış Van Allen kemerinde tutulan enerjik elektronlar GEO'ya enjekte edilir ve uzay aracı yüzeylerinde birikebilir. Yükleme farkı yeterince yüksek olursa boşalır — elektroniklere zarar verebilecek, belleği bozabilecek veya güneş panellerini kalıcı olarak bozabilecek geçici bir darbe oluşur.

1994'te Kanada'nın Anik-E1 ve Anik-E2 uydularının arızası yüksek elektron akısı sırasında meydana geldi. Anik-E1 yön kontrolünü kaybederek Kanada kuzeyindeki uzak topluluklar için hizmet kesintisine yol açtı. Soruşturma arızayı yüksek enerjili elektronların tetiklediği elektrostatik boşalmaya bağladı.

Uydu anomalileri için sigorta talepleri jeomanyetik fırtına dönemleriyle belirgin biçimde koreledir. Sektör bunu biliyor. Büyük operatörlerin çoğu artık uydu tasarım gereksinimlerine uzay hava maddelerini dahil ediyor.

## Tek Olay Bozulmaları

Yüksek enerjili protonlar — özellikle X sınıfı patlamaları izleyen güneş enerjili parçacık (SEP) olayları sırasında — zırhı delip bellekteki bitleri çevirebilir. Bunlar tek olay bozulmalarıdır: bireysel olarak zararsız, ancak yanlış yazmaçı vururlarsa yazılım bozulmasına, yanlış komutlara veya yön kontrol arızalarına yol açabilirler.

GPS uydusu anomalileri SEP olaylarıyla iyi koreledir. Havacılık endüstrisi, yüksek SEP akısı sırasında kendini kullanılamaz ilan eden GPS tabanlı navigasyon sistemlerine dayanır; bu da doğrudan uçuş gecikmelerine ve rota değişikliklerine çevirir.

## Nelere Dikkat Edilmeli

LEO operatörleri için temel ölçütler:
- **Kp endeksi**: ≥5 değerleri sürükleme etkileri için izlemeye değer koşulları gösterir
- **Kp tahmini**: 3 saatlik tahmin tahrik pencerelerini planlamanıza veya fırlatmaları ertelemenize olanak tanır
- **Güneş rüzgarı hızı**: >600 km/s Kp'yi yükseltecek hızlı bir güneş rüzgarı akışını gösterir
- **IMF Bz**: güçlü negatif Bz (< -10 nT) manyetosferle enerji bağlantısının birincil sürücüsüdür

GEO operatörleri için:
- **Kp geçmişi**: birden fazla saat boyunca süregelen Kp > 5 enjeksiyon olaylarının devam ettiğini gösterir
- **Dst endeksi**: derin negatif Dst (< -100 nT) büyük halka akımı güçlenmesini ve yüksek GEO yükleme riskini gösterir

Astraeusio tüm bunları gerçek zamanlı olarak alır. Anomali tespit katmanı eşikler aşıldığında uyarı verir. Tahmin 3 saatlik bir ileriye bakış sağlar.

## Dürüst Değerlendirme

Uzay hava durumu olasılıksaldır. Kp 7 tahmini uydunuzun arızalanacağı anlamına gelmez — ortamın düşmanca olduğu ve riskinizin yüksek olduğu anlamına gelir. Uydular fırtınaların büyük çoğunluğundan sağ çıkar. Uzay hava izlemenin amacı bireysel arızaları tahmin etmek değil; en yüksek riskli dönemlerde maruziyeti azaltacak kadar iyi ortamı anlamak ve bir şey ters gittiğinde tutarlı bir açıklamaya sahip olmaktır.
    `.trim(),
  },

  {
    slug: 'understanding-kp-index',
    title: 'Understanding the Kp Index',
    titleTr: 'Kp Endeksini Anlamak',
    date: '2025-04-21',
    author: 'Altug Tatlisu',
    tags: ['Explainer', 'Kp Index', 'Space Weather'],
    image: 'https://assets.science.nasa.gov/dynamicimage/assets/science/esd/eo/images/imagerecords/151000/151043/naaurora_vir_2023058.jpg',
    excerpt:
      'The Kp index is the most widely cited number in space weather. Here\'s what it actually measures, how it\'s computed, what the scale means, and why it matters for everything from aurora to power grids.',
    excerptTr:
      'Kp endeksi, uzay hava durumunda en sık atıfta bulunulan sayıdır. Gerçekte neyi ölçtüğünü, nasıl hesaplandığını, ölçeğin ne anlama geldiğini ve auroradan elektrik şebekelerine kadar neden önemli olduğunu açıklıyoruz.',
    content: `
The Kp index is the most widely cited number in space weather. It appears in NOAA storm alerts, satellite operator briefings, aurora forecast apps, and ham radio propagation guides. But what exactly does it measure, how is it computed, and what does a given value actually mean for the world below?

## What Kp Measures

Kp is a global measure of geomagnetic activity. More specifically, it measures how disturbed Earth's magnetic field is, as observed by a network of ground-based magnetometer stations distributed across the planet at mid-latitudes (roughly 44° to 60° magnetic latitude).

Each station computes a local K index every three hours by measuring the range of variation in the horizontal component of the magnetic field. The range is compared to a baseline for that station and converted to a quasi-logarithmic scale from 0 to 9. The planet-wide Kp is then derived by averaging the K indices from the contributing stations, with a weighting scheme to account for geographic distribution.

The three-hour averaging period is both a strength and a limitation. It smooths out short-duration disturbances that don't represent sustained storm conditions. It also means Kp lags real-time conditions by up to three hours.

NOAA also publishes an estimated real-time Kp (updated every minute) derived from a slightly different algorithm using a smaller network of stations. This is the value Astraeusio ingests and displays at one-minute resolution. It's noisier than the official three-hour product but much more timely.

## The Scale: 0 to 9

The Kp scale runs from 0 to 9, in steps of one-third: 0, 0.33, 0.67, 1, 1.33, and so on up to 9. In practice you'll often see this written as 0o, 0+, 1-, 1o, 1+, and so on — the o/+/- notation indicates the third within each integer step.

The scale is quasi-logarithmic. A Kp of 5 corresponds to roughly three times the magnetic variation of Kp 4, not just one unit more. This means comparisons at the high end of the scale are particularly significant.

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

G1 begins at Kp 5. G5 requires sustained Kp 9. The G scale is specifically designed for communicating operational impact — each level comes with a standardised description of effects on power systems, spacecraft operations, radio propagation, and aurora visibility.

## Aurora Visibility

Aurora visibility is one of the most concrete ways the Kp index manifests for most people. The relationship between Kp and the equatorward boundary of the auroral oval is well established.

At Kp 5, aurora can be seen at magnetic latitudes of around 60° — roughly the latitude of Fairbanks, Alaska, or Tromsø, Norway. At Kp 7, that boundary drops to around 50°, covering much of Canada, Scandinavia, and northern Germany. At Kp 9, the oval expands to 40° and below — low enough to see aurora from New York, Rome, or Beijing, which happens only a few times per solar cycle.

The 1989 Quebec blackout (caused by a G5 storm, Kp 9) was accompanied by aurora visible as far south as Texas and Florida. The 2003 Halloween storms produced aurora visible from the southern United States and parts of Europe well outside typical aurora zones.

## What Kp Doesn't Tell You

Kp is a useful summary statistic, but it compresses a lot of information into a single number.

**Direction matters.** The primary driver of geomagnetic storm intensity isn't the magnitude of the solar wind magnetic field — it's the southward component (negative Bz). When the interplanetary magnetic field (IMF) points southward, it can couple efficiently with Earth's northward-pointing magnetic field through magnetic reconnection. The Kp index reflects the outcome of this process, but doesn't tell you which factor drove it.

**Duration matters.** A sustained Kp 7 for six hours is far more damaging to infrastructure than a brief Kp 7 spike followed by recovery. The Dst index captures this better — it measures the total energy injected into the ring current over the storm's lifetime.

**Recovery phase matters.** After the peak of a storm, Kp can drop sharply while the ring current — and its associated effects on the radiation belts — takes hours to days to dissipate.

For most purposes, Kp is the right number to watch. For a deeper operational picture, pair it with solar wind speed, IMF Bz, and Dst index together.
    `.trim(),
    contentTr: `
Kp endeksi, uzay hava durumunda en sık atıfta bulunulan sayıdır. NOAA fırtına uyarılarında, uydu operatör brifinglerinde, aurora tahmin uygulamalarında ve amatör telsiz yayılım rehberlerinde görünür. Ancak tam olarak neyi ölçer, nasıl hesaplanır ve belirli bir değer aşağıdaki dünya için gerçekte ne anlama gelir?

## Kp'nin Ölçtüğü

Kp, jeomanyetik aktivitenin küresel bir ölçüsüdür. Daha spesifik olarak, Dünya'nın manyetik alanının ne kadar rahatsız olduğunu, gezegen genelinde orta enlemlerde (yaklaşık 44° ile 60° manyetik enlem) dağıtılmış yer tabanlı manyetometre istasyonları ağı tarafından gözlemlendiği şekliyle ölçer.

Her istasyon, manyetik alanın yatay bileşenindeki değişim aralığını ölçerek her üç saatte bir yerel K endeksini hesaplar. Aralık, o istasyonun temel çizgisiyle karşılaştırılır ve 0'dan 9'a yarı-logaritmik bir ölçeğe dönüştürülür. Gezegen genelindeki Kp, coğrafi dağılımı hesaba katmak için bir ağırlıklı şema ile katkıda bulunan istasyonlardan K endekslerinin ortalaması alınarak elde edilir.

Üç saatlik ortalama dönemi hem bir güç hem de bir sınırlamadır. Sürekli fırtına koşullarını temsil etmeyen kısa süreli rahatsızlıkları düzeltir. Aynı zamanda Kp'nin gerçek zamanlı koşulların üç saate kadar gerisinde kalabileceği anlamına gelir.

NOAA ayrıca daha küçük bir istasyon ağı kullanan biraz farklı bir algoritmadan türetilen tahmini gerçek zamanlı bir Kp yayımlar. Bu, Astraeusio'nun dakikalık çözünürlükte aldığı ve gösterdiği değerdir. Resmi üç saatlik üründen daha gürültülüdür ama çok daha zamanındadır.

## Ölçek: 0'dan 9'a

Kp ölçeği 0'dan 9'a, üçte birlik adımlarla çalışır: 0, 0,33, 0,67, 1, 1,33 ve 9'a kadar böyle devam eder. Pratikte bunu genellikle 0o, 0+, 1-, 1o, 1+ şeklinde yazılmış görürsünüz — o/+/- gösterimi her tam sayı adımı içindeki üçüncü dilimleri gösterir.

Ölçek yarı-logaritmiktir. Kp 5, Kp 4'ten yalnızca bir birim daha fazla değil, yaklaşık üç kat daha fazla manyetik değişime karşılık gelir. Bu, ölçeğin yüksek ucundaki karşılaştırmaların özellikle önemli olduğu anlamına gelir.

| Kp | Durum | Etki |
|----|-------|------|
| 0–1 | Sakin | Önemli aktivite yok |
| 2–3 | Kararsız | Küçük dalgalanmalar |
| 4 | Aktif | Yüksek enlemlerde aurora görünür (>60°) |
| 5 | G1 Hafif Fırtına | 60°+ enlemde aurora; küçük şebeke dalgalanmaları mümkün |
| 6 | G2 Orta Fırtına | 55°+ enlemde aurora; yüksek enlemlerde KF radyo bozulması |
| 7 | G3 Güçlü Fırtına | 50°+ enlemde aurora; aralıklı navigasyon hataları |
| 8 | G4 Şiddetli Fırtına | 45°+ enlemde aurora; yaygın KF bozulması; boru hattı korozyon akımları |
| 9 | G5 Aşırı Fırtına | 40° ve altında aurora; yaygın şebeke sorunları; uydu hasarı mümkün |

## G Ölçeği

NOAA, jeomanyetik fırtına sınıflandırması için temel olarak Kp endeksini kullanır (G ölçeği, G1'den G5'e). Bu, resmi fırtına izlemelerinde, uyarılarında ve alarmlarında göreceğiniz ölçektir.

G1 Kp 5'te başlar. G5, sürekli Kp 9 gerektirir. G ölçeği özellikle operasyonel etkiyi iletmek için tasarlanmıştır — her seviye güç sistemleri, uzay aracı operasyonları, radyo yayılımı ve aurora görünürlüğü üzerindeki etkiler için standartlaştırılmış bir açıklamayla gelir.

## Aurora Görünürlüğü

Aurora görünürlüğü, Kp endeksinin çoğu insan için en somut tezahürüdür. Kp ile auroral ovalın güney sınırı arasındaki ilişki iyi kurulmuştur.

Kp 5'te aurora yaklaşık 60° manyetik enlemi civarında görülebilir — kabaca Alaska'nın Fairbanks veya Norveç'in Tromsø'sunun enlemi. Kp 7'de bu sınır 50° civarına düşer; Kanada'nın çoğunu, İskandinavya'yı ve kuzey Almanya'yı kapsar. Kp 9'da oval 40° ve altına genişler — yalnızca birkaç güneş döngüsü başına bir kez gerçekleşen New York, Roma veya Pekin'den aurora görmek için yeterince güney.

1989 Quebec karartması (G5 fırtınası, Kp 9), Texas ve Florida'ya kadar güneyde görülen aurora eşliğinde gerçekleşti. 2003 Halloween fırtınaları, tipik aurora kuşaklarının çok dışında olan güney Amerika Birleşik Devletleri ve Avrupa'nın bazı bölgelerinde aurora üretmişti.

## Kp'nin Söylemediği

Kp kullanışlı bir özet istatistiktir ancak tek bir sayıya çok fazla bilgi sıkıştırır.

**Yön önemlidir.** Jeomanyetik fırtına yoğunluğunun birincil sürücüsü güneş rüzgarı manyetik alanının büyüklüğü değil — güney bileşenidir (negatif Bz). IMF güneye döndüğünde manyetik yeniden bağlanma yoluyla Dünya'nın kuzeye dönük manyetik alanıyla etkin şekilde bağlanabilir. Kp endeksi bu sürecin sonucunu yansıtır ancak hangi faktörün onu yönlendirdiğini söylemez.

**Süre önemlidir.** Altı saat boyunca süregelen Kp 7, kısa bir Kp 7 zirvesinin ardından toparlanmadan altyapı için çok daha zararlıdır. Dst endeksi bunu daha iyi yakalar — fırtınanın ömrü boyunca halka akımına enjekte edilen toplam enerjiyi ölçer.

**İyileşme aşaması önemlidir.** Fırtınanın zirvesinden sonra Kp keskin şekilde düşebilirken halka akımı saatlerden günlere kadar dağılmaya devam eder.

Çoğu amaç için izlenecek doğru sayı Kp'dir. Daha derin bir operasyonel tablo için güneş rüzgarı hızı, IMF Bz ve Dst endeksiyle birlikte değerlendirin.
    `.trim(),
  },

  {
    slug: 'reading-the-solar-wind',
    title: 'Reading the Solar Wind',
    titleTr: 'Güneş Rüzgârını Okumak',
    date: '2026-05-08',
    author: 'Altug Tatlisu',
    tags: ['Explainer', 'Solar Wind', 'DSCOVR'],
    image: 'https://svs.gsfc.nasa.gov/vis/a010000/a014800/a014892/14892_CMEstrikesEarth_1080.00001_print.jpg',
    excerpt:
      'Kp tells you a storm has arrived. The solar wind tells you it is coming. Speed, density, and the magnetic field carried with it are the upstream signal — and one number decides whether a storm happens at all.',
    excerptTr:
      'Kp size bir fırtınanın geldiğini söyler. Güneş rüzgârı ise yaklaştığını söyler. Hız, yoğunluk ve taşıdığı manyetik alan akış-üstü sinyaldir — ve tek bir sayı fırtınanın gerçekleşip gerçekleşmeyeceğini belirler.',
    content: `
The Kp index tells you that a geomagnetic storm has arrived. The solar wind tells you that one is on its way. It is the upstream signal — measured before the disturbance reaches the ground — and reading it correctly is the difference between reacting to a storm and anticipating one.

## What the Solar Wind Is

The Sun continuously sheds its outer atmosphere into space. This is the solar wind: a stream of charged particles — mostly protons and electrons — flowing outward in every direction at hundreds of kilometres per second. It carries with it a piece of the Sun's magnetic field, stretched out across the solar system. By the time it reaches Earth, roughly 150 million kilometres away, it has thinned to a handful of particles per cubic centimetre, but it never stops.

Earth sits inside this wind. Our magnetic field deflects most of it, carving out the magnetosphere — a protective cavity that the solar wind drapes around and trails behind. Space weather, at its core, is the story of how variations in that wind disturb the magnetosphere.

## The Three Numbers That Matter

Astraeusio ingests one-minute solar wind data from NOAA's real-time feed. Three quantities carry almost all of the operational signal:

- **Speed** (km/s). Quiet conditions run around 300–400 km/s. A fast stream can exceed 700 km/s, and the leading edge of a coronal mass ejection can top 1,000 km/s. Higher speed means more energy delivered to the magnetosphere per second.
- **Density** (protons/cm³). Typically a few particles per cubic centimetre. A sudden jump in density often marks the arrival of a CME's compressed leading edge.
- **IMF Bz** (nanotesla). The north–south component of the interplanetary magnetic field. This is the one that decides whether a storm happens at all.

Speed and density set how much energy is available. Bz sets whether that energy can get in.

## Why Southward Bz Is the Real Driver

Earth's magnetic field points roughly northward at the dayside boundary of the magnetosphere. When the magnetic field carried by the solar wind points **southward** — negative Bz — it is antiparallel to Earth's field, and the two can splice together in a process called magnetic reconnection. Reconnection opens the magnetosphere, letting solar wind energy pour in. The more strongly southward the Bz and the longer it stays there, the bigger the storm.

When Bz points northward, reconnection at the dayside is suppressed and the magnetosphere stays comparatively closed. You can have a fast, dense solar wind stream slam into Earth and produce only a modest disturbance — because Bz stayed positive. Conversely, a moderate stream with strongly negative Bz for several hours can drive a serious storm.

This is why a single number — solar wind speed — is never enough. A 600 km/s stream with Bz at +5 nT is a non-event. The same stream with Bz at −15 nT is a G2 or G3 storm in the making.

## The Warning Window: L1

The reason the solar wind is a *forecast* tool and not just a *nowcast* is geometry. NOAA's DSCOVR spacecraft sits at the first Sun–Earth Lagrange point (L1), about 1.5 million kilometres upstream of Earth — roughly 1% of the way to the Sun. It measures the wind before it reaches us.

At typical solar wind speeds, that buys 15 to 60 minutes of warning. It is not much, but it is enough to safe a sensitive instrument, delay a satellite manoeuvre, or alert grid operators that strongly southward Bz is inbound. When you see Bz swing hard negative in the DSCOVR feed, you know the magnetosphere is about to respond — before Kp moves.

## Fast Streams and Corotating Interaction Regions

Not all disturbances come from CMEs. Coronal holes — regions where the Sun's magnetic field opens to space — emit persistent high-speed streams. Because the Sun rotates roughly every 27 days, these streams sweep past Earth on a recurring schedule, producing what forecasters call corotating interaction regions (CIRs). They rarely cause severe storms, but they reliably elevate Kp to the G1–G2 range and can recur for several solar rotations. If you see a moderate storm arrive on a 27-day cadence, a coronal hole is usually the cause.

## What Astraeusio Shows

The Charts page plots solar wind speed, proton density, and IMF Bz together, alongside the Dst index. Reading them as a set is the point: speed and density tell you the energy budget, Bz tells you the gate, and Dst tells you how much has already been deposited into the ring current. The anomaly layer fires when speed crosses 700 km/s; sustained negative Bz is the leading indicator to watch underneath it.

## The Honest Caveat

L1 monitoring has a blind spot: it measures the wind at a single point. The magnetic structure that passes DSCOVR is not always identical to what hits Earth 1.5 million kilometres away, and Bz can rotate during transit. The warning is real but probabilistic. Treat a hard southward turn at L1 as a strong signal — not a guarantee — and weight it alongside the Kp forecast and the X-ray record.
    `.trim(),
    contentTr: `
Kp endeksi size bir jeomanyetik fırtınanın geldiğini söyler. Güneş rüzgârı ise birinin yolda olduğunu söyler. O, akış-üstü sinyaldir — rahatsızlık yere ulaşmadan önce ölçülür — ve onu doğru okumak, bir fırtınaya tepki vermekle onu öngörmek arasındaki farktır.

## Güneş Rüzgârı Nedir

Güneş, dış atmosferini sürekli olarak uzaya saçar. Bu, güneş rüzgârıdır: çoğunlukla proton ve elektronlardan oluşan, her yöne saniyede yüzlerce kilometre hızla dışarı akan yüklü parçacık akışı. Beraberinde Güneş'in manyetik alanının bir parçasını, güneş sistemine yayılmış halde taşır. Yaklaşık 150 milyon kilometre uzaktaki Dünya'ya ulaştığında santimetreküp başına birkaç parçacığa kadar seyrelmiştir, ama hiç durmaz.

Dünya bu rüzgârın içinde durur. Manyetik alanımız çoğunu saptırarak manyetosferi oluşturur — güneş rüzgârının etrafına sarıldığı ve arkasında kuyruk bıraktığı koruyucu bir boşluk. Uzay hava durumu, özünde, bu rüzgârdaki değişimlerin manyetosferi nasıl rahatsız ettiğinin hikâyesidir.

## Önemli Olan Üç Sayı

Astraeusio, NOAA'nın gerçek zamanlı beslemesinden dakikalık güneş rüzgârı verisi alır. Operasyonel sinyalin neredeyse tamamını üç büyüklük taşır:

- **Hız** (km/s). Sakin koşullar 300–400 km/s civarındadır. Hızlı bir akış 700 km/s'yi aşabilir ve bir koronal kütle atımının (CME) ön kenarı 1.000 km/s'yi geçebilir. Daha yüksek hız, saniye başına manyetosfere iletilen daha fazla enerji demektir.
- **Yoğunluk** (proton/cm³). Tipik olarak santimetreküp başına birkaç parçacık. Yoğunluktaki ani sıçrama çoğu zaman bir CME'nin sıkışmış ön kenarının gelişini işaret eder.
- **IMF Bz** (nanotesla). Gezegenlerarası manyetik alanın kuzey–güney bileşeni. Bir fırtınanın gerçekleşip gerçekleşmeyeceğine karar veren budur.

Hız ve yoğunluk ne kadar enerji bulunduğunu belirler. Bz ise o enerjinin içeri girip giremeyeceğini belirler.

## Neden Güneye Dönük Bz Asıl Sürücüdür

Dünya'nın manyetik alanı, manyetosferin gündüz tarafı sınırında kabaca kuzeye doğrultulur. Güneş rüzgârının taşıdığı manyetik alan **güneye** doğrultulduğunda — negatif Bz — Dünya'nın alanına ters paraleldir ve ikisi manyetik yeniden bağlanma denen bir süreçte birbirine eklenebilir. Yeniden bağlanma manyetosferi açar ve güneş rüzgârı enerjisinin içeri akmasına izin verir. Bz ne kadar güçlü güneye dönükse ve orada ne kadar uzun kalırsa fırtına o kadar büyük olur.

Bz kuzeye doğrultulduğunda gündüz tarafındaki yeniden bağlanma baskılanır ve manyetosfer görece kapalı kalır. Hızlı ve yoğun bir güneş rüzgârı akışı Dünya'ya çarpıp yalnızca mütevazı bir rahatsızlık üretebilir — çünkü Bz pozitif kaldı. Tersine, birkaç saat boyunca güçlü negatif Bz'ye sahip ılımlı bir akış ciddi bir fırtına sürebilir.

İşte bu yüzden tek bir sayı — güneş rüzgârı hızı — asla yeterli değildir. Bz +5 nT iken 600 km/s'lik bir akış önemsizdir. Aynı akış Bz −15 nT iken oluşmakta olan bir G2 ya da G3 fırtınasıdır.

## Uyarı Penceresi: L1

Güneş rüzgârının yalnızca bir *anlık-durum* değil bir *tahmin* aracı olmasının nedeni geometridir. NOAA'nın DSCOVR uzay aracı, Dünya'nın yaklaşık 1,5 milyon kilometre akış-üstünde — Güneş'e giden yolun kabaca %1'inde — birinci Güneş–Dünya Lagrange noktasında (L1) durur. Rüzgârı bize ulaşmadan önce ölçer.

Tipik güneş rüzgârı hızlarında bu 15 ila 60 dakikalık uyarı kazandırır. Çok değil, ama hassas bir cihazı güvene almaya, bir uydu manevrasını ertelemeye veya şebeke operatörlerini güçlü güneye dönük Bz'nin yaklaştığı konusunda uyarmaya yeter. DSCOVR beslemesinde Bz'nin sert biçimde negatife döndüğünü gördüğünüzde, manyetosferin yanıt vermek üzere olduğunu Kp hareket etmeden önce bilirsiniz.

## Hızlı Akışlar ve Birlikte Dönen Etkileşim Bölgeleri

Tüm rahatsızlıklar CME'lerden gelmez. Koronal delikler — Güneş'in manyetik alanının uzaya açıldığı bölgeler — kalıcı yüksek hızlı akışlar yayar. Güneş yaklaşık her 27 günde bir döndüğünden, bu akışlar Dünya'nın yanından yinelenen bir programla geçer ve tahmincilerin birlikte dönen etkileşim bölgeleri (CIR) dediği şeyi üretir. Nadiren şiddetli fırtınalara yol açarlar ama Kp'yi güvenilir biçimde G1–G2 aralığına yükseltir ve birkaç güneş dönüşü boyunca yinelenebilirler. Ilımlı bir fırtınanın 27 günlük bir ritimde geldiğini görürseniz, neden genellikle bir koronal deliktir.

## Astraeusio Ne Gösterir

Grafikler sayfası güneş rüzgârı hızını, proton yoğunluğunu ve IMF Bz'yi Dst endeksiyle birlikte çizer. Onları bir bütün olarak okumak işin özüdür: hız ve yoğunluk enerji bütçesini, Bz kapıyı, Dst ise halka akımına şimdiden ne kadar biriktirildiğini söyler. Anomali katmanı hız 700 km/s'yi geçtiğinde tetiklenir; sürekli negatif Bz ise altında izlenmesi gereken öncü göstergedir.

## Dürüst Uyarı

L1 izlemesinin bir kör noktası vardır: rüzgârı tek bir noktada ölçer. DSCOVR'dan geçen manyetik yapı, 1,5 milyon kilometre öteden Dünya'ya çarpana her zaman aynı olmaz ve Bz geçiş sırasında dönebilir. Uyarı gerçektir ama olasılıksaldır. L1'deki sert güneye dönüşü güçlü bir sinyal olarak değerlendirin — bir garanti değil — ve onu Kp tahmini ile X-ışını kaydının yanında tartın.
    `.trim(),
  },

  {
    slug: 'x-ray-flares-and-radio-blackouts',
    title: 'X-ray Flares and Radio Blackouts',
    titleTr: 'X-ışını Patlamaları ve Radyo Karartmaları',
    date: '2026-05-15',
    author: 'Altug Tatlisu',
    tags: ['Explainer', 'Solar Flares', 'X-ray'],
    image: 'https://svs.gsfc.nasa.gov/vis/a000000/a004400/a004491/Sept2017_X8Flare_304A.07800_print.jpg',
    excerpt:
      'A solar flare arrives at the speed of light — eight minutes from Sun to Earth, with no warning. Here is how the GOES X-ray sensor classifies flares, what the A/B/C/M/X scale means, and why the radio blackout is instant.',
    excerptTr:
      'Bir güneş patlaması ışık hızında gelir — Güneş\'ten Dünya\'ya sekiz dakika, hiçbir uyarı olmadan. GOES X-ışını sensörünün patlamaları nasıl sınıflandırdığını, A/B/C/M/X ölçeğinin ne anlama geldiğini ve radyo karartmasının neden anlık olduğunu anlatıyoruz.',
    content: `
Most space weather gives you warning. A coronal mass ejection takes one to three days to cross from the Sun to Earth. A fast solar wind stream announces itself at L1 with up to an hour to spare. A solar flare gives you nothing. It travels at the speed of light, which means it arrives about eight minutes after it happens — the same eight minutes it takes the Sun's light to reach us. By the time you detect it, its effects are already underway.

## What a Flare Is

A solar flare is a sudden, intense release of energy from the Sun's atmosphere, triggered when twisted magnetic fields in an active region snap into a lower-energy configuration. That reconnection dumps enormous energy into the surrounding plasma in minutes, heating it to tens of millions of degrees and emitting a burst of radiation across the spectrum — radio waves, visible light, ultraviolet, and X-rays.

The X-ray component is what we monitor most closely, because it is both a clean measure of flare intensity and the direct cause of one of the flare's most immediate effects on Earth.

## The GOES X-ray Sensor

Flare intensity is measured by the X-Ray Sensor (XRS) aboard NOAA's GOES satellites in geostationary orbit. It reports the solar X-ray flux in the 0.1–0.8 nanometre band, in watts per square metre, updated continuously. Astraeusio ingests the GOES primary feed and stores the flux as a scaled integer; the dashboard surfaces the current flux and its flare class.

Because the sensor sits above the atmosphere, it sees the X-rays directly — there is no transit delay beyond the eight-minute light travel time from the Sun.

## The Flare Classification: A, B, C, M, X

Flares are classified by their peak X-ray flux on a letter scale, where each letter is a tenfold (logarithmic) step:

| Class | Peak flux (W/m²) | Meaning |
|-------|------------------|---------|
| A | < 10⁻⁷ | Background level; no effect |
| B | 10⁻⁷ – 10⁻⁶ | Minor; common, no impact |
| C | 10⁻⁶ – 10⁻⁵ | Small flares, few noticeable effects |
| M | 10⁻⁵ – 10⁻⁴ | Medium; can cause brief radio blackouts at the poles |
| X | ≥ 10⁻⁴ | Large; planet-wide radio and navigation effects |

Within each letter, a number gives the linear multiplier — an M5 is five times stronger than an M1, and X-class has no ceiling: the September 2017 event reached X9.3, and the famous 2003 flare saturated the sensors at roughly X28.

Astraeusio's anomaly layer flags M-class flares as a warning and X-class as critical, matching the thresholds at which terrestrial effects become operationally relevant.

## Radio Blackouts and the R-Scale

The most immediate terrestrial effect of a flare is a radio blackout. The burst of X-rays ionises the dayside of Earth's upper atmosphere (the D-layer of the ionosphere), which then absorbs high-frequency (HF) radio signals instead of reflecting them. HF communication — used by aviation over oceans and polar routes, by maritime operators, and by emergency services — degrades or drops out entirely on the sunlit side of the planet.

NOAA classifies these on the R-scale, tied directly to flare class:

| Scale | Flare | Effect |
|-------|-------|--------|
| R1 Minor | M1 | Weak HF degradation on sunlit side |
| R2 Moderate | M5 | Limited HF blackout, low-frequency navigation degraded |
| R3 Strong | X1 | Wide-area HF blackout for ~1 hour, navigation errors |
| R4 Severe | X10 | HF blackout across most of the sunlit hemisphere |
| R5 Extreme | X20 | Complete HF blackout on the entire daylit side for hours |

Because the cause is electromagnetic radiation travelling at light speed, the blackout begins the moment the flare's X-rays arrive. There is no forecast window for the flare itself — only for the active region's *likelihood* of producing one.

## Flares Versus CMEs

A flare and a coronal mass ejection often originate from the same active region, but they are different hazards with different timelines. The flare is radiation: it arrives in eight minutes and causes radio and navigation effects. The CME is matter: a cloud of magnetised plasma that takes one to three days to arrive and drives the geomagnetic storm — the Kp spike, the aurora, the grid currents.

A big flare does not guarantee a big storm, and vice versa. A flare aimed away from Earth can still black out radio on the sunlit side; a CME with no notable flare can still trigger a severe storm if its Bz turns sharply southward. They are best read as separate channels of the same event.

## What to Watch in Astraeusio

The Charts page plots GOES X-ray flux on a logarithmic axis, with the M and X thresholds marked. A flux line climbing toward 10⁻⁵ is an M-class flare in progress; crossing 10⁻⁴ is X-class. Sudden vertical spikes are the signature. Pair the X-ray record with the alerts feed: NOAA issues radio-blackout warnings keyed to the same flare classes, and the anomaly panel flags M and X events as they cross threshold.

## The Honest Assessment

You cannot forecast the exact onset of a flare — the physics of magnetic reconnection is not predictable minute-to-minute. What you can do is watch active regions: large, magnetically complex sunspot groups produce most major flares, and NOAA tracks them. The X-ray record will not warn you before a flare, but it gives you an instant, unambiguous measure of one in progress — and that is enough to explain a sudden HF blackout, time-stamp it, and brief anyone whose operations just lost a radio link.
    `.trim(),
    contentTr: `
Çoğu uzay hava olayı size uyarı verir. Bir koronal kütle atımının Güneş'ten Dünya'ya geçmesi bir ila üç gün sürer. Hızlı bir güneş rüzgârı akışı kendini L1'de bir saate varan payla haber verir. Bir güneş patlaması ise size hiçbir şey vermez. Işık hızında ilerler; bu da olduktan yaklaşık sekiz dakika sonra geldiği anlamına gelir — Güneş'in ışığının bize ulaşması için gereken aynı sekiz dakika. Onu tespit ettiğinizde etkileri çoktan başlamıştır.

## Patlama Nedir

Güneş patlaması, bir aktif bölgedeki bükülmüş manyetik alanların daha düşük enerjili bir düzene aniden geçmesiyle tetiklenen, Güneş atmosferinden ani ve yoğun bir enerji salımıdır. Bu yeniden bağlanma, çevredeki plazmaya dakikalar içinde muazzam enerji boşaltır, onu on milyonlarca dereceye ısıtır ve tayf boyunca bir radyasyon patlaması yayar — radyo dalgaları, görünür ışık, morötesi ve X-ışınları.

X-ışını bileşeni en yakından izlediğimiz şeydir, çünkü hem patlama yoğunluğunun temiz bir ölçüsü hem de patlamanın Dünya üzerindeki en anlık etkilerinden birinin doğrudan nedenidir.

## GOES X-ışını Sensörü

Patlama yoğunluğu, NOAA'nın jeostasyoner yörüngedeki GOES uydularındaki X-ışını Sensörü (XRS) tarafından ölçülür. Güneş X-ışını akısını 0,1–0,8 nanometre bandında, metrekare başına watt cinsinden, sürekli güncellenerek bildirir. Astraeusio GOES birincil beslemesini alır ve akıyı ölçekli bir tam sayı olarak saklar; gösterge paneli mevcut akıyı ve patlama sınıfını gösterir.

Sensör atmosferin üzerinde durduğundan X-ışınlarını doğrudan görür — Güneş'ten gelen sekiz dakikalık ışık yolculuğu süresinin ötesinde bir geçiş gecikmesi yoktur.

## Patlama Sınıflandırması: A, B, C, M, X

Patlamalar, her harfin on katlık (logaritmik) bir adım olduğu bir harf ölçeğinde tepe X-ışını akısına göre sınıflandırılır:

| Sınıf | Tepe akı (W/m²) | Anlamı |
|-------|-----------------|--------|
| A | < 10⁻⁷ | Arka plan seviyesi; etki yok |
| B | 10⁻⁷ – 10⁻⁶ | Küçük; yaygın, etkisiz |
| C | 10⁻⁶ – 10⁻⁵ | Küçük patlamalar, az fark edilir etki |
| M | 10⁻⁵ – 10⁻⁴ | Orta; kutuplarda kısa radyo karartmalarına yol açabilir |
| X | ≥ 10⁻⁴ | Büyük; gezegen geneli radyo ve navigasyon etkileri |

Her harfin içinde bir sayı doğrusal çarpanı verir — bir M5, bir M1'den beş kat güçlüdür ve X sınıfının tavanı yoktur: Eylül 2017 olayı X9.3'e ulaştı ve ünlü 2003 patlaması sensörleri kabaca X28'de doyurdu.

Astraeusio'nun anomali katmanı M sınıfı patlamaları uyarı, X sınıfını ise kritik olarak işaretler; bu, karasal etkilerin operasyonel olarak anlamlı hale geldiği eşiklere karşılık gelir.

## Radyo Karartmaları ve R Ölçeği

Bir patlamanın en anlık karasal etkisi radyo karartmasıdır. X-ışını patlaması Dünya'nın üst atmosferinin gündüz tarafını (iyonosferin D katmanı) iyonlaştırır; bu katman daha sonra yüksek frekanslı (HF) radyo sinyallerini yansıtmak yerine soğurur. Okyanuslar ve kutup rotaları üzerinde havacılık tarafından, deniz operatörleri tarafından ve acil servisler tarafından kullanılan HF iletişimi, gezegenin güneş gören tarafında bozulur veya tamamen kesilir.

NOAA bunları doğrudan patlama sınıfına bağlı R ölçeğinde sınıflandırır:

| Ölçek | Patlama | Etki |
|-------|---------|------|
| R1 Hafif | M1 | Güneş gören tarafta zayıf HF bozulması |
| R2 Orta | M5 | Sınırlı HF karartması, düşük frekanslı navigasyon bozulur |
| R3 Güçlü | X1 | ~1 saat geniş alan HF karartması, navigasyon hataları |
| R4 Şiddetli | X10 | Güneş gören yarıkürenin çoğunda HF karartması |
| R5 Aşırı | X20 | Tüm gündüz tarafında saatlerce tam HF karartması |

Neden ışık hızında ilerleyen elektromanyetik radyasyon olduğundan, karartma patlamanın X-ışınları geldiği anda başlar. Patlamanın kendisi için bir tahmin penceresi yoktur — yalnızca aktif bölgenin bir patlama üretme *olasılığı* için vardır.

## Patlamalar ile CME'ler

Bir patlama ve bir koronal kütle atımı çoğu zaman aynı aktif bölgeden kaynaklanır ama farklı zaman çizelgelerine sahip farklı tehlikelerdir. Patlama radyasyondur: sekiz dakikada gelir ve radyo ile navigasyon etkilerine yol açar. CME ise maddedir: gelmesi bir ila üç gün süren ve jeomanyetik fırtınayı süren manyetize plazma bulutu — Kp sıçraması, aurora, şebeke akımları.

Büyük bir patlama büyük bir fırtınayı garanti etmez, tersi de geçerlidir. Dünya'dan uzağa yönelmiş bir patlama yine de güneş gören tarafta radyoyu karartabilir; kayda değer patlaması olmayan bir CME, Bz'si sertçe güneye dönerse yine de şiddetli bir fırtına tetikleyebilir. En iyisi onları aynı olayın ayrı kanalları olarak okumaktır.

## Astraeusio'da Nelere Dikkat Edilmeli

Grafikler sayfası GOES X-ışını akısını, M ve X eşikleri işaretlenmiş halde logaritmik bir eksende çizer. 10⁻⁵'e doğru tırmanan bir akı çizgisi devam eden bir M sınıfı patlamadır; 10⁻⁴'ü geçmek X sınıfıdır. Ani dikey sıçramalar onun imzasıdır. X-ışını kaydını uyarı beslemesiyle eşleştirin: NOAA aynı patlama sınıflarına bağlı radyo karartma uyarıları yayımlar ve anomali paneli M ve X olaylarını eşiği geçerken işaretler.

## Dürüst Değerlendirme

Bir patlamanın tam başlangıcını tahmin edemezsiniz — manyetik yeniden bağlanmanın fiziği dakika dakika öngörülebilir değildir. Yapabileceğiniz şey aktif bölgeleri izlemektir: büyük, manyetik olarak karmaşık güneş lekesi grupları büyük patlamaların çoğunu üretir ve NOAA onları takip eder. X-ışını kaydı bir patlamadan önce sizi uyarmaz, ama devam eden bir patlamanın anlık ve net bir ölçüsünü verir — bu da ani bir HF karartmasını açıklamaya, zaman damgası vurmaya ve operasyonu az önce bir radyo bağlantısını kaybeden herkesi bilgilendirmeye yeter.
    `.trim(),
  },

  {
    slug: 'the-carrington-event',
    title: 'The Carrington Event: Space Weather\'s Worst Case',
    titleTr: 'Carrington Olayı: Uzay Hava Durumunun En Kötü Senaryosu',
    date: '2026-05-22',
    author: 'Altug Tatlisu',
    tags: ['History', 'Space Weather', 'Risk'],
    image: 'https://images-assets.nasa.gov/image/GSFC_20171208_Archive_e001661/GSFC_20171208_Archive_e001661~large.jpg',
    excerpt:
      'In 1859 a solar storm set telegraph offices on fire and lit aurora over the tropics. We have built a civilisation on the grid and satellites since. Here is what a Carrington-class event would do today — and whether we would see it coming.',
    excerptTr:
      '1859\'da bir güneş fırtınası telgraf ofislerini ateşe verdi ve tropiklerin üzerinde aurora yaktı. O zamandan beri şebeke ve uydular üzerine bir uygarlık inşa ettik. Carrington ölçeğinde bir olayın bugün ne yapacağını — ve onu önceden görüp göremeyeceğimizi anlatıyoruz.',
    content: `
On the morning of 1 September 1859, the English astronomer Richard Carrington was sketching sunspots when he saw two brilliant beads of white light erupt over a large sunspot group. They faded within minutes. He had just witnessed the first solar flare ever recorded — and the leading edge of the most intense geomagnetic storm in recorded history.

## What Happened in 1859

The flare Carrington saw was followed by a coronal mass ejection that reached Earth in about 17 hours — extraordinarily fast, because an earlier CME had cleared the path through the solar wind. When it arrived, it produced effects no one had a framework to explain.

Aurora were seen as far south as the Caribbean, Colombia, and Hawaii — within roughly 23° of the equator. In the northern United States, the light was bright enough to read a newspaper by at midnight. Gold miners in the Rocky Mountains reportedly woke and began making breakfast, thinking it was dawn.

The one piece of electrical infrastructure that existed — the telegraph network — failed spectacularly. Operators received electric shocks. Telegraph paper caught fire. And in a detail that still unsettles engineers: some operators disconnected their batteries entirely and found they could still send messages, powered purely by the current the storm had induced in the wires.

## How Big Was It

The storm's intensity is estimated from the few magnetometer records that existed and from later ice-core analysis. The Dst index — a measure of the storm-time ring current — is estimated to have reached somewhere between −850 and −1,760 nanotesla. For comparison, the 1989 storm that collapsed the Quebec power grid reached about −589 nT, and a "severe" G4 storm today is in the −200 to −300 nT range.

Carrington was not merely a strong storm. It was several times larger than anything the modern grid has ever experienced.

## The 2012 Near Miss

It is tempting to file Carrington under "nineteenth-century history." On 23 July 2012, that comfort evaporated. A CME at least as powerful as the 1859 event erupted from the Sun at over 3,000 km/s — and crossed the orbit of Earth. Earth simply was not in the way; the eruption region had rotated past the Earth-facing line about nine days earlier. The CME swept through the position Earth had occupied roughly one week before.

The STEREO-A spacecraft happened to be there and measured it directly. Analyses afterward concluded it was Carrington-class. A 2013 study put the probability of a Carrington-level storm hitting Earth in the following decade at roughly 12%. The event is not a historical curiosity; it is a recurring natural hazard that we have mostly been lucky to avoid.

## What It Would Do Today

A Carrington-class storm striking the modern world would stress systems that did not exist in 1859:

- **Power grids.** Geomagnetically induced currents flow through long transmission lines and can saturate and overheat the large transformers at the heart of the grid. These transformers are custom-built, cost millions, and can take a year or more to replace. The 1989 Quebec storm blacked out six million people in 90 seconds; a Carrington-class event could damage transformers across continents.
- **Satellites.** Severe charging, accelerated drag, and single-event upsets across a fleet far larger and more economically central than anything aloft in 1989.
- **Navigation and timing.** GPS provides not just position but the precise timing that synchronises financial transactions, telecom networks, and the grid itself. Severe ionospheric disturbance degrades it.
- **Aviation.** Polar flights reroute to maintain communication and limit radiation exposure, at significant cost.

Economic impact estimates for a severe, prolonged event range into the trillions of dollars, with recovery measured in months to years where transformers are lost. The figures are uncertain by design — we have never run the experiment on a grid this interconnected.

## Could We See It Coming?

Better than 1859, but the warning is uneven. A flare's radiation arrives in eight minutes — effectively no warning. The CME that drives the geomagnetic storm takes one to three days to cross from the Sun, and coronagraphs like those on SOHO can spot it leaving the Sun, giving a day or more of lead time to estimate arrival.

The decisive measurement comes last and latest: the CME's magnetic orientation — its Bz — cannot be reliably known until it reaches DSCOVR at L1, 15 to 60 minutes upstream. A Carrington-class CME with northward Bz would be a dramatic near-miss; the same CME with strongly southward Bz would be the worst day in the history of the electrical grid. We do not know which until the final hour.

That final hour is the entire point of real-time monitoring. Grid operators can shed load and reconfigure; satellite operators can safe their fleets; airlines can reroute. None of it is possible without continuous, low-latency data.

## Why This Matters for a Monitoring Platform

Carrington is the reason space weather is treated as critical infrastructure rather than a curiosity. Astraeusio ingests the same upstream signals that would call the warning — solar wind speed and density, IMF Bz, GOES X-ray flux, the Kp index and its forecast — and surfaces them continuously, with anomaly detection on the thresholds that matter. Most storms are minor. The job of monitoring is to be already watching, with history as the baseline, on the day one is not.
    `.trim(),
    contentTr: `
1 Eylül 1859 sabahı, İngiliz gökbilimci Richard Carrington güneş lekeleri çizerken büyük bir güneş lekesi grubunun üzerinde iki parlak beyaz ışık boncuğunun patladığını gördü. Dakikalar içinde söndüler. Az önce kaydedilen ilk güneş patlamasına — ve kayıtlı tarihteki en yoğun jeomanyetik fırtınanın ön kenarına — tanık olmuştu.

## 1859'da Ne Oldu

Carrington'ın gördüğü patlamayı, Dünya'ya yaklaşık 17 saatte ulaşan bir koronal kütle atımı izledi — olağanüstü hızlı, çünkü daha önceki bir CME güneş rüzgârındaki yolu temizlemişti. Geldiğinde, kimsenin açıklayacak bir çerçevesi olmayan etkiler üretti.

Aurora, Karayipler, Kolombiya ve Hawaii kadar güneyde — ekvatorun kabaca 23° yakınında — görüldü. Kuzey Amerika Birleşik Devletleri'nde ışık, gece yarısı gazete okumaya yetecek kadar parlaktı. Kayalık Dağlar'daki altın madencilerinin uyanıp şafak sandıkları için kahvaltı hazırlamaya başladıkları anlatılır.

Var olan tek elektrik altyapısı — telgraf ağı — muhteşem biçimde çöktü. Operatörler elektrik çarpmasına maruz kaldı. Telgraf kâğıdı tutuştu. Ve mühendisleri hâlâ tedirgin eden bir ayrıntı: bazı operatörler pillerini tamamen çıkardı ve fırtınanın tellerde indüklediği akımla, tamamen onunla beslenerek hâlâ mesaj gönderebildiklerini gördü.

## Ne Kadar Büyüktü

Fırtınanın yoğunluğu, var olan birkaç manyetometre kaydından ve sonraki buz çekirdeği analizinden tahmin edilir. Fırtına zamanı halka akımının bir ölçüsü olan Dst endeksinin −850 ile −1.760 nanotesla arasında bir yere ulaştığı tahmin edilir. Karşılaştırma için, 1989'da Quebec elektrik şebekesini çökerten fırtına yaklaşık −589 nT'ye ulaştı ve bugün "şiddetli" bir G4 fırtınası −200 ila −300 nT aralığındadır.

Carrington yalnızca güçlü bir fırtına değildi. Modern şebekenin yaşadığı her şeyden birkaç kat daha büyüktü.

## 2012 Kıl Payı Atlatma

Carrington'ı "on dokuzuncu yüzyıl tarihi" altına koymak cazip gelir. 23 Temmuz 2012'de bu rahatlık buharlaştı. En az 1859 olayı kadar güçlü bir CME, Güneş'ten 3.000 km/s'nin üzerinde bir hızla patladı — ve Dünya'nın yörüngesini geçti. Dünya yalnızca yolda değildi; patlama bölgesi yaklaşık dokuz gün önce Dünya'ya bakan çizgiden dönmüştü. CME, Dünya'nın kabaca bir hafta önce bulunduğu konumdan geçti.

STEREO-A uzay aracı orada bulunuyordu ve onu doğrudan ölçtü. Sonraki analizler bunun Carrington sınıfı olduğu sonucuna vardı. 2013'teki bir çalışma, sonraki on yılda Carrington seviyesinde bir fırtınanın Dünya'ya çarpma olasılığını kabaca %12 olarak koydu. Olay tarihsel bir merak değil; çoğunlukla kaçınmakta şanslı olduğumuz yinelenen bir doğal tehlikedir.

## Bugün Ne Yapardı

Modern dünyaya çarpan Carrington sınıfı bir fırtına, 1859'da var olmayan sistemleri zorlardı:

- **Elektrik şebekeleri.** Jeomanyetik olarak indüklenen akımlar uzun iletim hatları boyunca akar ve şebekenin kalbindeki büyük transformatörleri doyurup aşırı ısıtabilir. Bu transformatörler özel üretimdir, milyonlara mal olur ve değiştirilmesi bir yıl veya daha fazla sürebilir. 1989 Quebec fırtınası 90 saniyede altı milyon insanı karanlığa gömdü; Carrington sınıfı bir olay kıtalar boyunca transformatörlere zarar verebilir.
- **Uydular.** 1989'da gökyüzündeki her şeyden çok daha büyük ve ekonomik olarak çok daha merkezi bir filo genelinde şiddetli yüklenme, hızlanan sürükleme ve tek olay bozulmaları.
- **Navigasyon ve zamanlama.** GPS yalnızca konum değil, finansal işlemleri, telekom ağlarını ve şebekenin kendisini eşitleyen hassas zamanlamayı da sağlar. Şiddetli iyonosfer rahatsızlığı onu bozar.
- **Havacılık.** Kutup uçuşları iletişimi sürdürmek ve radyasyona maruziyeti sınırlamak için önemli maliyetle rota değiştirir.

Şiddetli ve uzun süreli bir olay için ekonomik etki tahminleri trilyonlarca dolara ulaşır; transformatörlerin kaybedildiği yerlerde iyileşme aylarla yıllarla ölçülür. Rakamlar doğası gereği belirsizdir — bu kadar birbirine bağlı bir şebeke üzerinde bu deneyi hiç yapmadık.

## Onu Önceden Görebilir miydik

1859'dan daha iyi, ama uyarı eşitsiz. Bir patlamanın radyasyonu sekiz dakikada gelir — etkili biçimde hiçbir uyarı yok. Jeomanyetik fırtınayı süren CME'nin Güneş'ten geçmesi bir ila üç gün sürer ve SOHO üzerindekiler gibi koronagraflar onu Güneş'ten ayrılırken görebilir; bu, varışı tahmin etmek için bir gün veya daha fazla öncü süre verir.

Belirleyici ölçüm en son ve en geç gelir: CME'nin manyetik yönelimi — Bz'si — 15 ila 60 dakika akış-üstündeki L1'de DSCOVR'a ulaşana kadar güvenilir biçimde bilinemez. Kuzeye dönük Bz'ye sahip Carrington sınıfı bir CME çarpıcı bir kıl payı atlatma olurdu; güçlü güneye dönük Bz'ye sahip aynı CME, elektrik şebekesi tarihindeki en kötü gün olurdu. Hangisi olduğunu son saate kadar bilemeyiz.

O son saat, gerçek zamanlı izlemenin tüm amacıdır. Şebeke operatörleri yük atabilir ve yeniden yapılandırabilir; uydu operatörleri filolarını güvene alabilir; havayolları rota değiştirebilir. Sürekli, düşük gecikmeli veri olmadan bunların hiçbiri mümkün değildir.

## Bunun Bir İzleme Platformu için Önemi

Carrington, uzay hava durumunun bir merak değil kritik altyapı olarak ele alınmasının nedenidir. Astraeusio, uyarıyı verecek aynı akış-üstü sinyalleri alır — güneş rüzgârı hızı ve yoğunluğu, IMF Bz, GOES X-ışını akısı, Kp endeksi ve tahmini — ve bunları önemli eşiklerde anomali tespitiyle birlikte sürekli olarak gösterir. Fırtınaların çoğu küçüktür. İzlemenin görevi, bir tanesinin küçük olmadığı gün, tarih temel çizgi olacak şekilde, çoktan izliyor olmaktır.
    `.trim(),
  },
]

export function getPosts(lang) {
  if (lang === 'tr') {
    return posts.map(p => ({
      ...p,
      title:   p.titleTr   ?? p.title,
      excerpt: p.excerptTr ?? p.excerpt,
      content: p.contentTr ?? p.content,
    }))
  }
  return posts
}

export function getPost(slug, lang) {
  const list = getPosts(lang)
  return list.find(p => p.slug === slug) ?? null
}
