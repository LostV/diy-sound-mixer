# Software-plan – DIY Sound Mixer

Status: utkast (2026-10-22). Hardware (prototype A) er bestilt, se [handleliste-prototype.md](handleliste-prototype.md).

## Mål

En egen "Sonar/BEACN-erstatter" der vi eier all kode:

- 4 kanaler: **Spill, Media, Chat, Mic**, styrt med fysiske knotter og mute-knapper.
- En 5"-skjerm som viser kanalnavn, volum, mute og nivåmålere.
- Routing: vi bestemmer selv hvilke apper som havner i hvilken kanal, og hvor lyden går.
- Ingen avhengighet av en tredjeparts mikser-app som kan bli forlatt eller ødelagt av en oppdatering.

## Grunnprinsipp: PC-en er hjernen

```
 ┌──────────────┐  USB (serial)   ┌───────────────────────────────┐   USB (serial)  ┌──────────────────┐
 │ Pico 2       │ ──────────────► │ PC-app (Windows)              │ ──────────────► │ CrowPanel 5"     │
 │ knotter,     │  "knott 2 +1"   │ - lydmotor (Core Audio)       │  "Chat: 62 %,   │ ESP32-S3 + LVGL  │
 │ knapper, LED │ ◄────────────── │ - regler: app → kanal         │   mutet, nivå"  │ (berøring)       │
 └──────────────┘  "LED 3 på"     │ - routing, profiler, oppsett  │ ◄────────────── └──────────────────┘
                                  └───────────────────────────────┘  "trykket Chat"
```

- **All logikk ligger i PC-appen.** Fastvaren på Pico og skjerm er "dum": den rapporterer input og tegner det den får beskjed om. Nye funksjoner betyr da nesten alltid bare en ny versjon av PC-appen, ikke ny flashing av hardware.
- I starten har vi **to USB-enheter**: Pico og skjerm hver for seg. I det ferdige bygget kan en liten USB-hub inni boksen gjøre det til én kabel. Alternativt kan Pico-en kobles til skjermen via UART (GPIO17/18), slik at skjermen videresender.
- Enhetene identifiserer seg selv ("hello"-melding med type og fastvareversjon), så PC-appen finner riktig COM-port automatisk og kobler til igjen hvis kabelen dras ut.

## Komponenter og teknologivalg

| Del | Valgt stack | Hvorfor |
|---|---|---|
| **PC-app** | **Rust + Tauri 2**. Lydmotoren er skrevet i Rust med Microsofts offisielle `windows`-crate (Core Audio/WASAPI via COM). UI-et (oppsett-vindu og tray) er laget i Tauri med vanilla TypeScript. | Rust/Tauri er det du kjenner best. `windows`-craten gir full tilgang til de samme Core Audio-API-ene som EarTrumpet og Sonar bruker. Tauri 2 har innebygd tray-ikon og autostart. |
| **Pico-fastvare** | C++ med **Arduino-Pico**-kjernen (støtter Pico 2 / RP2350) | Enkel å komme i gang med, mye eksempelkode. Encodere leses med PIO/interrupts, så ingen "klikk" blir borte. |
| **Skjerm-fastvare** | C++ med **Arduino + LovyanGFX + LVGL 9** (PlatformIO) | Elecrow sine egne eksempler for CrowPanel bruker samme oppsett. LVGL er standard for pene UI-er på mikrokontrollere. |
| **Skjerm-UI uten hardware** | **LVGL PC-simulator** (SDL) | UI-et kan designes og testes på PC-en før skjermen kommer, med samme kode. |
| **Protokoll** | Linjebasert JSON over USB-serial (starter enkelt, kan byttes til binært senere) | Lett å lese og debugge i en seriell konsoll. |

## Lydmotoren – routing i lag

Vi bygger routing i lag, der hvert lag er nyttig alene. **Lag 1 erstatter det meste av daglig Sonar-bruk.**

