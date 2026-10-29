# Project: Generic Linux Server Benchmark

## Doel

Ontwikkel een generieke, reproduceerbare benchmarktool voor Linux-servers waarmee fysieke servers, VPS’en, cloud-VM’s en andere Linux-machines eerlijk met elkaar vergeleken kunnen worden.

De tool moet nadrukkelijk niet alleen kijken naar:

- aantal CPU-cores;
- CPU-model;
- geadverteerde vCPU’s;
- kloksnelheid.

Het doel is om te meten hoeveel bruikbare en consistente rekenkracht een server in de praktijk levert.

Dit is vooral belangrijk bij VPS’en met shared CPU’s, omdat:

- meerdere VM’s dezelfde fysieke CPU kunnen delen;
- providers verschillende oversubscription-ratio’s gebruiken;
- noisy neighbours de performance kunnen beïnvloeden;
- dezelfde VPS-specificaties op verschillende fysieke hosts anders kunnen presteren;
- korte benchmarks een onrealistisch positief beeld kunnen geven.

De benchmark moet daarom zowel ruwe performance als performance-consistentie meten.

De tool moet volledig generiek zijn en mag geen kennis bevatten over een specifieke applicatie, SaaS-platform of workload.

---

# Kernprincipes

De benchmark moet voldoen aan de volgende principes:

1. Reproduceerbaar
2. Platformonafhankelijk binnen Linux
3. Geen root-rechten noodzakelijk
4. Geen Docker/Podman vereist
5. Geen netwerkverbinding noodzakelijk voor een benchmark
6. Geen externe datasets noodzakelijk
7. Geen afhankelijkheid van lokaal geïnstalleerde benchmarktools
8. Resultaten machineleesbaar opslaan
9. Geschikt voor bare metal én virtuele machines
10. VPS contention en CPU steal expliciet meten
11. Korte én langdurige performance meten
12. Benchmarkversies moeten onderling reproduceerbaar blijven
13. Een score van versie 1.0 mag niet stilzwijgend veranderen in versie 1.1
14. Raw metrics moeten altijd beschikbaar blijven naast samengestelde scores

---

# Technologie

Implementeer de tool bij voorkeur in **Rust**.

Redenen:

- native Linux binary;
- goede performance;
- minimale runtime overhead;
- eenvoudig statisch te distribueren;
- memory safe;
- ondersteuning voor x86_64 en ARM64;
- geschikt voor multithreaded benchmarks;
- goede toegang tot `/proc`, `/sys` en Linux system calls.

Doelplatformen voor de MVP:

- Linux x86_64
- Linux ARM64 / aarch64

De tool moet als standalone executable geleverd kunnen worden:

```bash
./serverbench
```

Geen installatie van Python, Node.js, Java, Docker, Podman of andere runtime mag noodzakelijk zijn.

---

# Naamgeving

Gebruik intern voorlopig:

```text
serverbench
```

De uiteindelijke productnaam mag later worden gewijzigd.

CLI voorbeelden:

```bash
serverbench info
serverbench benchmark
serverbench benchmark --quick
serverbench benchmark --full
serverbench compare result-a.json result-b.json
serverbench survey --duration 24h --interval 2h
serverbench validate
```

---

# Architectuur

Gebruik een modulaire architectuur.

Voorstel:

```text
serverbench/
│
├── Cargo.toml
├── README.md
├── LICENSE
│
├── crates/
│   ├── serverbench-cli/
│   ├── serverbench-core/
│   ├── serverbench-system/
│   ├── serverbench-cpu/
│   ├── serverbench-metrics/
│   ├── serverbench-score/
│   ├── serverbench-report/
│   └── serverbench-survey/
│
├── benchmarks/
├── reference/
├── docs/
├── tests/
├── scripts/
└── .github/
```

Verantwoordelijkheden:

