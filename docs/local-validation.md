# Uitgevoerde lokale validatie — 29 oktober 2026

De oorspronkelijke controles hieronder betreffen app 0.1.0. Aanvullende validatie voor app 0.1.1 staat onderaan; de 0.1.0-checksums zijn historische waarden en horen niet bij de bijgewerkte binaries.

Omgeving: Ubuntu 24.04 via WSL2, Linux x86_64, Rust/Cargo 1.85.1. De benchmarkprocessen zijn tijdens de smoketests met `taskset` beperkt tot één of twee toegestane logical CPU's. Deze resultaten zijn functionele controles; ze kalibreren de experimentele baseline niet.

## Geslaagd

- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `cargo test --workspace --locked`: **30 tests**, geen failures; ook de docteststappen slagen.
- Onafhankelijke Python-verificatie van corpus SHA-256 en de vaste integer/data-checksums.
- Statische releasebuilds en distributiearchieven voor x86_64-musl en aarch64-musl. `file` bevestigt respectievelijk x86-64 static PIE en ARM aarch64 statically linked ELF.
- `info --json` en `validate --json` op de x86_64-releasebinary.
- `validate --expected-sha256 ...` met onafhankelijk lokaal berekende executablehash. Dit test de checksumvergelijking; er is nog geen ondertekende publieke release.
- Een complete quick-run met één worker, 30 seconden sustained, JSON en maandprijsmetadata: `results/local-quick.json`.
- Een korte survey met twee workers, twee quick-runs, 10 seconden sustained per run, startinterval 65 seconden en maximaal één seconde jitter. Beide runs en de incrementele samenvatting zijn opgeslagen onder `results/local-survey/`. De samenvatting eindigt met `completed: true`, twee geslaagde runs en geen failures.
- Een complete quick-run op de definitieve x86_64-binary, met twee workers en 30 seconden sustained: `results/final-quick.json`. Alle worker-affinitycontroles zijn waar. De executablehash in de JSON komt overeen met het definitieve artifact.
- Compare met hetzelfde echte resultaat geeft nul procent verschil en nul Consistency-punten verschil.
- Onafhankelijke herberekening van Single, Multi, Reliable en CPU Index uit de definitieve JSON. Ook prijsratio's en surveyaggregatie zijn onafhankelijk gecontroleerd.
- Executable- en archiefchecksums van beide definitieve musl-distributies gecontroleerd.

## Executablechecksums app 0.1.0 (historisch)

| Target | SHA-256 |
|---|---|
| x86_64-unknown-linux-musl | `3be0a3250537d76270d2a236ea90b0032e3a870e4fae956626b4fde7016b0e03` |
| aarch64-unknown-linux-musl | `4fa9c1035cd73428400c183e661d8faec659eee9b516b8b70210e95ea0352d10` |

De executables en `SHA256SUMS` staan in `dist/serverbench-<target>/`. Daarnaast bevat `dist/` tar.gz-archieven met aparte archiefchecksums. `dist/` en de lokale meetresultaten zijn gitignored, maar beschikbaar in deze werkdirectory.

## Grenzen van deze validatie

ARM64 is gecompileerd en als ELF geïnspecteerd, maar niet op echte ARM64-hardware uitgevoerd. De GNU-buildmatrix en GitHub Actions-workflow zijn toegevoegd; de remote CI is hier niet uitgevoerd. Er is tijdens ontwikkeling ook een x86_64-GNU-release gebouwd en uitgevoerd, maar de uiteindelijke aangeboden artifacts zijn de twee hierboven genoemde musl-binaries.

De full- en extended-fases zijn geïmplementeerd en hun timingconfiguraties worden getest; er is lokaal geen volledige 5/30-minutenpraktijkproef gedaan. Er is geen bare-metalherhaalproef, commerciële shared-VPS-proef, onafhankelijke niet-nul-stealvalidatie of 24-uurs survey uitgevoerd. De gemeten nul-steal in WSL bewijst geen afwezigheid van contention. Zie [de praktijkchecklist](validation.md) voordat een publieke standaard wordt bevroren.

Alle scores dragen daarom `baseline_status: experimental` en contract `cpu-v1-experimental-1`. De oorspronkelijke opdracht staat in [specification.nl.md](specification.nl.md).

## Aanvullende validatie app 0.1.1

- Formatcontrole, Clippy met warnings-as-errors en **40 unit-/integratietests** geslaagd, inclusief docteststappen.
- Tests voor lange/ontbrekende labels, filenamefallback, JSON-bestanden zonder de nieuwe optionele velden, CPU-sensorselectie, cgroup-geheugen, onbekende sensoren, uitvoerinterval, quiet/no-progress en de koppeling tussen observer en opgeslagen samples.
- De aanwezige 0.1.0-resultaten zijn met de nieuwe binary geladen en vergeleken; labels en bronbestandsnamen zijn zichtbaar. Ook een vergelijking tussen app 0.1.0- en 0.1.1-resultaten gebruikt hetzelfde meetcontract zonder incompatibiliteitsmelding.
- Een volledige quick-run met twee WSL-workers is geslaagd: `results/live-0.1.1.json`, consoleverslag `results/live-0.1.1.txt`, live-uitvoer `results/live-0.1.1.log`. Alle 75 idle/single/multi/sustained samples bevatten geheugenobservaties; updates zijn zichtbaar tijdens alle fasen. Warm-upstats worden getoond maar blijven uitgesloten van scoring. CPU-temperatuur is in deze WSL-omgeving onbekend; positieve sensorwaarden zijn via fixtures getest.
- De scoreformule is onafhankelijk uit de JSON nagerekend; benchmarkversie 0.1.0, CPU-schema 1, dataset en referentiehash blijven gelijk. De resultaten van dit lokale smoketestmoment zijn geen bewijs voor gelijkblijvende performance of een gekalibreerde standaard.
- Uitvoer gaat via een begrensde, niet-blokkerende queue naar een aparte thread. JSON wordt opgeslagen voordat op het afronden van terminaluitvoer wordt gewacht.
- De definitieve x86_64- en ARM64-musl-artifacts zijn opnieuw gebouwd. ARM64 blijft compile-only gevalideerd.

| Target | SHA-256 app 0.1.1 executable |
|---|---|
| x86_64-unknown-linux-musl | `a227c70b67356b037b972bca308960558a8270c74e4e671da289665c8b47fd82` |
| aarch64-unknown-linux-musl | `c213418d704d34e4dcb2a4aef512e3265ee14793c5abbb8e20a07624fe39b690` |

De eerste 0.1.1-smokebuild is uitgevoerd vóór een laatste CLI-aanpassing aan de volgorde van opslaan/uitvoer afronden. De definitieve binaries hierboven bevatten die aanpassing en zijn daarna opnieuw door de volledige kwaliteitscontroles en de bestaande-resultaatvergelijking gegaan. `SHA256SUMS` in de distributiemappen geeft steeds de checksum van het actuele artifact.