### Lag 1 – Kanaler med per-app-volum (ingen drivere)
- Windows har allerede en egen lydøkt (audio session) per app. Vi grupperer øktene i kanaler med **regler**:
  - etter prosessnavn: `discord.exe` → Chat, `spotify.exe` → Media
  - etter mappe: alt under `...\steamapps\common\` → Spill
  - standardkanal for alt som ikke matcher (f.eks. Media)
- Knott = volum for alle apper i kanalen. Trykk = mute.
- **Mic** = opptaksenhetens volum/mute (`IAudioEndpointVolume` på mikrofonen), pluss "global mute" som alltid fungerer.
- Nivåmålere per kanal (`IAudioMeterInformation`) → VU-metere på skjermen.
- Oppdager apper som starter og slutter, i sanntid.

### Lag 2 – Velg utgang per kanal
- Send f.eks. Chat + Spill til headset og Media til høyttalere, og bytt med ett trykk.
- Bruker den samme mekanismen som "App-volum og enhetsinnstillinger" i Windows (udokumentert API som EarTrumpet og SoundSwitch også bruker). **Risiko:** API-et kan endre seg mellom Windows-versjoner, så det kapsles inn bak et eget lag.

### Lag 3 – Egen stream-/opptaksmiks og virtuell mikrofon (valgfritt)
- **Process loopback capture** (Windows 10 2004+ / 11): appen fanger lyden fra hver app for seg, uten driver.
- Vi lager en egen miks (f.eks. Spill 70 % + Media 30 %, *uten* Chat) og sender den til en virtuell lydenhet (**VB-Cable**, signert og gratis). OBS/Discord velger den som kilde.
- Knottene kan bytte mellom "det jeg hører" og "det streamen hører".
- Mikrofon-kjede: støyfjerning (f.eks. RNNoise), noise gate, EQ → virtuell mikrofon.

### Lag 4 – Egen virtuell lyddriver (unngås)
- Krever kjernedriver og signering fra Microsoft. Tas først hvis VB-Cable faktisk blir en begrensning.

> **Sonar:** SteelSeries Sonar legger inn egne virtuelle enheter som kolliderer med dette. Den bør deaktiveres eller avinstalleres når lag 1 fungerer.

## PC-appens funksjoner

- **Tray-app**: kjører i bakgrunnen, starter med Windows, ikon med status (enheter tilkoblet).
- **Oppsett-vindu**: kanaler (navn, farge, ikon), regler (dra apper inn i kanaler), knott-steg (f.eks. 2 %/klikk, raskere når du vrir fort), hva knappene gjør, LED-oppførsel.
- **Profiler**: f.eks. "Gaming", "Jobb", "Stream". Bytt fra skjermen eller med hurtigtast.
- **Konfig** lagres som JSON i `%APPDATA%\DIYSoundMixer\`.
- **Simulator-modus**: virtuelle knotter og en virtuell skjerm i et vindu, så alt kan utvikles og testes **før hardware er kommet**.
- **Logging** til fil, for feilsøking.
- Senere: liten OSD-overlay ved volumendring, installer og auto-oppdatering.

## Protokoll (første versjon)

> Den gjeldende spesifikasjonen ligger i [protocol/README.md](../protocol/README.md). Eksemplene under er det opprinnelige utkastet.

Én JSON-melding per linje (`\n`), 115200 baud (USB CDC, så hastigheten er egentlig likegyldig).

```jsonc
// Enhet → PC
{"t":"hello","dev":"knobs","fw":"0.1.0"}          // ved oppstart / på forespørsel
{"t":"enc","ch":2,"d":1}                          // knott 2, ett klikk med klokka (-1 = mot)
{"t":"btn","id":"enc2","s":1}                     // knott 2 trykket inn (s:0 = sluppet)
{"t":"touch","action":"mute","ch":1}              // fra skjermen