```text
serverbench-cli
CLI parsing en commands

serverbench-system
hardware-, kernel- en virtualization-detectie

serverbench-cpu
CPU benchmarkimplementaties

serverbench-metrics
sampling van CPU, steal time, load en andere runtime metrics

serverbench-score
normalisatie en scoreberekening

serverbench-report
console-, JSON- en eventueel CSV-output

serverbench-survey
langdurige en geplande benchmarkruns
```

Voorkom dat benchmarklogica direct in de CLI terechtkomt.

---

# Command: serverbench info

Dit command voert geen zware benchmark uit.

Het verzamelt zoveel mogelijk systeeminformatie.

Voorbeeld:

```text
ServerBench

System
────────────────────────────────────
Hostname              server01
Architecture          x86_64
Kernel                6.x
Distribution          Ubuntu 26.04
Virtualized           yes
Hypervisor            KVM
Container             no

CPU
────────────────────────────────────
Model                  AMD EPYC ...
Logical CPUs           4
Physical cores         unknown
Sockets                unknown
SMT                    unknown
Nominal frequency      ...
Current frequency      ...
CPU quota              unlimited
Cpuset                 0-3

Memory
────────────────────────────────────
Total                  15.6 GiB
Available              14.2 GiB
```

Detecteer waar mogelijk:

- `/proc/cpuinfo`
- `/proc/stat`
- `/proc/meminfo`
- `/sys/devices/system/cpu`
- cgroup v2 CPU quota
- cpuset restrictions
- kernel version
- Linux distribution
- uptime
- load average
- architecture
- virtualization
- containerization
- NUMA
- SMT
- CPU governor
- CPU frequency

Virtualisatie kan onder andere zijn:

- bare metal;
- KVM;
- QEMU;
- VMware;
- Hyper-V;
- Xen;
- VirtualBox;
- unknown.

Wees voorzichtig met claims.

Als informatie niet betrouwbaar vast te stellen is:

```text
unknown
```

tonen.

Niet gokken.

---

# Detecteren van containers

Detecteer of de benchmark draait binnen:

- Docker;
- Podman;
- Kubernetes;
- systemd-nspawn;
- LXC;
- andere cgroup/containeromgevingen.

Wanneer de tool in een container draait:

```text
WARNING

ServerBench appears to be running inside a container.

CPU quotas, cpusets and container limits may affect results.

For comparing physical or virtual servers, running directly on the host is recommended.
```

De benchmark mag wel doorgaan.

Sla containerinformatie op in het resultaatbestand.

---

# Benchmarkmodellen

De CPU-benchmark moet uit meerdere verschillende workloads bestaan.

Gebruik niet één kunstmatige berekening als complete CPU-index.

Implementeer minimaal vier categorieën.

## 1. Integer / branching

Meet:

- integer arithmetic;
- loops;
- branching;
- lookup operations;
- kleine datastructuren.

De workload moet deterministisch zijn.

Geen randomness gebruiken tijdens de timed benchmark.

---

## 2. Compression

Gebruik een vaste ingebouwde dataset.

Meet compressie en decompressie.

Bijvoorbeeld Zstd of een vergelijkbaar algoritme.

Rapporteer afzonderlijk:

```text
compression MB/s
decompression MB/s
```

De benchmarkdata moet onderdeel zijn van de executable of deterministisch tijdens startup gegenereerd worden.

Geen filesystem-I/O tijdens de timed benchmark.

---

## 3. Hashing

Voer een vaste hashworkload uit.

Bijvoorbeeld SHA-256.

Rapporteer:

```text
MB/s
operations/sec
```

Deze benchmark mag hardwareacceleratie gebruiken indien de normale library/CPU dat automatisch doet.

Leg dat vast in de metadata.

---

## 4. Data-processing

Simuleer generiek applicatiewerk:

- objecten lezen;
- data transformeren;
- zoeken/sorteren/filteren;
- serialiseren/deserialiseren;
- string operations;
- hashing/lookups.

