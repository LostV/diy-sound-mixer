# Hardware-alternativer – DIY Sound Mixer

Status: **Første runde = variant A (én stor skjerm).** B-variantene tas senere ([handleliste-senere.md](handleliste-senere.md)). Kanaler: Spill, Media, Chat, Mic. (2026-10-22) Prisene er grove estimater i NOK inkl. MVA (AliExpress/produsentbutikk) og kan variere.

## Felles for alle varianter

Uansett skjermvalg består boksen av de samme byggeklossene:

| Komponent | Hva | Ca. pris |
|---|---|---|
| Dreieknapper | 4× EC11 rotary encoder med trykk (trykk = mute) | 30–50 kr |
| Knapper | 4–8× mekaniske tastatur-switcher (f.eks. Gateron/Kailh) for mute/profiler | 20–50 kr |
| Knotter | 3D-printet, eller aluminium-knotter for 6 mm D-aksel | 0–150 kr |
| Smådeler | Ledninger (Dupont/JST), USB-C-kabel, skruer, gummiføtter | 80–150 kr |
| Vekt | Stålskiver/blyfri vekt i bunnen så den står stødig når du vrir | 0–50 kr |
| Kabinett | 3D-printet (PLA/PETG) | filament |

Verktøy hvis du ikke har det: loddebolt (f.eks. Pinecil, ~300 kr), loddetinn, multimeter (~150 kr), breadboard + jumperkabler for prototyping (~100 kr).

**Encoder vs potmeter:** Vi bruker encodere (uendelig rotasjon), ikke potmetere. Da kan PC-en endre volumet uten at knotten blir "feil", og skjermen viser alltid riktig verdi. Det er slik BEACN gjør det.

---

## Variant A – Én stor skjerm (BEACN-stil)

En ferdig ESP32-S3-skjermmodul, med 4 knotter under.

| Del | Eksempel | Ca. pris |
|---|---|---|
| Skjermmodul | Elecrow CrowPanel 5" ESP32-S3 (800×480, berøring), ~$30 | 350–450 kr |
| | Waveshare ESP32-S3-Touch-LCD-4.3 (800×480, berøring), ~$37–44 | 400–500 kr |
| Knott-kontroller | Raspberry Pi Pico (RP2040) eller ESP32-S3 DevKit | 60–100 kr |
| Felles deler | se over | 150–400 kr |
| **Totalt** | | **ca. 600–950 kr** |

**Viktig fallgruve:** Disse skjermkortene bruker nesten alle pinnene sine på å styre skjermen, og har bare noen få ledige (I2C/UART). Løsningen er en egen liten mikrokontroller (Pico) som leser knottene. Enten kobles den som en egen USB-enhet via en liten USB-hub inni boksen, eller den snakker med skjermkortet over I2C/UART. Det første er enklest for en nybegynner.

- **Fordeler:** Nærmest BEACN. Skjermen er ferdig montert, så det blir lite lodding (bare knottene). Det er god plass til VU-metere, app-ikoner og en meny for routing rett på enheten.
- **Ulemper:** Skjermen og knottene er fysisk adskilt, så kabinettet må designes rundt en ferdig modul. Det er 2 mikrokontrollere å programmere.

## Variant B – 4 små skjermer, én per kanal

Én mikrokontroller (ESP32-S3 DevKit), med en liten skjerm over eller ved hver knott.

### B1: 4× OLED (svart/hvit)

| Del | Eksempel | Ca. pris |
|---|---|---|
| MCU | ESP32-S3 DevKitC (innebygd USB) | 80–120 kr |
| Skjermer | 4× 1.3" OLED 128×64 (SH1106/SSD1306), **SPI-versjon** | 120–200 kr |
| Felles deler | | 150–400 kr |
| **Totalt** | | **ca. 400–700 kr** |

- **Fordeler:** Billigst. Skarp, høy kontrast og "pro audio"-look. Svært lett for mikrokontrolleren å drive.
- **Ulemper:** Bare én farge og lite plass: kanalnavn, en volum-bar og et mute-ikon. OLED kan få innbrent bilde av statiske elementer over tid. Det kan vi motvirke med dimming og skjermsparer.
- **Fallgruve:** I2C-versjonene har bare 2 mulige adresser, så 4 stykker krever en I2C-multiplekser (TCA9548A). Kjøp SPI-versjonen, så får hver skjerm sin egen CS-pinne.

