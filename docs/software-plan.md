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

A50-basestasjonen viser seg ofte som **to avspillingsenheter** i Windows ("Game" og "Voice/Chat"), og headsettet har sin egen game/chat-balanse. Det avhenger av generasjon (Gen 4, Gen 5, A50 X). Hvis du har to enheter, kan routingen (lag 2) sende Chat-kanalen til "Voice"-enheten og resten til "Game", slik at balansehjulet på headsettet fortsatt fungerer. Lydmotoren håndterer uansett apper på alle enheter samtidig.

## Åpne spørsmål

1. **Hvilken A50-generasjon har du**, og hvilke lydenheter dukker opp i Windows (Innstillinger → System → Lyd)?
