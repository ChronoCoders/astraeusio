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