### B2: 4× rund farge-LCD (GC9A01, 1.28" 240×240)

| Del | Eksempel | Ca. pris |
|---|---|---|
| MCU | ESP32-S3 DevKitC N16R8 (med PSRAM) | 100–130 kr |
| Skjermer | 4× GC9A01 1.28" rund IPS | 160–250 kr |
| Felles deler | | 150–400 kr |
| **Totalt** | | **ca. 450–800 kr** |

- **Fordeler:** Ser veldig bra ut. Med en ring rundt hver knott, i stil med BEACN-skjermbildet, får du farger, app-ikon og nivåmåler.
- **Ulemper:** Mest ledningstrekking av alle variantene: 4 skjermer + 4 encodere + knapper, cirka 24 pinner.
- **Merk:** Skjermene må ha 3,3 V logikk, og ESP32 er 3,3 V, så det går fint. Alle fire deler én SPI-buss (SCK/MOSI/DC/RST) og har hver sin CS-pinne.

### B3: 4× ferdig "knott med skjerm i" (Elecrow CrowPanel 1.28" Rotary)

Hver modul er en komplett ESP32-S3 med rund berøringsskjerm *inne i knotten* og en dreiering rundt.

| Del | Eksempel | Ca. pris |
|---|---|---|
| 4× knottmoduler | Elecrow CrowPanel 1.28" HMI Rotary (~$29 stk.) | 1 300–1 450 kr |
| USB-hub (intern, 4 porter) | | 80–150 kr |
| Smådeler/knapper | | 100–200 kr |
| **Totalt** | | **ca. 1 500–1 800 kr** |

- **Fordeler:** Den mest premium løsningen, med skjermen i selve knotten. Den krever nesten null lodding, fordi alt er ferdig montert.
- **Ulemper:** Dyrest, og nær grensen på 2000 kr. Det er 4 separate enheter som PC-appen må holde orden på. Kabinettet blir større.

---

## Sammenligning

| | A: Stor skjerm | B1: 4× OLED | B2: 4× rund LCD | B3: 4× knott-modul |
|---|---|---|---|---|
| Pris | 600–950 | 400–700 | 450–800 | 1 500–1 800 |
| Lodding/ledninger | Lite | Middels | Mye | Nesten ingen |
| Visuelt | Mest info | Enkelt | Pent | Mest premium |
| Routing-meny på enheten | Ja (berøring) | Nei, kun på PC | Begrenset | Begrenset |
| Vanskelighetsgrad hardware | Lav–middels | Middels | Middels–høy | Lav |

## Anbefaling for å utforske

Bestill et rimelig **prototype-sett** som lar deg prøve både A og B2 før du bestemmer deg:

1. 1× CrowPanel 5" ESP32-S3 (variant A)
2. 1× ESP32-S3 DevKitC N16R8 + 2× GC9A01 rund LCD (variant B2, test med 2 kanaler)
3. 1× Raspberry Pi Pico
4. 6–8× EC11-encodere, en pose switcher, breadboard og jumperkabler

Ca. 700–900 kr totalt, og nesten alt kan gjenbrukes i det ferdige bygget.

---

## Notat: routing (software, kommer senere)

Routing krever ikke nødvendigvis egen driver. Plan i lag:

1. **Volum og mute per app:** Windows' innebygde lydøkter (WASAPI/Core Audio). Ingen driver trengs.
2. **Velge utgang per app:** Den samme mekanismen som "App-volum og enhetsinnstillinger" i Windows. EarTrumpet og SoundSwitch bruker den.
3. **Egen stream-mix:** Windows 10 2004+ støtter loopback-opptak per prosess. Appen kan altså fange lyden fra Discord, Spotify og så videre hver for seg og lage en egen miks, uavhengig av det du hører selv.
4. **Virtuell mikrofon/utgang:** Til å begynne med bruker vi en eksisterende signert driver (VB-Cable). En egen virtuell lyddriver krever driversignering fra Microsoft. Den tar vi først hvis vi faktisk trenger den.