Gebruik een volledig deterministische dataset.

Geen filesystem-I/O tijdens de timed meting.

---

# Benchmarkfasen

Een volledige benchmark moet verschillende fasen hebben.

## Fase 1 – Pre-flight

Controleer:

- systeemload;
- available memory;
- CPU count;
- CPU quota;
- cpuset;
- virtualization;
- container state.

Waarschuw als bijvoorbeeld:

```text
load average > number of available CPUs
```

of als er al zware CPU-belasting aanwezig is.

Benchmark mag wel doorgaan.

---

# Fase 2 – Idle baseline

Duur:

```text
30 seconden
```

Meet iedere seconde:

- CPU utilization;
- steal time;
- load average;
- context switches indien beschikbaar;
- CPU frequency indien beschikbaar.

Doel:

vaststellen hoe rustig de server vóór de benchmark is.

---

# Fase 3 – Warm-up

Duur ongeveer:

```text
15 seconden
```

Doel:

- CPU uit lage power state halen;
- caches opwarmen;
- JIT vermijden is niet nodig omdat Rust native is;
- initialisatie buiten de timed benchmarks houden.

Warm-up-resultaten tellen niet mee.

---

# Fase 4 – Single-core benchmark

Draai iedere benchmark met exact één worker.

Pin de worker waar mogelijk aan één logical CPU.

Benchmark minimaal:

```text
integer
compression
decompression
hash
data processing
```

Iedere workload ongeveer:

```text
10–20 seconden
```

Voer eventueel meerdere iteraties uit.

Rapporteer:

```text
median
minimum
maximum
standard deviation
coefficient of variation
```

Gebruik de median als primaire score.

---

# Fase 5 – Multi-core benchmark

Voer dezelfde workload uit met alle CPU’s die daadwerkelijk beschikbaar zijn voor het proces.

Respecteer:

- cgroup CPU quota;
- cpuset;
- processor affinity.

Gebruik niet simpelweg het aantal CPU’s uit `/proc/cpuinfo`.

Voorbeeld:

```text
Machine exposes 8 CPUs
cgroup quota = 4 CPUs

Benchmark workers = 4
```

Rapporteer:

- totale throughput;
- throughput per vCPU;
- scaling efficiency.

Scaling efficiency:

```text
multi-core throughput
---------------------
single-core throughput × worker count
```

Bijvoorbeeld:

```text
single = 100
4 workers theoretical = 400
measured = 350

scaling efficiency = 87.5%
```

---

# Fase 6 – Sustained benchmark

Dit is essentieel.

Draai een gecombineerde CPU-workload gedurende standaard:

```text
5 minuten
```

Configurabel:

```bash
serverbench benchmark --sustained 10m
```

Sample iedere seconde of iedere vijf seconden:

```text
throughput
CPU usage
steal time
frequency
load
```

Doel:

detecteren van:

- hypervisor contention;
- throttling;
- thermal throttling;
- provider CPU limits;
- inconsistent CPU scheduling;
- noisy neighbours.

Rapporteer minimaal:

```text
average throughput
median throughput
P10 throughput
P50 throughput
P90 throughput
P95 throughput
minimum throughput
maximum throughput
standard deviation
coefficient of variation
```

Gebruik vooral P10 throughput als maat voor betrouwbare performance.

---

# CPU steal time

CPU steal time moet een first-class metric zijn.

Lees `/proc/stat` tijdens de benchmark.

Bereken per sample het verschil in steal ticks ten opzichte van totale CPU ticks.

Rapporteer:

```text
average steal
median steal
P95 steal
P99 steal
maximum steal
```

Voor bare-metal systemen is steal normaliter nul.

Als steal niet beschikbaar of betekenisvol is:

```text
null
```

opslaan in JSON.

Gebruik steal time nooit als enige indicatie voor VPS contention.

Combineer deze metric met throughputvariation.

---

# Consistency score

