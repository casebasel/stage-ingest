# Daten vom Plate Assistant (Lesesicht des Ingest)

Stand 07.10.2026, Auskunft der Plate-Assistant-Session. Massgeblich ist das Repo `casebasel/plate-assistant` (`docs/UEBERGABE-STAGE.md`, `docs/FOTOS.md`, `docs/HDRI-SCANNER-BRIEFING.md`) und die Systemkarte. Hier steht nur, was der Ingest davon braucht. „Geplant“ heisst nicht gebaut.

## Tabellen (Supabase des Plate Assistant)

Schreiben nur über die Funktion `aenderungen_anwenden` (pro Feld gewinnt die jüngere Änderung, Verlauf in `aenderung`). Löschen ist weich (`geloescht`); ein gelöschter Elternsatz blendet die Kinder aus, ohne sie zu markieren.

| Tabelle | Für den Ingest wichtig |
| --- | --- |
| `dreh` (= **Drehort**, mehrere je Tag) | `id` (ULID), `name`, `datum`, `ort`, `produktion`, `kamera`; geplant `szene` |
| `plate` | `id`, `dreh_id`, `nummer` (nicht eindeutig), `name`, `gps`, `richtung`, `neigung_grad`, `rollen_grad`, `kamera_hoehe_cm`, `abstand_cm`, `kamera {…}`, `referenzen [...]`; geplant `szene`, `vfx_notiz` |
| `take` | `id`, `plate_id`, `nummer` (je Plate), `start_zeit`, `end_zeit`, `start_tc`, `end_tc`, `clip {name, uuid, startTc, bilder}`, `clip_name` (von Hand vor dem Take), `bewertung`, `aus_clip {tiltGrad, rollGrad, tiltBereich, rollBereich}` (übernimmt der Plate Assistant aus der `ingest_meldung`); geplant `art` = take/graukugel/chromkugel/cleanplate |
| `foto` | `id`, `plate_id`, `art`, `pfad`, `zeit` (noch nicht gebaut) |
| `hdri`, `hdri_frame` | noch kein Modell; Vorschlag siehe unten |

Referenz-Takes (Grau-/Chromkugel, Cleanplate) sind echte Clips der Hauptkamera, zählen in der Take-Nummer mit und gehören in `02_PLATES/<Plate>/` als Referenz, nicht als Plate-Take.

Klappe: QR mit `PA:<take.id>`.

## Clip ↔ Take aus dem Clip (CAP)

**Entschieden 07.10.2026 (Systemkarte):** Szene = Filmszene + Buchstabe der Plate (`42A`), Take = Take-Nummer, Info 1 = `PA:<take.id>`, Info 2 vorerst leer; der Plate-Buchstabe ist ein eigenes Feld an der Plate, nicht aus der Plate-Nummer abgeleitet (Systemkarte 4d38276). Die Tabelle unten ist der frühere Vorschlag, nur noch zur Nachvollziehbarkeit.

| CAP-Feld | Inhalt | Nutzen für den Ingest |
| --- | --- | --- |
| Info 1 | `PA:<take.id>` | **eindeutige Zuordnung**, erste Wahl |
| Szene | `plate.szene` (Szenennummer des Films) | für Menschen und Schnitt |
| Take | `<plate.nummer>-<take.nummer>`, z. B. `3-12` | Prüfung gegen Info 1 |
| Info 2 | `<Drehort> · <Plate-Name>` | für Menschen |

Offen: ob die Info-Felder in den Clip-Metadaten (ProRes/ALE) ankommen; an einem echten Clip der Mini prüfen (Phase 0). Feldlängen laut CAP 1.19 (von der Stage geprüft): Scene 0x0096 höchstens 16, Take 0x0097 höchstens **8**, User Info 1/2 (0x0098/0x0099) je 128 Zeichen, beschreibbar ab SUP 5.3 (an der Kamera noch nicht live getestet); zusätzlich Location 0x0095 (64) und Production 0x0090 (32). Nur ASCII. `3-12` passt; Take-Text über 8 Zeichen wird abgeschnitten, deshalb höchstens `999-999`. Testclip mit beschriebenen Info-Feldern: Plate Assistant beim Mini-Test um den 20.10.

Migrationen des Plate Assistant: 0006 Szene/VFX-Notiz/Take-Art (nicht angewendet), 0007/0008 Fotos, danach HDRI – alle erst auf Marlons Wort.

Rückfälle: `clip_name` → Timecode-Überlappung → Zeitfenster der Plate → Klärungsliste.

## Speicher (alles geplant)

- Fotos: Bucket `fotos` (privat), `<plate_id>/<foto_id>.jpg`.
- HDRI-Rohdaten: TUS-Upload, Vorschlag `hdri/<hdri_id>/<frame>`; Grösse 1–1,5 GB je HDRI, Upload-Grenze muss hoch (Server-Administration, nur mit Marlon).
- Löschen der Rohdaten: die App löscht nie; der Ingest löscht erst ab `linked` und Bericht ok.

## HDRI (Vorschlag Plate Assistant)

`hdri` (id, `plate_id` oder `dreh_id` – genau eines, zustand `captured|uploaded|processed|linked`, Aufnahmezeit, Roh-Pfad, Ergebnis-Pfade) und `hdri_frame` (hdri_id, Nummer, yaw, pitch, roll, fov, Belichtungszeit, iso, Zeit, Kompass, Datei). Plate Assistant legt an, Ingest schreibt ab `processed` und Ergebnisse; die Job-Tabelle gehört dem Ingest.

## Rückmeldungen des Ingest

Gewünscht: Karte freigegeben (wann), Speicherort, `aus_clip` je Take, HDRI-Zustand und EXR-Pfade, Klärungsliste (Clips ohne Take, Takes ohne Clip).

**Entschieden 07.10.2026:** eigene Tabellen des Ingest (`ingest_meldung`, HDRI-Jobs), Rolle `ingest_writer`, angelegt von Marlon. `aus_clip` nur in der Meldung; der Plate Assistant übernimmt ihn an seinen Take. HDRI: `captured`/`uploaded` setzt der Plate Assistant, `processed`/`linked` der Ingest.
