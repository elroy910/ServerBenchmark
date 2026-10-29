# Versiebeleid

Er zijn drie afzonderlijke versies: resultaatschema (`schema_version`), CPU-meetcontract (`benchmark_schema_version` plus `contract`) en applicatieversie. `benchmark_version` blijft gelijk bij zuivere CLI-bugfixes; de appversie mag veranderen. De huidige benchmarkversie is 0.1.0 en is experimenteel.

Het meetcontract omvat datasets/generatoren, workloadalgoritmen, verificatieoverhead, vaste dependencyversies/Cargo.lock, compiler- en releaseprofielbeleid, eenheden, modes, sampling, statistiek, referentiewaarden, scoreformules en metriekbetekenis. Gebruik geen native CPU-optimalisatie bij een generiek releaseartifact.

De referentie wordt met `include_str!` ingebouwd en tijdens validatie getoetst tegen een hardcoded SHA-256. De tests pinnen vaste dataset- en workloadresultaten en numerieke scorevoorbeelden. `.gitattributes` legt LF vast; verander ook whitespace in de referentie niet zonder contractwijziging.

Wijzigingen die scores of meetbetekenis beïnvloeden krijgen een nieuw meetcontract en benchmarkschema. Een nieuw resultaatsveld dat de bestaande score niet verandert kan een apart resultaatschema vereisen. Bewaar bij een contractupgrade de oude referentie en implementatie of een expliciete historische reader; herschrijf oude JSON nooit naar nieuwe scores.

`compare` vereist gelijke schema's, benchmark-majorversie, modus, looptijden, compiler/buildconfiguratie en workload/referencefingerprints. De huidige validator ondersteunt het huidige contract; een toekomstige reader moet historische contracten expliciet implementeren. Applicatiepatchversies hoeven niet gelijk te zijn.

App 0.1.1 voegt labelweergave, asynchrone live-uitvoer en optionele observationele `memory`/`cpu_temperature_celsius`-samplevelden toe. Deze velden tellen niet mee in scoring en zijn bij oudere JSON-bestanden standaard onbekend. Het bestaande resultaatschema en CPU-meetcontract blijven geldig; benchmarkversie 0.1.0, workloads, timings, normalisatie en formules veranderen niet. De extra resourceobservaties gebruiken de bestaande sampler, en trage terminaluitvoer blokkeert de meetlus niet. Rust-dependencyversies blijven vast; alleen de app-packageversies in het lockfile stijgen.

Publiceer CPU v1 pas na de empirische checklist. De huidige constante referentie is geen meting van een bepaalde machine. Bij kalibratie vóór een publieke freeze moet het experimentele contract worden opgehoogd; wijzig niet stilzwijgend deze referentie onder dezelfde identiteit. Archiveer de laatste experimentele versie. Het uiteindelijke publieke v1-contract kan dus een ander schema-ID krijgen dan de huidige experimentele 1.
