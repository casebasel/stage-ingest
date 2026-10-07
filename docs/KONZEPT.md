# Stage Ingest – Konzept

Stand 07.10.2026. Entschieden von Marlon im Grill-Me mit der Stage-Session. Begriffe wie in der Systemkarte (`casebasel/stage-system`, `BEGRIFFE.md`): Dreh (ein Ort an einem Tag, mehrere pro Tag), Drehort, Plate, Take, Clip, Karte, Reel, Freigabe, Entwerten, Bericht, HDRI, Ada.

**Öffentliches Repo:** Hier stehen keine Schlüssel, Token, Studio-IPs, Hostnamen oder internen Pfade. Alles Ortsabhängige (Ziele, NAS, Supabase, Stage-Server, Ada) kommt aus einer **lokalen Einstellung** pro Rechner, nie aus dem Code.

## 1. Zweck

Speicherkarten Byte für Byte mit Beweis kopieren und alles eines Drehs in eine VFX-taugliche Ordnerstruktur zusammenführen: Kamera-Originale, Plates mit Referenzen und HDRI, Ton, Berichte, Metadaten. Stage Ingest macht den **ganzen Import der Plate-Assistant-Daten**; der Plate Assistant baut keinen eigenen.

## 2. Aufbau

| Teil | Technik | Läuft auf |
| --- | --- | --- |
| **Stage Ingest** (App) | Tauri 2, Kern in Rust (Kopieren, Prüfen, MHL, Bericht), Oberfläche in React | Mac und Windows, eine Codebasis |
| **HDRI-Dienst** | eigener Dienst im selben Repo (Merge, Stitching, EXR; später KI-Stufen) | Ada (RTX 6000 Ada, 48 GB) |

Vorbilder zum Anschauen (nur lesen, nichts ungeprüft übernehmen): Sluice (MIT, Rust, Freigabe-Urteile, MHL), o/COPY (mehrere Ziele, ASC MHL), ASC-MHL-Referenz (`ascmitc/mhl`), Offloader, BitMatch, OffloadKit. Updates und Releases wie Syncomat.

## 3. Kopieren und Prüfen (immer voll, kein schneller Modus)

1. Die Karte wird **genau einmal gelesen**; beim Lesen wird **XXH3-128** gerechnet, optional zusätzlich **MD5**.
2. Die gelesenen Blöcke gehen **gleichzeitig an alle Ziele** (NAS, externe Platten).
3. Danach wird **jedes Ziel komplett zurückgelesen, am Zwischenspeicher des Betriebssystems vorbei** (unbuffered/direct I/O), und gegen die Quell-Prüfsumme verglichen.
4. Abweichung, Lesefehler, fehlende oder zusätzliche Datei → das Ziel ist nicht gültig, die Karte nicht freigegeben. Kein stilles Weitermachen.
5. Ein laufender Kopiervorgang wird **nie** unterbrochen, auch nicht durch ein Update.

## 4. Freigabe und Entwerten

- **„Sicher zum Formatieren“** erst, wenn die Karte an mindestens **N unabhängige Kopien** geprüft geschrieben ist. N ist pro Produktion einstellbar, Standard **2**.
- **Unabhängig** heisst: verschiedene physische Platten. Zwei Ziele auf derselben Platte (gleiche Seriennummer) zählen als eine Kopie.
- **NAS** (SMB) zählt als eine unabhängige Kopie, mit Hinweis im Bericht: den Zwischenspeicher des NAS kann keine App umgehen (Marlon, 07.10.2026).
- **Entwerten** (zweiter, bewusster Klick nach der Freigabe): nur den Kopf des Dateisystems löschen, damit Amira/ALEXA Mini beim Einlegen das Formatieren anbieten. Die Kamera braucht ihr eigenes ARRI-UDF; Stage Ingest formatiert nie selbst.
- **Entwerten bleibt ausgeschaltet**, bis es an einer Ersatzkarte an der echten Amira getestet ist (Phase 0).

## 5. Ordnerstruktur

Kamera-Originale **1:1 wie auf der Karte**, mit ASC MHL. Plates verweisen auf Clips, Clips werden nie doppelt abgelegt.

```
<Produktion>/<Datum>_<Dreh>/
  01_KAMERA/<Reel>/          Karte 1:1 + ascmhl/
  02_PLATES/P003_Name/       plate.json (Verweise auf Clips), Referenzfotos, HDRI
  03_TON/
  04_BERICHTE/               PDF-Berichte, Verlauf
  05_METADATEN/              ALE pro Karte, Bewegungs-/Objektivdaten pro Clip (ART CMD)
```

