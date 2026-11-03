# Test 3 – skjermen

Mål: skjermen viser mikseren med ekte data fra PC-en, og du kan styre den med fingeren. Når du vrir på en knott, følger skjermen med.

Ingen lodding trengs. Du trenger bare skjermen, en USB-C-kabel som overfører data, og gjerne Pico-en fra test 1. Fastvaren ligger i [firmware/panel-esp32](../firmware/panel-esp32/).

Legg skjermen på et håndkle eller noe mykt, med glasset ned eller opp. Ikke legg den med baksiden mot noe av metall.

## Steg 1 – Koble til skjermen

1. **Lukk mikser-appen** (høyreklikk tray-ikonet → Avslutt). Den holder ellers porten.
2. Plugg USB-C-kabelen i skjermen og i PC-en.
3. Skjermen viser Elecrows demo-program. Det er helt normalt.
4. Åpne **Enhetsbehandling** og se under **Porter (COM og LPT)**. Der skal det stå **USB-SERIAL CH340 (COMx)**. Si fra hvilket nummer det har.

Står det ingen CH340 der, eller den har et gult utropstegn, mangler Windows driveren. Si fra, så hjelper jeg deg med den.

## Steg 2 – Last inn fastvaren

Si fra, så laster jeg inn fastvaren. Du kan også gjøre det selv:

```bash
pwsh firmware/panel-esp32/build.ps1 -Port COMx
```

Det tar under ett minutt. Skjermen starter på nytt av seg selv etterpå.

**Sjekk:** Skjermen er svart og viser «Waiting for the PC app …». Teksten er på engelsk til PC-en har fortalt skjermen hvilket språk den skal bruke. Ser du striper, flimmer, at bildet er forskjøvet, eller at fargene er feil, så ta et bilde og send meg det.

## Steg 3 – Start appen

Appen må få en liten oppdatering for å finne skjermen, så denne gangen starter jeg en utviklerversjon av den for deg. Den installerte appen skal fortsatt være avsluttet.

**Sjekk:**
- Innen et par sekunder viser skjermen de fire kanalene, med de riktige navnene og volumene.
- Ringene beveger seg når det spilles lyd.
- Fargene er de samme som i appen på PC-en.

## Steg 4 – Berøring

1. **Trykk på et kort** på mikser-siden. Kanalen skal bli mutet på PC-en: ringen blir rød og det står MUTED. Trykk igjen for å slå mute av.
2. Treffer du feil kort, eller ingenting skjer, så si hvor du trykket og hva som skjedde. Da kan berøringen være speilvendt, og det retter jeg.
3. Gå til **Apper**-fanen. Trykk på en app, og trykk så **Flytt hit** i en annen kolonne. Appen skal flytte seg, også i PC-appen.
4. Prøv enhetsvelgeren under en kanal.

## Steg 5 – Knottene og skjermen sammen

Koble også til Pico-en med knottene fra test 1.

**Sjekk:**
- Vri på en knott: volumet endrer seg både på skjermen og i PC-appen.
- Trykk på knotten: kortet på skjermen viser MUTED.
- Trykk på mic-mute-knappen: mic-kortet viser MUTED og lampen lyser.

## Steg 6 – Dra ut og sett i igjen

1. Dra ut USB-C-kabelen til skjermen og sett den i igjen. Etter noen sekunder viser skjermen dataene igjen.
2. Avslutt appen. Etter ca. 3 sekunder skal skjermen vise «Venter på PC-appen …» igjen.

Når alt dette virker, er test 3 bestått.

## Hvis noe er galt

| Det du ser | Hva det betyr |
|---|---|
| Bildet flimrer, glir sidelengs eller har striper | Pikselklokka er for høy. Jeg setter den ned. |
| Rødt og blått har byttet plass | Fargepinnene er i feil rekkefølge. Jeg bytter dem. |
| Lyset er på, men bildet er svart | Skjerm-timingen stemmer ikke. Send et bilde. |
| Skjermen starter på nytt hele tiden når appen er åpen | Appen nullstiller kortet via USB. Si fra. |
| Berøringen treffer speilvendt | Jeg snur aksene. |