Maak naast performance een aparte consistency score.

Schaal:

```text
0 – 100
```

100 betekent zeer stabiele performance.

Gebruik voor de score onder andere:

- coefficient of variation;
- P10 versus median;
- sustained throughput degradation;
- steal time;
- outliers.

Voorbeeld:

```text
Consistency

97/100     excellent
90/100     very good
80/100     good
70/100     variable
<70        inconsistent
```

Deze labels zijn informatief.

Sla altijd de onderliggende metrics op.

---

# Single-core score

Bereken één genormaliseerde single-core score uit de verschillende workloads.

Gebruik bij voorkeur een geometric mean.

Voorbeeld:

```text
single_integer
single_compression
single_hash
single_data
```

Geen simpele som gebruiken.

---

# Multi-core score

Zelfde principe voor multi-thread workloads.

Rapporteer daarnaast:

```text
multi throughput
per-vCPU throughput
scaling efficiency
```

---

# Reliable CPU score

Definieer een aparte score die uitgaat van sustained P10-performance.

Dit vertegenwoordigt ongeveer:

> de rekenkracht waarop de gebruiker tijdens het grootste deel van een langere workload redelijkerwijs kan rekenen.

Deze score moet duidelijk onderscheiden worden van average performance.

Voorbeeld:

```text
CPU Average Index     1450
CPU Reliable Index    1320
```

Een VPS kan dus hoge piekperformance hebben maar een lagere reliable score.

---

# CPU Index

Maak uiteindelijk één hoofdscore:

```text
CPU Index
```

Gebruik als uitgangspunt:

```text
35% single-core
40% multi-core
25% reliable performance
```

Gebruik een weighted geometric mean.

Conceptueel:

```text
CPU_INDEX =
single^0.35 *
multi^0.40 *
reliable^0.25
```

De exacte formule moet:

- centraal gedefinieerd zijn;
- unit-tested zijn;
- per benchmarkversie immutable blijven.

---

# Baseline

Definieer een vaste baseline voor versie 1.

Bijvoorbeeld:

```text
ServerBench CPU Index 1000
```

De baseline mag niet afhankelijk zijn van hardware die telkens opnieuw getest moet worden.

Gebruik een vast opgeslagen benchmark-referencebestand:

```text
reference/cpu-v1.json
```

Dit bestand bevat de absolute throughputwaarden die overeenkomen met score 1000.

Vanaf het moment dat versie 1 officieel is:

```text
NOOIT veranderen
```

Nieuwe scoring vereist:

```text
benchmark schema v2
```

of:

```text
CPU Index v2
```

Hierdoor blijven resultaten uit verschillende jaren vergelijkbaar.

---

# Voorbeeldresultaat

Console:

```text
ServerBench 1.0

CPU
─────────────────────────────────────────
Model                 AMD EPYC
Available vCPU        4
Virtualization        KVM

Single-core           1,420
Multi-core            1,310
Per-vCPU              1,270
Reliable CPU          1,245

CPU Index             1,327

Consistency
─────────────────────────────────────────
Score                 94 / 100
Average steal         0.6 %
P95 steal             2.0 %
Max steal             4.1 %
Throughput variation  3.7 %

Classification        Very good
```

---

# Geen absolute claims

Vermijd teksten zoals:

```text
Deze server is 32.7% sneller in iedere situatie.
```

Gebruik:

```text
This server scored 32.7% higher on ServerBench CPU v1.
```

Benchmarkscore is workloadafhankelijk.

Documenteer dit duidelijk.

---

# Quick mode

Ondersteun:

```bash
serverbench benchmark --quick
```

Doelduur:

```text
1–2 minuten
```

Quick mode:

- korte warmup;
- single-core;
- multi-core;
- korte sustained test.

Markeer resultaat:

```json
"benchmark_mode": "quick"
```

Quick-resultaten mogen niet automatisch als gelijkwaardig aan full-resultaten worden behandeld.

