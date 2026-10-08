# HDRI-Dienst (Stage Ingest)

Rechnet die HDRI-Aufnahmen des Plate Assistant zu einem linearen 32-Bit-EXR (Equirectangular 2:1). Läuft auf Ada,
nur wenn nDisplay nicht läuft (`docs/KONZEPT.md`, Kapitel 8). Phase 3: ohne KI.

Ablauf je Aufnahme:
1. `metadata.json` (format_version 1) und die DNGs (Bayer-RAW) lesen.
2. Je Position die Belichtungsreihe linear zusammenführen (Gewichtung nach Debevec, tatsächliche Zeiten und ISO);
   Clip-Maske = gesättigt auch in der kürzesten Belichtung.
3. Nach der Lage (`lage_quaternion`, Kamera → Welt) ins Panorama legen, weiche Übergänge.
4. EXR schreiben: `R`, `G`, `B` (linear, Rec.709-Primärfarben), `clip.Y`, `abdeckung.Y` (0 = Loch); dazu eine
   JPEG-Vorschau.

Dazu: Objektivkorrektur aus dem DNG (`OpcodeList3`: WarpRectilinear2, FixVignetteRadial), Verfeinerung der Lage über
SIFT-Merkmale (RANSAC je Paar, Bündelausgleich der Drehungen und des Sichtfelds), Übergänge zum mittigsten Bild hin
(Parallaxe aus der Hand gibt schmale Nähte statt Geister), nDisplay-Wächter.

Betrieb auf Ada (`D:\hdri-dienst\`, festgelegt von Marlon 08.10.2026): Code in `code\`, Umgebung in `venv\`,
Zugang in `zugang.env` (Dienst-Konto `app = "hdri"`, nie im Repo), Ergebnisse in `ergebnisse\<hdri_id>\`.
`python -m hdri_dienst laufen` holt jede Aufnahme mit Zustand `uploaded` und rechnet sie; Stand in `hdri_job`
(ab Migration 0019). Offen: Ablage im NAS-Projektordner, KI-Stufen (Phase 4).

```
python -m venv .venv && .venv/bin/pip install -r requirements.txt pytest
.venv/bin/python -m pytest -q tests
.venv/bin/python -m hdri_dienst verarbeiten <ordner mit metadata.json und DNGs> --halb
```