// PC → enhet
{"t":"hello?"}                                    // be enheten identifisere seg
{"t":"led","id":0,"on":true}                      // mute-LED
{"t":"ch","i":2,"name":"Chat","vol":62,"mute":false,"color":"#5865F2"}   // ved endring
{"t":"meter","v":[0.42,0.10,0.73,0.05]}           // nivåer, ~30 ganger i sekundet
```

Spesifikasjonen flyttes til `protocol/` og versjoneres når den stabiliserer seg.

## Mappestruktur

```
DIY_Sound_Mixer/
├─ docs/                    planer, handlelister, koblingsskjema
├─ protocol/                protokoll-spesifikasjon
├─ pc-app/                 Cargo workspace
│  ├─ crates/mixer-core/    kanaler, regler, mikser-logikk, konfig (ren Rust, testbar uten lyd)
│  ├─ crates/mixer-wasapi/  Windows Core Audio-backend (windows-crate)
│  ├─ crates/mixer-devices/ serial, protokoll, enhetsoppdagelse (fase 2/4)
│  └─ app/                  Tauri 2-app: tray, oppsett-vindu, simulator (src-tauri/ + frontend)
└─ firmware/
   ├─ knobs-pico/           Pico 2: encodere, knapper, LED
   └─ panel-esp32/          CrowPanel: LVGL-UI (+ PC-simulator)