---

# Full mode

Default:

```bash
serverbench benchmark
```

of:

```bash
serverbench benchmark --full
```

Doelduur ongeveer:

```text
7–10 minuten
```

Dit is de standaard vergelijkingsmodus.

---

# Extended mode

Ondersteun:

```bash
serverbench benchmark --extended
```

Duur bijvoorbeeld:

```text
30 minuten
```

Vooral nuttig voor VPS-hostingonderzoek.

---

# Survey mode

Maak een aparte modus voor shared VPS’en:

```bash
serverbench survey \
  --duration 24h \
  --interval 2h
```

Dit command voert periodiek een benchmark uit.

Voorbeeld:

```text
00:00   1340
02:00   1328
04:00   1350
06:00   1315
08:00   1260
10:00   1185
12:00   1210
...
```

Doel:

detecteren van performanceverschillen op verschillende momenten.

Survey moet uiteindelijk rapporteren:

```text
median CPU index
P10 CPU index
P90 CPU index
minimum
maximum
coefficient of variation

median steal
P95 steal
maximum steal
```

Dit is bijzonder belangrijk bij shared VPS’en.

---

# Random jitter in survey mode

Wanneer automatische surveys op veel servers worden gebruikt, voorkom dat iedere benchmark exact op het hele uur start.

Ondersteun bijvoorbeeld:

```text
--jitter 5m
```

Hiermee wordt een kleine willekeurige startvertraging toegepast.

Gebruik deze randomness uitsluitend voor scheduling.

Niet binnen de benchmarkworkloads.

---

# Resultaatbestanden

Iedere benchmark moet minimaal JSON produceren.

Voorbeeld:

```text
results/
└── serverbench-2026-10-29T120000Z.json
```

Optioneel:

```text
.txt
.csv
```

JSON-schema bijvoorbeeld:

```json
{
  "schema_version": 1,
  "benchmark_version": "1.0.0",
  "benchmark_mode": "full",
  "timestamp": "2026-10-29T12:00:00Z",

  "system": {
    "architecture": "x86_64",
    "kernel": "6.x",
    "distribution": "Ubuntu",
    "virtualized": true,
    "hypervisor": "kvm",
    "containerized": false
  },

  "cpu": {
    "model": "AMD EPYC",
    "logical_cpus": 4,
    "usable_cpus": 4,
    "smt": null
  },

  "scores": {
    "single": 1420,
    "multi": 1310,
    "per_vcpu": 1270,
    "reliable": 1245,
    "cpu_index": 1327,
    "consistency": 94
  },

  "steal": {
    "average_percent": 0.6,
    "p95_percent": 2.0,
    "p99_percent": 3.3,
    "maximum_percent": 4.1
  }
}
```

Voeg daarnaast de individuele benchmarkmetingen toe.

Raw data mag niet verloren gaan.

---

# Compare command

Implementeer:

```bash
serverbench compare a.json b.json
```

Output:

```text
                 Server A       Server B      Difference
─────────────────────────────────────────────────────────
CPU Index          1000           1327          +32.7%
Single             1000           1420          +42.0%
Multi              1000           1310          +31.0%
Reliable            995           1245          +25.1%
Consistency          99             94           -5
```

Maak verschilpercentages alleen wanneer beide resultaten:

- dezelfde schema-versie;
- dezelfde benchmark-majorversie;
- dezelfde benchmarkmodus;

gebruiken.

Waarschuw anders.

---

# Price/performance

Ondersteun optioneel:

```bash
serverbench benchmark --monthly-cost 48.85 --currency EUR
```

Prijs mag nooit onderdeel zijn van CPU Index.

Bereken aparte metrics:

```text
CPU Index / currency unit
Reliable Index / currency unit
```

Voorbeeld:

```text
Monthly cost           €48.85
CPU Index              1327
CPU points / €         27.16
Reliable points / €    25.49
```