Die Struktur ist ein Vorschlag (Phase 2 bestätigen). **NAS-Ziel:** ein eigenes Footage-Dataset auf den Festplatten, z. B. `Footage/<Kunde>/<Projekt>/<Datum>_<Dreh>/`, mit den Kunden- und Projektnamen der Stage-Konsole. Die Produktion eines Drehs ordnet der Ingest beim ersten Import einem Kunden/Projekt zu und merkt sich das. Der genaue Pool-Pfad steht in der lokalen Einstellung.

## 6. Zuordnung Clip ↔ Take

1. **Exakt über die Kamera:** Der Plate Assistant schreibt vor der Aufnahme Szene/Take per ARRI CAP in den Clip und speichert den Clipnamen per CAP am Take. Stage Ingest prüft beides gegeneinander.
2. Rückfälle in dieser Reihenfolge: `clip_name` (von Hand) → Timecode-Überlappung → Zeitfenster der Plate.
3. Alles Unklare kommt in eine **Klärungsliste** in der App; nichts wird geraten.

**Slate in der Kamera (entschieden 07.10.2026, Systemkarte `SCHNITTSTELLEN.md`):** Szene = Filmszene + Buchstabe der Plate (z. B. `42A`), Take = Take-Nummer, Info 1 = `PA:<take.id>` (erste Wahl der Zuordnung), Info 2 bleibt vorerst leer. Der Plate-Buchstabe ist ein eigenes Feld an der Plate (läuft je Filmszene über den ganzen Film) und wird nie aus der Plate-Nummer abgeleitet. CAP-Grenzen: Szene 16, Take 8, Info je 128 Zeichen, nur ASCII. Ob die Info-Felder im Clip ankommen, wird beim Mini-Test um den 20.10. geprüft.

**Timecode:** Beim Plate-Dreh läuft die Kamera auf Tageszeit-Timecode (Pflicht); der Rückfall über den Timecode bleibt.


## 6a. Soll-Liste: was gedreht wurde

Idee von Marlon (07.10.2026), mit der Stage abgestimmt. Der Ingest weiss schon vor dem Einlegen der Karte, welche Takes gedreht wurden, und gleicht die Karte dagegen ab:

- **Im Studio** liest der Ingest die Takes der Stage mit (WebSocket des Stage-Servers, Ereignisse `takes`, `einstellungen`, `sitzung`; nur lesen, keine eigene CAP-Verbindung zur Kamera). Draussen sind die Takes des Plate Assistant die Soll-Liste.
- Beim Einlesen: „Einstellung/Szene X, Take n → Clip `A001C004` liegt auf der Karte“. Clipnamen werden ohne Endung verglichen (je nach Firmware mit oder ohne `.mov`).
- **Erwarteter Clip fehlt** → Warnung vor der Freigabe (gold, mit Take). **Unerwarteter Clip** → Klärungsliste.
- Die Prüfung der Bytes bleibt davon unabhängig: die Freigabe hängt weiter an den geprüften Kopien; die Soll-Liste kommt als zusätzliche Warnung dazu.
- Rückweg an die Stage (Phase 2): ein Befehl pro Karte mit der Clipliste (Name, Reel, TC, UUID, NAS-Pfad, Prüfsumme, optional ALE).

## 7. Bewegungs- und Objektivdaten (ART CMD)

Pro Clip werden mit ARRI ART CMD die Werte **pro Bild** herausgezogen (positional/orientation: tilt, roll; `lensEntrancePupilOffset` u. a.) und als eigene Datei in `05_METADATEN` abgelegt. Mittelwert und Bereich gehen als `aus_clip` an den Plate Assistant. Felder und Vorzeichen wie in der Systemkarte (`SCHNITTSTELLEN.md`) und im Werkzeug der Stage. Dazu pro Karte das **ALE** (von der Karte oder selbst erzeugt) in die Struktur und an den Stage-Server.

## 8. HDRI

