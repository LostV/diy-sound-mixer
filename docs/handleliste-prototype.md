# Handleliste – prototype A (én stor skjerm)

## Bestillingsstatus (2026-10-22)

| Butikk | Status | Innhold |
|---|---|---|
| Elecrow | Bestilt | CrowPanel 5" ESP32-S3 (DDP, $43,58) |
| Electrokit | Bestilt | Pico 2 WH, 6× encoder 41017819, 6× taktil knapp, 4× hette, 2× LED-knapp rød, 10× 220 Ω, breadboard, kabler, loddetinn, flussgel, loddeveke |
| AliExpress | Bestilt | Krympestrømpe 780 stk., knotter D 30×17 mm (2 pk. à 2 = 4 stk.), GH1.25 4P single head, MX1.25 4P single head |
| – | Valgfritt senere | Tastatur-switcher til mute-knapper (ferdig bygg), avisoleringstang |

Kanaler: **Spill, Media, Chat, Mic**. Det gir 4 knotter og 4 mute-knapper (trykk på knotten = mute, i tillegg til egne knapper).

Mål: å få **variant A** (5" berøringsskjerm + 4 knotter) til å fungere på skrivebordet, uten å designe kabinett ennå. Se [hardware-alternativer.md](hardware-alternativer.md). Andre varianter ligger i [handleliste-senere.md](handleliste-senere.md).

Prisene ble sjekket 2026-10-22 og er veiledende. Sjekk lagerstatus og frakt i kassen. Alle butikkene krever inn norsk MVA ved kjøp (VOEC), så det kommer ikke noe ekstra fra tollen.

Strategi: **2 bestillinger** (+ 1 valgfri).
1. **Elecrow:** skjermen.
2. **Electrokit** (Sverige, rask levering, priser i SEK): Pico, encodere, prototype-utstyr og loddetinn.
3. **AliExpress** (valgfritt, lengre leveringstid): knotter og switcher.

---

## 1. Elecrow – skjermen

| # | Produkt | Ant. | Ca. pris | Lenke |
|---|---|---|---|---|
| 1 | CrowPanel 5" ESP32-S3 IPS 800×480, berøring (SKU DLE06050B). Følger med USB-C-kabel og 4-pins kabel. | 1 | $29,90 | [elecrow.com](https://www.elecrow.com/5-esp32-s3-ips-display-800-480-capacitive-touchscreen-supports-wifi-bluetooth-with-speaker-interface.html) |

> Notat: skjermen har ingen vanlig pinnerad. Ledige pinner er GPIO17/18 (UART1) via en liten 1,25 mm 4-pins kontakt. Derfor leser en egen Pico knottene. Pico-en kobles enten til PC-en med USB (enklest å starte med) eller til skjermen via den medfølgende 4-pins kabelen.

**Delsum: ca. 400–550 kr** inkl. MVA og frakt. Frakten fra Elecrow varierer.

## 2. Electrokit – knottene og prototyping

| # | Produkt | Ant. | Ca. pris | Lenke / art.nr. |
|---|---|---|---|---|
| 2 | Raspberry Pi Pico 2 **WH** (med ferdig loddede pinner) | 1 | 109 SEK | Art.nr. **41024158** |
| 3 | Rotationsenkoder 24p/v med tryckknapp (fasta lägen). 4 i bruk + 2 i reserve. | 6 | 32 SEK stk. | Art.nr. **41017819** |
| 4 | Kopplingsdäck 840 anslutningar (fullstørrelse breadboard) | 1 | 69 SEK | Art.nr. **10160840** |
| 5 | Kopplingsbyglar för kopplingsdäck – mjuka 65st (han-han) | 1 | 45 SEK | Art.nr. **41003181** |
| 6 | Kopplingskabel 10st hane/hona 15cm (han-hun). Hun-enden tres rett på encoder-pinnene, og hver encoder trenger 5 ledninger. | 3 pk. | 22 SEK pk. | Art.nr. **41035552** |
| 6b | Micro-USB-**data**kabel til Pico 2 (Pico 2 har micro-USB, ikke USB-C). Hopp over hvis du har en som overfører data. | 1 | – | – |
| 7 | Taktil omkopplare PCB 12×12×4.5mm svart (midlertidige mute-knapper) | 6 | 3,20 SEK stk. | Art.nr. **41016951** |
| 7b | *Valgfritt:* Topp för tryckknapp 12×12mm (hetter som passer på #7) | 4 | 2,70 SEK stk. | Art.nr. 41024343 (svart) / 42 (rød) / 44 (gul) / 45 (grønn) |
| 7c | *Valgfritt:* Taktil omkopplare PCB 12×12×7mm med LED röd (teste "mute-lampe" i knappen) | 1–2 | 15 SEK stk. | Art.nr. 41004645 (eller grønn 41004652) |
| 7d | *Valgfritt (hvis 7c):* Motstånd kolfilm 0.25W 220ohm (220R), formotstand til LED-en. Pass på at det er **ohm**, ikke kohm, og ikke SMD. | 10 (minste antall) | 1 SEK stk. | Art.nr. **40810222** |
| 8 | Lödtenn Sn/Cu/Ag 0.8mm 20g blyfritt | 1 | 62 SEK | Art.nr. **41035916** |
| 9 | Flussmedel gel 1.5g spruta (TermoPasty, no-clean) | 1 | 49 SEK | Art.nr. **41023554** |

Valgfritt: [Rotasjonsenkoder-modul med Qwiic (I2C)](https://www.electrokit.com/rotationsenkoder-modul-med-tryckknapp-qwiic) til 79 SEK, en "ingen-lodding"-vei til encodere.

**Delsum: ca. 550–700 kr** (SEK ≈ NOK) + frakt.

## 3. AliExpress – valgfritt nå, kjekt senere

AliExpress-annonser forsvinner ofte, så her er søkelenker. Velg selgere med mange salg og gode vurderinger.

| # | Produkt | Ca. pris | Søk |
|---|---|---|---|
| 10 | Aluminiumsknotter: **6 mm hull, D-aksel eller settskrue**, ca. 30–40 mm diameter, 17–22 mm høy. Encoderen (41017819) har 6 mm D-aksel, 15 mm lang. Kjøp 2–3 størrelser for å teste følelsen. | 10–40 kr stk. | [søk](https://www.aliexpress.com/w/wholesale-aluminum-knob-6mm-shaft.html) |
| 11 | Mekaniske switcher (Gateron/Kailh, f.eks. 10-pk) og keycaps, til ekte mute-knapper | 30–60 kr | [søk](https://www.aliexpress.com/w/wholesale-gateron-switch.html) |
| 12 | JST 1,25 mm 4-pins kabler (ekstra kabler til skjermkontaktene) | 20–40 kr | [søk](https://www.aliexpress.com/w/wholesale-jst-1.25mm-4pin-cable.html) |
| 13 | Sortiment med krympestrømpe | 30–50 kr | [søk](https://www.aliexpress.com/w/wholesale-heat-shrink-tube-kit.html) |

**Delsum: ca. 150–300 kr.**

---

## Verktøy

### Har allerede

Loddebolt, avbitertang, multimeter, silikon-loddematte, "tredje hånd" og pinsett (ESD).

### Må kjøpe

Loddetinn og flussmiddel ligger i Electrokit-bestillingen over (#8 og #9). I tillegg:

| Verktøy | Anbefaling | Ca. pris | Hvor |
|---|---|---|---|
| Avisoleringstang | Automatisk avisolering, eller en enkel tang for 0,2–1 mm² (AWG 30–18) | 80–200 kr | Kjell / Clas Ohlson |

### Kjekt å ha

| Verktøy | Hvorfor | Ca. pris |
|---|---|---|
| Loddeveke (solder wick): Lödfläta 2.0mm ZD-180, Electrokit art.nr. **41032902** | Fjerner feil lodding | 34 SEK |
| Liten vifte/avtrekk, eller vindu på gløtt | Røyken fra flussmiddelet | – |

Sjekk loddebolten: den bør være **temperaturstyrt**, satt til ca. 330–350 °C for blyfritt tinn, og ha en ren, fortinnet spiss.

---

## Oppsummert budsjett

| Bestilling | Ca. pris |
|---|---|
| Elecrow (skjerm) | 400–550 kr |
| Electrokit (Pico, encodere, prototyping, loddetinn) | 550–700 kr |
| AliExpress (valgfritt småtteri) | 150–300 kr |
| **Prototype A totalt** | **ca. 950–1 550 kr** |
| Avisoleringstang | 80–200 kr |

Alt her gjenbrukes i det ferdige bygget.

## Når pakkene kommer – første test

1. **Knotter:** Koble Pico 2 + 1 encoder på breadboard. Mål: tallet øker og minker i en seriell konsoll når du vrir. Utvid deretter til 4 encodere og mute-knappene.
2. **Skjerm:** Flash et testprogram til CrowPanel 5". Mål: en LVGL-demo med berøring, deretter en enkel mikser-skisse med 4 kanaler.
3. **PC-app v0:** knott → volumet til én app (f.eks. Spotify), og en mute-knapp → mute mikrofonen.
4. **Sammenkobling:** PC-appen sender kanalnavn og nivåer til skjermen, og knottene oppdaterer skjermen i sanntid.

Jeg skriver koden og koblingsskjemaene til hvert steg når du har delene.