Prijs is gebruikersmetadata.

---

# Metadata

Laat gebruikers optioneel metadata toevoegen.

Bijvoorbeeld:

```bash
serverbench benchmark \
  --label "Provider X VPS Medium" \
  --provider "Provider X" \
  --monthly-cost 48.85 \
  --currency EUR
```

Deze informatie mag geen invloed hebben op de benchmark.

---

# Privacy

Verzamel standaard geen:

- publiek IP;
- MAC-adressen;
- gebruikersnamen;
- home directories;
- serienummers;
- machine IDs;
- cloud metadata;
- provider credentials.

Hostname alleen optioneel of standaard geanonimiseerd.

Voeg:

```bash
serverbench benchmark --anonymous
```

toe.

Idealiter is anonymous de default.

---

# Geen automatische upload in MVP

De MVP moet volledig lokaal werken.

Geen resultaten automatisch uploaden.

Later kan optioneel worden toegevoegd:

```bash
serverbench submit result.json
```

maar uitsluitend expliciet na toestemming van de gebruiker.

---

# Validatie

Implementeer:

```bash
serverbench validate
```

Controleert onder andere:

- binary-integriteit;
- benchmark datasets;
- reference data;
- timer;
- CPU affinity;
- monotonic clock;
- voldoende vrije RAM;
- correcte benchmarkversionering.

---

# Timing

Gebruik een monotonic high-resolution clock.

Gebruik geen wall-clocktijd voor throughputmetingen.

Benchmarkduur mag niet beïnvloed worden door aanpassingen van NTP/system clock.

---

# Compileroptimalisatie

Zorg dat de compiler de benchmarkworkload niet kan wegoptimaliseren.

Gebruik:

- `black_box`;
- checksum/result validation;
- runtime-verbruik van het resultaat.

Iedere workload moet na afloop een bekende checksum/resultaatwaarde opleveren.

Bij incorrect resultaat:

```text
benchmark invalid
```

Geen score produceren.

---

# CPU affinity

Waar Linux dit ondersteunt:

Single-core benchmark:

```text
worker → CPU X
```

Multi-core benchmark:

```text
worker 0 → CPU 0
worker 1 → CPU 1
...
```

Respecteer de daadwerkelijke toegestane CPU-set.

Documenteer wanneer affinity niet kon worden ingesteld.

---

# NUMA

Voor MVP:

- detecteren;
- rapporteren.

Niet proberen complexe NUMA-optimalisaties toe te passen.

Als meerdere NUMA-nodes zichtbaar zijn:

```text
NOTICE: Multiple NUMA nodes detected.
CPU results may be influenced by topology.
```

Later kan NUMA-aware benchmarking worden toegevoegd.

---

# CPU governor

Detecteer indien mogelijk:

```text
performance
powersave
schedutil
unknown
```

Rapporteer deze metadata.

Pas de governor nooit automatisch aan.

De benchmarktool mag geen systeemconfiguratie wijzigen.

---

# System load warning

Voor iedere run:

```text
Current load average: ...
```

Als de server al duidelijk belast is:

```text
WARNING

Existing system load may influence benchmark results.
```

Doorgaan tenzij gebruiker `--strict` gebruikt.

Met:

```bash
serverbench benchmark --strict
```

mag de benchmark weigeren wanneer preconditions niet voldoen.

---

# Exit codes

Definieer consistente exit codes.

Bijvoorbeeld:

```text
0 successful benchmark
1 benchmark failed
2 invalid CLI parameters
3 unsupported platform
4 benchmark validation failed
5 system too constrained
```

Documenteer dit.

---

# Logging

Ondersteun:

```bash
--verbose
--quiet
```

Standaardoutput moet menselijk leesbaar zijn.

Geen enorme debugoutput.

---

# Tests

Schrijf uitgebreide tests.

Minimaal:

## Unit tests

Voor:

- percentile calculation;
- geometric mean;
- consistency score;
- CPU index;
- steal calculation;
- parser `/proc/stat`;
- parser `/proc/cpuinfo`;
- cgroup limits;
- compare logic;
- JSON serialization;
- benchmark result validation.

## Determinisme

Benchmarkdatasets moeten bij iedere build exact dezelfde checksum hebben.

## Integration tests

Mock Linux proc/sys filesystem waar mogelijk.

Test onder:

- bare-metalachtige fixture;
- KVM fixture;
- container fixture;
- CPU quota fixture;
- cpuset fixture.

---

# CI

Gebruik bijvoorbeeld GitHub Actions.

Build:

```text
x86_64-unknown-linux-gnu
x86_64-unknown-linux-musl
aarch64-unknown-linux-gnu
aarch64-unknown-linux-musl
```

Waar praktisch mogelijk.

Run:

```text
cargo fmt --check
cargo clippy
cargo test
```

Maak release artifacts met checksums.

---

# Benchmarkversionering

Dit is essentieel.

Gebruik twee versies:

```text
application_version
benchmark_schema_version
```

Bijvoorbeeld:

```text
ServerBench app 1.4.2
CPU benchmark schema 1
```

Bugfixes aan de CLI mogen benchmarkresultaten niet ongeldig maken.

Wanneer:

- workload verandert;
- dataset verandert;
- scoring verandert;
- benchmarkduur fundamenteel verandert;

moet benchmark schema omhoog.

---

# Reproduceerbaarheid

Sla bij iedere run minimaal op:

```text
ServerBench version
benchmark schema
OS
kernel
architecture
CPU information
logical CPU count
usable CPU count
virtualization
container state
cgroup limits
CPU governor
benchmark mode
benchmark durations
timestamp
```

Hierdoor kan later worden vastgesteld waarom resultaten verschillen.

---

# Documentatie

README moet minimaal bevatten:

1. Wat ServerBench meet
2. Wat CPU Index betekent
3. Wat CPU Index niet betekent
4. Hoe shared VPS CPU werkt
5. Wat steal time betekent
6. Waarom sustained tests belangrijk zijn
7. Hoe benchmarkversies werken
8. Hoe resultaten eerlijk worden vergeleken
9. Hoe survey mode werkt
10. Privacybeleid
11. Buildinstructies
12. CLI-referentie

---

# Wat expliciet NIET in MVP hoeft

Nog niet implementeren:

- disk benchmark;
- PostgreSQL benchmark;
- netwerkbenchmark;
- memory bandwidth index;
- GPU benchmark;
- webdashboard;
- centrale database;
- accounts;
- provider ranking;
- automatische upload;
- cloudproviderdetectie;
- Kubernetes operator;
- Docker image als primaire distributie;
- Windows;
- macOS.

Houd MVP CPU-focused.

---

# Toekomstige V2 modules

Na CPU v1 kunnen aparte onafhankelijke modules worden toegevoegd.

## Memory Index

Meet:

- sequential read;
- sequential write;
- copy;
- random access;
- latency.

## Storage Index

Meet onder andere:

- sequential read/write;
- random read/write;
- IOPS;
- fsync latency;
- sync write;
- queue depth;
- sustained write.

Storagebenchmark moet destructievrij zijn en uitsluitend een tijdelijk benchmarkbestand gebruiken.

Voor databases is vooral interessant:

```text
fsync latency
random 4K
sustained sync write
```

## Network Index

Optioneel client/servermodel.

Geen externe publieke benchmarkserver verplicht maken.

## Complete Server Index

Pas wanneer CPU, memory en storage stabiel zijn:

```text
CPU Index
Memory Index
Storage Index
Network Index
```

eventueel combineren tot:

```text
Server Index
```

Maar losse scores moeten altijd zichtbaar blijven.

---

# Definition of Done voor MVP

De MVP is gereed wanneer:

1. één standalone Linux executable bestaat;
2. x86_64 werkt;
3. ARM64 werkt of minimaal compileert;
4. `serverbench info` werkt;
5. bare metal versus VM wordt gedetecteerd;
6. containers/cgroups worden gedetecteerd;
7. single-core benchmark werkt;
8. multi-core benchmark werkt;
9. sustained benchmark werkt;
10. steal time wordt gemeten;
11. throughputvariatie wordt gemeten;
12. consistency score wordt berekend;
13. CPU Index wordt berekend;
14. Reliable CPU Index wordt berekend;
15. resultaten als JSON worden opgeslagen;
16. `compare` twee resultaten kan vergelijken;
17. quick/full modes bestaan;
18. alle scoreformules unit tests hebben;
19. reference baseline immutable is;
20. benchmarkresultaten reproduceerbaar zijn;
21. de tool geen root vereist;
22. de tool geen netwerk nodig heeft;
23. de tool geen systeeminstellingen wijzigt.

---

# Implementatievolgorde

Werk iteratief.

## Milestone 1 – Foundation

Implementeer:

```text
workspace
CLI
system detection
JSON models
result serialization
tests
```

Nog geen benchmarkscore.

## Milestone 2 – Benchmark engine

Implementeer:

```text
timing
CPU affinity
warmup
worker orchestration
single-core execution
multi-core execution
```

## Milestone 3 – Workloads

Implementeer afzonderlijk:

```text
integer
compression
hash
data-processing
```

Voeg correctness checks toe.

## Milestone 4 – Runtime metrics

Implementeer:

```text
/proc/stat sampling
steal
CPU utilization
load
frequency where possible
```

## Milestone 5 – Sustained test

Implementeer:

```text
time series
percentiles
variation
P10
degradation
```

## Milestone 6 – Scoring

Implementeer:

```text
baseline
normalization
single score
multi score
reliable score
consistency score
CPU Index
```

Formules volledig testen.

## Milestone 7 – Reports

Implementeer:

```text
console output
JSON
compare
```

## Milestone 8 – Survey

Implementeer:

```text
periodic runs
aggregation
survey report
```

## Milestone 9 – Packaging

Implementeer:

```text
static binaries
release builds
checksums
CI
documentation
```

---

# Belangrijkste ontwerpregel

Optimaliseer het project niet voor het produceren van één indrukwekkend benchmarkgetal.

Het primaire doel is:

> eerlijk, reproduceerbaar en inzichtelijk vergelijken hoeveel CPU-capaciteit verschillende Linux-servers daadwerkelijk en consistent beschikbaar stellen.

Een server met hoge piekperformance maar sterk wisselende prestaties moet duidelijk anders worden weergegeven dan een server met iets lagere maar zeer stabiele performance.

Daarom moeten altijd minimaal naast elkaar zichtbaar zijn:

```text
Single-core
Multi-core
Per-vCPU
Average performance
Reliable performance
Consistency
Steal time
CPU Index
```

De samengestelde CPU Index is alleen een samenvatting; de onderliggende metingen zijn leidend.

---

# Eerste praktische validatie

Na afronding van de MVP:

1. draai dezelfde release drie keer achter elkaar op één bare-metal Linux-machine;
2. controleer hoeveel de resultaten variëren;
3. voer een sustained run uit;
4. draai dezelfde binary op een VM;
5. draai hem op een shared VPS;
6. controleer of steal time correct zichtbaar wordt;
7. voer vervolgens een 24-uurs survey uit;
8. vergelijk median/P10/P90;
9. controleer of een duidelijk instabiele VPS daadwerkelijk een lagere Reliable/Consistency-score krijgt;
10. freeze daarna pas de CPU v1 baseline en scoreformules.

Gebruik dus niet onmiddellijk de eerste implementatie als definitieve publieke benchmarkstandaard. Eerst empirisch valideren en daarna CPU Benchmark Schema v1 vastzetten.