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

Noch offen: Verfeinerung der Lage über Merkmale (IMU ±1–2°), Verzeichnung des Ultraweitwinkels, Abholen aus Supabase
und die Job-Tabelle `hdri_job`, Wächter für nDisplay.

```
python -m venv .venv && .venv/bin/pip install -r requirements.txt pytest
.venv/bin/python -m pytest -q tests
.venv/bin/python -m hdri_dienst verarbeiten <ordner mit metadata.json und DNGs> --halb
```