```

## Faser

Fase 0–3 kan gjøres **nå, uten hardware**.

| Fase | Innhold | Ferdig når |
|---|---|---|
| **0 – Oppsett** | Git-repo (privat på GitHub), mappestruktur, Cargo workspace + Tauri-app | `cargo build` og `cargo test` går grønt |
| **1 – Lydmotor (lag 1)** | Liste lydøkter, regler app → kanal, volum/mute per kanal, mikrofon-mute, nivåmålere | Du kan endre Discord-volumet fra et testvindu med glidebrytere |
| **2 – Tray-app og simulator** | Tray-ikon, oppsett-vindu, lagring av konfig, virtuelle knotter og virtuell skjerm | Hele flyten fungerer med simulerte knotter |
| **3 – Skjerm-UI i simulator** | LVGL-UI for 4 kanaler (navn, volum-ring, mute, VU) i PC-simulatoren | UI-et ser ferdig ut på PC-en |
| **4 – Hardware inn** | Pico-fastvare (encodere, knapper, LED), skjerm-fastvaren portet til CrowPanel, ekte serial | Knott → volum → skjerm, ende til ende |
| **5 – Routing (lag 2)** | Utgang per kanal, rask bytting mellom headset og høyttalere | Chat på headset, Media på høyttalere |
| **6 – Stream-miks og mic-kjede (lag 3)** | Process loopback, egen miks til VB-Cable, støyfjerning på mic | OBS/Discord får en egen miks |
| **7 – Finpuss** | Profiler, OSD, installer, auto-oppdatering, kabinett-integrasjon | Du har avinstallert Sonar for godt |

## Avklaringer (2026-10-22)

| Spørsmål | Svar | Konsekvens |
|---|---|---|
| Språk | Kjenner Tauri/Rust best | PC-appen skrives i Rust + Tauri 2 |
| Streaming/opptak | Nei | **Lag 3 (stream-miks) parkeres.** Mic-støyfjerning kan eventuelt tas senere som eget ønske. |
| Utganger | Headset i dag, soundbar via Bluetooth senere | **Lag 2 blir nyttig:** f.eks. Media til soundbar, eller bytte alt mellom headset og soundbar med ett trykk |
| Mic | Mikrofonen på headsettet (Astro A50) | Mic-kanalen styrer A50-mikrofonen. Se notatet under. |
| Git | Privat GitHub-repo | Satt opp |

### Notat: Astro A50

Skannet på maskinen din (2026-10-22). Windows ser A50-en som:

| Type | Enhet |
|---|---|
| Avspilling | `Headphones (A50 Game)` |
| Avspilling | `Headset Earphone (A50 Voice)` |
| Opptak | `Headset Microphone (A50 Mic)` |
| Opptak | `Line (A50 Stream Out)` |

- Headsettet har sin egen game/voice-balanse. Med routing (lag 2) kan Chat-kanalen sendes til **A50 Voice** og resten til **A50 Game**, slik at balansehjulet fortsatt fungerer.
- **Windows-standardmikrofonen er DualSense-kontrolleren**, ikke A50. Mic-kanalen har derfor et eget valg av mikrofon (`captureDevice` i konfigurasjonen), slik at den kan låses til `Headset Microphone (A50 Mic)`.
- Det finnes også flere virtuelle enheter (Steam Streaming, Oculus, S/PDIF, DualSense). Lydmotoren håndterer apper på alle enheter samtidig.

## Fremdrift

- [x] Fase 0: repo, Cargo workspace, Tauri-app
- [x] Fase 1: lydmotor (lag 1). Kanaler, regler, volum og mute per kanal, mic med valg av enhet, nivåmålere og testvindu. Testet med enhetstester og en skanning av maskinen som bare leser.
- [x] Nivåmålere per app via **process loopback**. Windows' egne sesjonsmålere viste hele A50 Game-enhetens nivå for alle aktive apper, så Discord beveget seg med musikken i Chrome. Nå måles hver apps egen lyd. Faller tilbake til Windows-måleren hvis loopback ikke er tilgjengelig. Verifisert live: Discord 0,00, Chrome 0,17–0,38.
- [x] **Lag 2: utgang per kanal** og **dra og slipp** (Sonar-stil). Appene er brikker med ikon som dras mellom kanaler og «Ikke styrt», eller flyttes via klikkmeny/tastatur. Hver kanal har en enhetsvelger, og Mic-kortet har mikrofonvelger.
  - **Tåler oppdateringer:** enheten lagres med navn og ID. Endres ID-en (driveroppdatering) eller får navnet nummer (`2- A50 Game`), finnes den igjen på navn. Routingen legges på igjen automatisk når en app starter, og enhet som ikke er koblet til vises som «ikke tilkoblet» uten at noe endres.
  - **Mekanisme:** Windows' interne `IAudioPolicyConfigFactory` (samme som Windows-innstillingene, EarTrumpet og SoundSwitch), med begge kjente interface-ID-er. Ukjent versjon → velgeren skjules, resten virker. Testet på maskinen ved å rute eksempel-prosessen selv og lese tilbake.
  - Apper med egen utgangsinnstilling (f.eks. Discord satt til en bestemt enhet i stedet for «Default») overstyrer Windows og må stå på «Default».
- [x] **Nytt design A2 – kantbelyst frostet glass** ([docs/design](design/), skisser i `design-konsepter.figcraft`). Vinduet er gjennomsiktig med **Acrylic** som standard, og kan byttes til Mica eller egen aurora-bakgrunn i tittellinjen (valget huskes). Egen tittellinje uten Windows-ramme, egen vertikal fader (mus/tastatur), volumtallet i midtkolonnen. Frosten er dempet (`--frost-blur`, `--acrylic-tint` i `styles.css`), fordi Acrylic allerede blurrer skrivebordet. Mikserens egen lydøkt (WebView) filtreres bort fra app-listen. Verifisert med skjermbilde av det ekte vinduet.
- [x] Fase 2: tray-ikon (åpne, simulator, autostart, avslutt), lukk = skjul til tray, én instans, autostart med `--minimized`, protokoll v1 ([protocol/README.md](../protocol/README.md)) og device hub (`mixer-devices`), simulatorvindu med skjerm, knotter, mute-knapper med LED og protokoll-logg. Simulatoren og hovedvinduet er testet i nettleser med simulert IPC. Hele appen er ikke kjørt ennå.