- Ein HDRI hängt an einer **Plate** oder an einem **Drehort**. Ein Take nimmt das HDRI seiner Plate, sonst das zeitlich nächste seines Drehorts.
- Rohdaten kommen **immer über Supabase Storage (TUS)** vom Plate Assistant. Gelöscht werden sie dort erst, wenn sie geprüft im Ingest liegen und der Bericht ok ist.
- **HDRI-Dienst auf Ada:** rechnet nur, wenn **nDisplay nicht läuft**. Startet nDisplay, wird ein laufender Job sofort abgebrochen und später neu gestartet. Ada ist der nDisplay-Node der Stage: nichts darf einen Take gefährden.
- Pipeline (Einzelheiten in `docs/HDRI-BRIEFING.md`): Debevec-Merge mit den echten Belichtungszeiten → Stitching (IMU-Startwert + Feature-Verfeinerung) → lineares 32-Bit-EXR, Equirectangular 2:1, mit Clip-Maske → später Nadir (ComfyUI) und Lichter (DiffHDR). „Gemessen“ und „ergänzt“ bleiben getrennt sichtbar; die rein gemessene Version wird immer mit abgelegt.
- **Korrektur in der ersten Version:** Prüfen/Freigeben (gemessen ↔ ergänzt, Maske, Belichtung) und Ausrichten (Horizont, Norden).
- Zustände am HDRI: `captured` und `uploaded` setzt der Plate Assistant (ihm gehört der HDRI-Datensatz); `processed` und `linked` sowie die HDRI-Jobs gehören dem Ingest.
- **nDisplay-Prüfung** macht der Dienst lokal auf Ada; es gibt keine Schnittstelle zur Stage.

## 9. Bericht und Meldungen

- **Bericht** auf jedem Ziel: PDF + ASC MHL.
- **Meldungen:** an den Plate Assistant (Karte freigegeben, Speicherort, `aus_clip`, HDRI-Zustand) und an den Stage-Server (Clips, ALE der Karte).
- **Verlauf** aller Vorgänge in der App.

**Rückweg (entschieden 07.10.2026):** eigene Tabellen des Ingest in der Supabase des Plate Assistant (`ingest_meldung`, HDRI-Jobs), Rolle `ingest_writer`, angelegt von Marlon. Der Ingest schreibt nie in Tabellen des Plate Assistant; `aus_clip` steht in der Meldung, der Plate Assistant übernimmt ihn an seinen Take.

Wege und Formate stehen in der Systemkarte (`SCHNITTSTELLEN.md`); was dort „offen“ ist, wird erst mit der zuständigen Session abgestimmt, dann gebaut.

## 10. Updates und Releases

- Wie Syncomat: Tauri-Updater, Banner oben in der App.
- **Pflicht-Update** nur, wenn sich Bericht, Ordnerstruktur oder eine Schnittstelle ändern (Feld `mindestVersion` in `latest.json`). Ein laufender Kopiervorgang wird nie unterbrochen.
- Öffentliche Releases auf GitHub, Endpunkt `https://github.com/casebasel/stage-ingest/releases/latest/download/latest.json`.
- Signierschlüssel nach Syncomat `docs/RELEASING.md`; die Secrets `TAURI_SIGNING_PRIVATE_KEY` und `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` legt Marlon an. Der private Schlüssel kommt nie ins Repo.
- Builds über GitHub Actions für Mac (Apple Silicon) und Windows.

## 11. Phasen

| Phase | Inhalt | Ziel |
| --- | --- | --- |
| **0** | Prüfungen: DiffHDR auf Ada messen; Entwerten an einer Ersatzkarte an der Amira testen; CAP-Schema Szene/Take mit dem Plate Assistant abstimmen | vor dem Bau der jeweiligen Funktion |
| **1** | Kopieren, Prüfen, ASC MHL, PDF-Bericht, Freigabe, Updates | **bis zum Plate-Dreh mit der ALEXA Mini Ende Oktober 2026** |
| **2** | Zuordnung Clip ↔ Take, Ordnerstruktur, ART CMD, ALE, Anbindung Plate Assistant und Stage | |
| **3** | HDRI ohne KI (Merge, Stitching, EXR, Maske) + Prüfen/Ausrichten | |
| **4** | KI-Stufen: Nadir (ComfyUI), Lichter (DiffHDR) | |
| **5** | DeckLink-Livebild mit Neigung/Rollen aus dem SDI-Signal | |

## 12. Arbeitsweise

- Gebaut in der Session `stage-ingest`; Builds über GitHub Actions.
- Windows-App wird auf Ada bzw. einer Windows-VM per SSH getestet; Ablage dort nur unter `C:\claude\tasks\JJJJ-MM-TT_kurzname`. Mac-Test durch Marlon.
- Getestet wird mit echten Clips einer ALEXA Mini (ProRes 422 HQ); der Pfad steht in der lokalen Einstellung.

## 13. Offen

- Info-Felder im Clip an der Mini prüfen (um den 20.10.).
- Empfang von Clips/ALE auf dem Stage-Server (mit Stage).
- Ordnerstruktur bestätigen (Phase 2).
- Laufzeit und VRAM von DiffHDR auf Ada (Phase 0).
