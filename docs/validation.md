# Praktijkvalidatie vóór een publieke freeze

De code kan correctness, parsing en scoreberekening testen. Dat bewijst nog niet dat de gekozen workloads en gewichten servers eerlijk rangschikken. De volgende praktijktests vereisen eigen bare-metal-, VM- en VPS-systemen; lokale WSL-smoketests vervangen ze niet.

1. Bouw één geoptimaliseerde release per architectuur en archiveer executable, Cargo.lock, target en checksum.
2. Voer `validate --expected-sha256 ...` uit met een vertrouwde checksum.
3. Draai dezelfde full release driemaal achter elkaar op rustige bare metal. Bewaar alle JSON, variatie, idle load, governor, frequentie en topologie. Leg vooraf acceptatiecriteria vast, bijvoorbeeld een onderzocht CV-doel per workload; presenteer dit niet als al gehaald.
4. Draai sustained 10m en extended. Onderzoek opwarming, finish skew, P10 en achteruitgang.
5. Herhaal op een VM en op een shared VPS. Meet minstens een rustige en een aantoonbaar belaste periode. Controleer steal onafhankelijk tegen kernel-/hypervisorobservaties indien beschikbaar. Afwezigheid van steal is geen bewijs van afwezige contention.
6. Test cpuset, affinity en parent cgroup quota, ook 0,5 en 1,5 CPU. Controleer dat de workerkeuze overeenkomt met de gerapporteerde capaciteit. Test daarnaast een container; interpreteer namespacewaarschuwingen.
7. Voer een 24-uurs full survey uit met interval 2h en jitter 5m. Bewaar median/P10/P90/min/max/CV en de ruwe runs. Observeer ten minste één onderbreking en een mislukt strict-preflightslot.
8. Controleer dat aantoonbaar wisselende sustained performance een lagere Reliable- en Consistency-score geeft. Onderzoek false positives door sampling, batchgrootte, SMT, frequentie en scheduler.
9. Test echte ARM64-hardware met dezelfde contractversie en correcte releasebinary; cross-compilatie alleen bewijst geen cross-architectuurgelijkheid.
10. Publiceer meetrapport, kalibratiebeslissingen, beperkingen en baseline. Verhoog het experimentele contract bij iedere beïnvloedende wijziging. Freeze daarna pas de publieke CPU v1 en bewaar de referentie blijvend.

## Lokale technische controles

`bash scripts/check.sh` voert formatcontrole, Clippy met warnings-as-errors en unit-/integratietests uit. Fixtures behandelen bare-metalachtige metadata, KVM, containers, cgroup v1/v2, ancestorquota, fractional quota, cpusets, namespaced mounts en strict load. Known-answer tests controleren alle workloads; score- en reporttests controleren formules, JSON-roundtrips, compatibiliteit en manipulatie van ruwe metingen.

`bash scripts/smoke.sh` bouwt release en voert info, validate, een volledige quick-run en compare met zichzelf uit. `taskset` beperkt alleen het benchmarkproces tot één toegestane CPU voor een lichte CI-smoketest. De Linux-benchmark zelf heeft geen afhankelijkheid van taskset.

Releasearchieven bevatten executablechecksums en een aparte archiefchecksum. Een checksum die naast een kwaadwillig gewijzigd artifact wordt aangeboden is geen authenticiteitsbewijs; verkrijg de verwachte checksum via een vertrouwd kanaal.
