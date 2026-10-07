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

Referenz-Takes (`take.art` = graukugel, chromkugel, cleanplate) sind echte Clips der Hauptkamera, zählen in der Take-Nummer mit und werden in `02_PLATES/<Plate>/` als Referenz verwiesen, nicht als Plate-Take (Systemkarte 04118cf). Der Clip selbst bleibt wie alle Clips 1:1 in `01_KAMERA/`.

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

Migrationen des Plate Assistant: 0006 (`dreh.szene`, `plate.szene`, `plate.buchstabe`, `take.art`) ist angewendet (Systemkarte 04118cf), 0007/0008 Fotos, danach HDRI – alle erst auf Marlons Wort.

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

## Projekt (entschieden 07.10.2026, Systemkarte 58963fa, a5f5177)

- Tabelle `projekt` in der gemeinsamen Supabase (bisher `supabase-plate-assistant`, künftig `supabase-stage`); Projekte sind in allen drei Apps anleg-, änder- und auswählbar.
- Der Ingest **schreibt auf `projekt` nur über `aenderungen_anwenden`** mit einem eigenen Benutzer (legt Marlon an), nicht über `ingest_writer`. Rückmeldungen bleiben in den eigenen Tabellen des Ingest.
- Schema und Formular kommen aus dem gemeinsamen Paket `casebasel/stage-projekt` (öffentlich, Stage pflegt den Inhalt).
- Schema steht (Plate Assistant, Migration 0009/0010, Systemkarte 3e1050f; genau: plate-assistant `docs/ABGLEICH.md`). Ordner mit dem **Kurznamen** (`A–Z`, `0–9`, `_`, z. B. `HAPPY_END`).
- **Zugang des Ingest:** eigener Benutzer mit `app_metadata.app = "ingest"` (legt Marlon an). Liest `dreh`, `plate`, `take`, `foto`, `projekt` und den Bucket `fotos`; schreibt über `aenderungen_anwenden` nur `projekt`. `ingest_meldung`, HDRI-Bucket und Jobs kommen später mit eigenen Migrationen.
- Der Kurzname ist nach dem Anlegen fest; Ordnernamen ändern sich nie. Doppeltes Anlegen wird je Feld zusammengeführt, Anlegen geht immer (Systemkarte 269e680).

## HDRI-Datensatz (Entwurf der HDRI-Session, 07.10.2026, noch nicht angewendet)

- `hdri`: id, dreh_id (Pflicht), plate_id (leer = ganzer Drehort), zustand `aufnahme|captured|uploaded` (processed/linked nur in den Job-Tabellen des Ingest), start/end_zeit, geraet, kamera (ultraweit|weit), **format** (`dng` = Bayer-RAW linear | `heic` = verarbeitet, nicht linear, nur gekennzeichneter Rückfall), hfov/vfov_grad, **bezugssystem** (xTrueNorthZVertical bzw. xArbitraryCorrectedZVertical), kompass_grad (rechtweisend für Yaw 0), gps, positionen, ev_stufen (tatsächlich verwendet, gleich für alle Positionen), vorschau_pfad.
- `hdri_frame`: position, yaw/pitch/roll_grad (nur Startwert, ±1–2°), **lage_quaternion [w,x,y,z]**: Kamera → Welt, v_welt = q · v_kamera; Kamera x = rechts, y = oben, Blick = −z; Welt x = rechts von Yaw 0, y = Yaw 0 waagrecht, z = oben. belichtung_s, iso, ev, zeit, pfad, geloescht (wiederholte Position).
- **Gruppieren nach Frames** (hdri_id, position, nicht gelöscht), nicht positionen × Stufen rechnen.
- Storage: Bucket `hdri`, `<hdri_id>/<frame_id>.dng|.heic`, `vorschau.jpg`, `metadata.json` (`format_version: 1`). DNG unverändert mit Metadaten und OpcodeLists. TUS-Upload.
- Löschen: Marlon hat entschieden (Systemkarte 3ee75ff), dass der Ingest die Rohdaten im Bucket `hdri` löscht, **erst nach geprüfter Ablage und Bericht ok**. Das Recht kommt als eigene Migration des Plate Assistant.
- Löschregel (Migration 0014, angewendet 07.10.2026 zusammen mit 0012/0013; hdri, hdri_frame und Bucket hdri lesbar, Grenze 50 MB je Datei, Aufnahmen erst ab Build 6): nur der Benutzer `ingest`, nur Dateien unter `<hdri_id>/…`, deren `hdri`-Zeile `zustand = 'uploaded'` oder `geloescht = true` hat. Sonst löscht der Server still 0 Dateien: **Zahl der wirklich gelöschten Objekte prüfen.** Andere Buckets tabu, Zeilen `hdri`/`hdri_frame` bleiben. Weg: `DELETE /storage/v1/object/hdri` mit `{"prefixes": [...]}`.

## Anmeldung (entschieden 07.10.2026, Systemkarte d16b393)

- Stage Ingest meldet sich mit dem **persönlichen Konto** an wie das iPhone; der Technik-Benutzer `ingest` entfällt.
- **HDRI löschen und `ingest_meldung` schreiben** darf nur ein Konto mit `app_metadata.ingest = true` (setzt Marlon pro Person, der Server prüft). Ohne das Kennzeichen: lesen und Projekte anlegen; die App zeigt „Löschen nicht freigegeben“.
- Adresse und Anon-Key setzt der Release-Build aus den Repository-Variablen `SUPABASE_ADRESSE` und `SUPABASE_ANON_KEY` ein (`docs/RELEASING.md`).
- Persönliches Konto = volle Rechte eines iPhones; der Ingest hält sich an BESITZ.md (schreibt nur `projekt`, später eigene Meldungen) und meldet sich als Gerät „Stage Ingest (Mac|Windows)“. Passwort vergessen: über die iPhone-App. Löschen mit `app_metadata.ingest = true` (JSON-Wert) erst ab Migration 0015; gelöschte Dateien in der Antwort einzeln prüfen.
