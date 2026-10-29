# CPU schema 1: experimenteel meetcontract

Contract-ID: `cpu-v1-experimental-1`. Applicatieversie 0.1.0. Dit document beschrijft de huidige implementatie; empirische kalibratie moet nog gebeuren.

## Workloads

De vaste corpusgenerator produceert 65.536 bytes. SHA-256:

```text
fed880d6c2ee7e7c6bdeaf14b99a18fbaa8172e5574fa00e05d35a69f79a4a61
```

Integer gebruikt wrapping u64-aritmetiek, 32.768 iteraties en een 256-element lookup-table. De checksum is `4644254777524890854`. Dataprocessing leest 256 records, filtert op active, transformeert waarde/naam, sorteert op (waarde,id), maakt een BTreeMap, zoekt alle behouden records op en serialiseert. De FNV-1a-gebaseerde resultaatcontrole is `4610356586914610349`. Deze twee resultaten zijn onafhankelijk berekend met Python; het controlescript staat in `scripts/check-vectors.py`.

Compressie gebruikt `miniz_oxide 0.8.5`, raw DEFLATE level 6. Het gecomprimeerde resultaat wordt vergeleken met het bij startup voorbereide resultaat; de voorbereiding controleert een volledige roundtrip. Decompressie vergelijkt alle bytes met het corpus. SHA-256 gebruikt `sha2 0.10.8 force-soft`, zonder instructiespecifieke hardwareacceleratie. De correctness-controles, allocaties en resultaatvergelijkingen in de batches tellen mee in de workloadtijd. Geen randomness of I/O in de workloads.

## Timing en workerorkestratie

Iedere worker krijgt zijn eigen voorbereide data. Een barrier synchroniseert de start na initialisatie en affinity. Workers werken tot één gedeelde monotone deadline en maken de laatste batch af. De echte totale looptijd, counts en worker finish skew worden bewaard. Samplerobservaties hebben een werkelijke intervalduur; throughput gebruikt de delta van de voltooide batchcounters. Warm-up telt niet mee.

Iedere workload wordt eenmaal uitgevoerd met één worker, en eenmaal met het berekende aantal multi-workers. De statistische verdeling betreft de per-seconde-vensters binnen die uitvoering, niet onafhankelijke herhalingen. Voor onafhankelijke variatie gebruikt men meerdere benchmarkruns of survey. CPU-affinity wordt geprobeerd op de eerste toegestane CPU's, zonder SMT- of NUMA-optimalisatie. Een mislukte pin is zichtbaar in de JSON en waarschuwingen.

Een meetvenster van minder dan 90% van de nominale intervalduur wordt uitgesloten van throughputstatistieken. Het venster blijft wel in raw samples. Absolute minimum- of maximumdoorvoer hangt hierdoor mede af van de vensterduur; vergelijk dezelfde timingconfiguratie.

## Statistiek

Percentielen gebruiken lineaire interpolatie op positie `(n-1) × p` in de gesorteerde reeks (R-7). Standaarddeviatie is de populatiestandaarddeviatie; `CV = sigma / gemiddelde`, dus een fractie. Console-uitvoer vermenigvuldigt CV met 100 voor procenten. P50 en mediaan zijn gelijk.

De referentie normaliseert elke van de vijf workloads tegen één vaste throughput. Het geometrisch gemiddelde geeft alle vijf gelijke weging; compression en decompression zijn dus afzonderlijke onderdelen. De referentiewaarden staan in `reference/cpu-v1.json`; de indexschaal is 1000.

```text
single = 1000 × exp(mean(log(single_workload_median / reference_workload)))
multi  = 1000 × exp(mean(log(multi_workload_median / reference_workload)))
per_vcpu = multi / aantal_multi_workers
scaling_efficiency = multi_median / (single_median × aantal_multi_workers)

average  = 1000 × gemiddelde_sustained_cycles_per_s / reference_mixed_cycles_per_s
reliable = 1000 × P10_sustained_cycles_per_s / reference_mixed_cycles_per_s

cpu_index = exp(0.35 × log(single) + 0.40 × log(multi) + 0.25 × log(reliable))
```

Een geldige nulcomponent geeft een CPU Index van nul. Negatieve of niet-eindige inputs zijn ongeldig. Scaling efficiency is onbekend als de single-mediaan nul is. De index is geen foutmarge of hardware-onafhankelijke applicatievoorspelling.

## Consistency

De vroege en late sustained-secties bestaan elk uit floor(n/3) meetvensters, met minimum één. Degradation = max(0, 1 − late_mediaan / vroege_mediaan). Een outlier is een meetvenster onder 75% van de sustained-mediaan. De stealcomponent gebruikt het rekenkundig gemiddelde van beschikbare sustained stealpercentages.

```text
penalty = 0.35 × clamp(CV / 0.5, 0, 1)
        + 0.25 × clamp(1 − P10/mediaan, 0, 1)
        + 0.20 × clamp(degradation, 0, 1)
        + 0.10 × clamp(mean_steal_percent / 20, 0, 1)
        + 0.10 × clamp(outlier_fraction, 0, 1)
consistency = 100 × (1 − penalty)
```

Een nulmediaan geeft P10/mediaan=0. Ontbrekende steal levert geen stealpenalty; de input blijft `null`, zodat onbekend niet als bewijs voor geen contention wordt gepresenteerd. Vergelijk daarom ook de metricbeschikbaarheid. Labels: ≥97 excellent, ≥90 very good, ≥80 good, ≥70 variable, lager inconsistent. De drempels en gewichten zijn experimenteel en moeten in echte omgevingen worden gevalideerd.

## Linux runtime metrics

De sampler leest per toegestane logical CPU de eerste acht `/proc/stat`-velden: user, nice, system, idle, iowait, irq, softirq, steal. Guest en guest_nice worden niet toegevoegd omdat ze al in user/nice zitten. CPU-gebruik = (total − idle − iowait − steal)/total. Steal = steal/total. Counters die ontbreken, teruglopen of geen delta hebben leveren onbekende waarden op. Een geldige nul blijft nul. `/proc/stat` kan iowait laten teruglopen; dat maakt dat interval onbekend.

De top-level stealstatistieken bundelen idle, single, multi en sustained samples. De consistency-input en surveys gebruiken sustained samples. Percentages zijn vensterpercentages, geen exclusief gebruik van dit proces, en op fracties na één seconde gelijk gewogen. Cgroup-throttling is de maximale beschikbare counterdelta over zichtbare ancestors, in seconden; counters worden niet opgeteld wegens mogelijk overlappende accounting. Het is een cgroup-metric, geen bewijs dat specifiek de benchmark werd vertraagd.

Frequentie is de zichtbare `scaling_cur_freq` van de eerste toegestane CPU. Context-switches en load zijn host-/namespaceobservaties, geen procescounters. Grenzen en cpusets worden bij preflight vastgesteld; dynamische herconfiguratie tijdens een run is geen gecontroleerde vergelijkingsconditie. Steal nul, lage frequentie of throughputvariatie identificeert nooit zelfstandig een bepaalde oorzaak.

## Niet-gecontroleerde factoren

Firmware, kernel, compiler, ABI, scheduling, thermal states, turbo, NUMA en hostactiviteit kunnen resultaten beïnvloeden. Gebruik dezelfde officiële release en timing, herhaal runs en beoordeel de ruwe gegevens. Er is nog geen empirisch vastgesteld betrouwbaarheidsinterval of cross-architectuurkalibratie.
