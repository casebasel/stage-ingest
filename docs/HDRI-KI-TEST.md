# HDRI: KI-Stufen im Test (08.10.2026)

Grundsatz: Ein HDRI ist eine Lichtmessung. KI füllt nur, wo keine Messung ist (Loch, Person, ausgebrannte Stelle),
jede erfundene Stelle wird markiert, und die gemessene EXR bleibt unverändert. Das KI-Ergebnis kommt als zweite
Datei `<hdri_id>_ki.exr` daneben, mit einem Kanal `erfunden.Y` (Entscheid Marlon, 08.10.2026).

Testfälle: drei Innen-HDRI aus dem Plate Assistant (Küche, Wohnzimmer, Zimmer mit Kind), auf 2048×1024 verkleinert.
Grafikkarte: RTX 6000 Ada (48 GB).

## 1. Füllen (Personen, Nadir): FLUX.2 Klein + 360-ERP-Outpaint-LoRA

Eingabe wie beim Training der LoRA: zu füllende Fläche grün, Prompt „Fill the green spaces according to the image.
Outpaint as a seamless 360 equirectangular panorama (2:1) …“. Nur die grüne Fläche wird ins Original übernommen
(weicher Rand, 4 px).

| Modell | Lizenz | Zeit | Person (Kind) | Nadir 25° (Fehler im Loch, /255) |
| --- | --- | --- | --- | --- |
| Klein 4B Basis + LoRA 4B, 28 Schritte, CFG 4 | Apache 2.0 | ~100 s | gut | Küche 4,8 · Wohnzimmer 15,8 |
| Klein 9B destilliert (fp8) + LoRA 9B, 6 Schritte | nicht kommerziell | ~25 s | gut | scheitert (bleibt grün) |

- Beide Modelle verschieben die Farben leicht (4B gelblich). **Farbausgleich** je Kanal (lineare Abbildung KI →
  Original, geschätzt in einem Ring um die Fläche) behebt das: Fehler im Nadir Küche 8,5 → 4,8.
- Die LoRA ist auf das *Basis*-Modell trainiert; mit der destillierten 9B-Fassung füllt sie nur kleine Flächen.
- Erfundenes bleibt erfunden (z. B. ein Kissen, wo Hausschuhe lagen); deshalb die Markierung.
- **Empfehlung:** Klein 4B (kommerziell frei) mit Farbausgleich.

## 2. Ausgebrannte Lichter: DiffHDR (Eyeline/Netflix, Wan2.1-VACE-14B, LoRA `DiffHDR_Pano`)

Test mit Wahrheit: aus der gemessenen EXR ein 8-bit-Bild mit Belichtung so, dass die hellsten 3–4 % clippen
(Fenster, Lampen); DiffHDR rekonstruiert (30 Schritte, ~9 min je HDRI), Massstab über die nicht geclippten Pixel.
Verglichen wird das Licht in den geclippten Pixeln mit der Messung.

| HDRI | geclippt | Licht abgeschnitten (8 bit) | Licht DiffHDR | Fehler je Pixel, abgeschnitten → DiffHDR |
| --- | --- | --- | --- | --- |
| Wohnzimmer | 3,8 % | 26 % | 55 % | 0,89 → 0,67 Blenden |
| Küche | 3,3 % | 16 % | 111 % | 1,72 → 0,90 Blenden |
| Zimmer | 4,0 % | 16 % | 114 % | 0,94 → 0,56 Blenden |

- DiffHDR holt die Gesamtmenge Licht in ausgebrannten Fenstern auf etwa die richtige Grössenordnung (Abschneiden
  verliert 75–85 %), der Fehler je Pixel halbiert sich etwa.
- Ungeprüft: die **Sonne** (10–15 Blenden über dem Himmel). Dafür braucht es ein Aussen-HDRI mit Sonne als Testfall;
  bis dahin gilt die Messung mit der kürzesten Belichtung als Untergrenze (Kanal `clip`).

## Nächste Schritte (Vorschlag)

1. Im HDRI-Dienst: Nadir automatisch füllen, wo `abdeckung` = 0 (Klein 4B, Farbausgleich) → `_ki.exr` mit `erfunden`.
2. DiffHDR auf geclippte Bereiche der gemessenen EXR (nicht auf ein 8-bit-Bild), nur wo `clip` = 1.
3. Personen: Maske aus den Überlappungen (Bewegung zwischen Positionen) statt von Hand.
4. Aussen-HDRI mit Sonne aufnehmen und DiffHDR dort prüfen.
