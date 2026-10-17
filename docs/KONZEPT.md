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

- **„Sicher zum Formatieren“** erst, wenn die Karte an mindestens **N unabhängige Kopien** geprüft geschrieben ist. N ist im Ingest einstellbar (nicht am Projekt), Standard **2**.
- **Unabhängig** heisst: verschiedene physische Platten. Zwei Ziele auf derselben Platte (gleiche Seriennummer) zählen als eine Kopie.
- **NAS** (SMB) zählt als eine unabhängige Kopie, mit Hinweis im Bericht: den Zwischenspeicher des NAS kann keine App umgehen (Marlon, 07.10.2026).
- **Entwerten** (zweiter, bewusster Klick nach der Freigabe): nur den Kopf des Dateisystems löschen, damit Amira/ALEXA Mini beim Einlegen das Formatieren anbieten. Die Kamera braucht ihr eigenes ARRI-UDF; Stage Ingest formatiert nie selbst.
- **Entwerten bleibt ausgeschaltet**, bis es an einer Ersatzkarte an der echten Amira getestet ist (Phase 0).

## 5. Ordnerstruktur

Kamera-Originale **1:1 wie auf der Karte**, mit ASC MHL. Plates verweisen auf Clips, Clips werden nie doppelt abgelegt.

```
<KURZNAME>/<Datum>_<Dreh>/
  01_KAMERA/<Reel>/          Karte 1:1 + ascmhl/
  02_PLATES/P003_Name/       plate.json (Verweise auf Clips), Referenzfotos, HDRI
  03_TON/
  04_BERICHTE/               PDF-Berichte, Verlauf
  05_METADATEN/              ALE pro Karte, Bewegungs-/Objektivdaten pro Clip (ART CMD)
```

Die Struktur ist von Marlon bestätigt (07.10.2026) und gebaut (`kern/src/struktur.rs`); ohne Projekt und Dreh kommt die Karte direkt in den Zielordner. **NAS-Ziel:** ein eigenes Footage-Dataset auf den Festplatten, z. B. `Footage/<KURZNAME>/<Datum>_<Dreh>/` (ohne Kunde). Projekte kommen aus der gemeinsamen Supabase und sind in allen drei Apps verwaltbar (Systemkarte 58963fa). Der genaue Pool-Pfad steht in der lokalen Einstellung.

## 6. Zuordnung Clip ↔ Take

1. **Exakt über die Kamera:** Der Plate Assistant schreibt vor der Aufnahme Szene/Take per ARRI CAP in den Clip und speichert den Clipnamen per CAP am Take. Stage Ingest prüft beides gegeneinander.
2. **Nur, was sicher ist (Marlon, 17.10.2026: „Es handelt sich um eine Ingest-Software, da darf kein Fehler passieren“).** `soll::abgleichen` (`kern/src/soll.rs`) ordnet automatisch nur in dieser Reihenfolge zu, eindeutig in beide Richtungen (hat ein Take mehrere Kandidaten oder beanspruchen zwei Takes denselben Clip, kommen alle Beteiligten nach „Zu klären“):
   1. **QR der Klappe im Bild** (`PA:<take.id>`, `src-tauri/src/qr.rs`: erste und letzte 8 s über ART CMD). Zwei verschiedene QR im Clip → nie automatisch; widerspricht der QR dem Namen von der Kamera, kommen beide nach „Zu klären“.
   2. **Clipname von der Kamera:** `take.clip` mit `kamera.quelle = cap`, und der Clip beginnt mit dem Timecode beim REC (`start_tc`, ±2 s; bei CAP stets gleich). Ein `take.clip`, das der Ingest beim Zuordnen von Hand schreibt, ist keine Bestätigung, wenn der Clip woanders beginnt (Treppenhaus Take 8: REC 15:13:24:23, von Hand C012 mit 15:14:38:11); der Name eines Platzhalters auch nicht (`plate::name_bestaetigt`).
   3. **Take-ID in Info 1** (`PA:`/`ST:` + ULID, `kern/src/kennung.rs`): nur mit bestätigtem REC, nur der erste Clip mit dieser ID, und nur, wenn der Slate auf dieser Karte wechselt (der erste Clip einer Karte kann den Slate der vorigen tragen: die Kamera ohne Verbindung behält ihn über den Kartenwechsel). Nicht, wenn der Take schon einen Clip auf einer anderen Karte hat (`vergeben` aus der Datenbank) oder die Zeit dagegen spricht.
   4. **Timecode:** Überlappung mit Start und Ende des Takes (Stage-CSV), oder der Timecode beim REC (CAP) auf ±2 s am Start eines Clips, auch wenn der Name nicht ankam; nur Clips der Kamera mit CAP (eine zweite Kamera hat ihren eigenen Timecode) und nur, wenn die Klappe am Tag der Karte liegt.
   5. **Zeitfenster der Klappe:** Versatz o = Timecode minus iPhone-Uhr; jedes sichere Paar grenzt ihn ein (Aufnahme beginnt nach der Klappe, endet vor der nächsten, ±2 s). Ein Clip wird nur zwischen zwei sicheren Clips derselben Rolle zugeordnet, wenn er nach den Bezugspunkten beider Seiten sicher im Abschnitt genau eines Takes beginnt (springt die Kamerauhr einmal, stimmt eine Seite), dazu die Reihenfolge der Aufnahmen unabhängig von jeder Uhr; und nur, wenn kein anderer offener Clip derselben Kamera dort beginnen kann (auch keiner mit der ID eines gelöschten oder fremden Takes).

   **Geschätzte Namen ordnen nie zu** (der Vorschlag der App „voriger Name plus eins“ trifft mit gestellter Kamerauhr fremde Clips). **Keine Klappe** haben Takes aus einem Clip (`take_aus_clip`, Startzeit = Timecode des Clips; erkannt an `kamera.quelle = ale`, ältere an der Startzeit = Timecode und späterem `erstellt_am`) und Platzhalter des Plate Assistant (Startzeit = Zeitpunkt, an dem die App die Lücke bemerkte; erkannt als Serie innerhalb von 2 s oder am Clip mit Timecode ohne REC). Gelöschte Takes begrenzen die Abschnitte weiter (die Klappe hat stattgefunden).
3. Alles Unklare kommt in eine **Klärungsliste** in der App; nichts wird geraten.

**Simulation (`src-tauri/src/simulation.rs`):** Zufällige, realistische Drehtage mit bekannter Wahrheit durch denselben Code wie die App (Plan → Soll-Liste → Abgleich → Zusammenfassung → Projektseite): Verbindung reisst ab und kommt wieder, Klappen ohne REC, Aufnahmen nur an der Kamera, QR, Kamerauhr auf 2012, verschobener Timecode und Sprung mitten im Tag, B-Kamera, Platzhalter, Kartenwechsel, zweiter Drehort auf derselben Karte, gelöschte Takes. Danach übernimmt die Simulation alle Vorschläge wie Marlon und rechnet neu, bis keiner mehr kommt, und ordnet zur Gegenprobe einen Clip von Hand falsch zu. Geprüft: keine falsche Zuordnung, kein falscher Vorschlag, kein Clip verloren oder doppelt, kein Take mit Aufnahme als „ohne Aufnahme“, kein „Karte fehlt“ bei eingelesenen Karten, keine Unstimmigkeit ohne Grund. Stand 17.10.2026: 100 000 Drehtage (1,2 Mio. Clips) ohne Fehler, 98 % der falschen Zuordnungen von Hand gemeldet (`SIM_ANZAHL=100000 cargo test -p stage-ingest --release drehtage -- --nocapture`, einzeln `SIM_NR=<n>`). Mit echten Daten: Test `echtes_projekt_pruefen` (Plan und Zusammenfassungen nur gelesen).

**„Zu klären“ (Projektseite):**
- **Offene Clips** bekommen Vorschläge nach denselben Regeln (je Karte, schon zugeordnete Clips als sichere Bezugspunkte, alle Takes des Projekts ohne Clip auf einer anderen Karte); „Alle Vorschläge übernehmen“. Ein neuer Take wird nur vorgeschlagen, wenn der Clip sicher im Abschnitt eines Takes beginnt, der seinen Clip schon hat, und kein Take (auch kein gelöschter) diesen Clip nennt. Warum ein Clip offen bleibt, steht darunter (z. B. „Take-ID im ersten Clip der Karte“). Beim Zuordnen zu einem Take ohne Clipnamen schreibt der Ingest `take.clip {name, startTc, bilder}` wie CAP nach dem Stopp.
- **Takes ohne Aufnahme:** Takes ohne Clip, deren Klappe an eine eingelesene Karte grenzt. **Sicher** ohne Aufnahme nur, wenn die Klappe zwischen zwei Takes mit Clip auf derselben Karte (oder auf zwei aufeinanderfolgenden Rollen) liegt, kein offener Clip für ihn in Frage kommt, die Kamera kein REC gemeldet hat, an dem Tag nur eine Kamera drehte, jeder offene Clip dort eine Zeit hat und er kein Platzhalter ist; sonst **unklar** mit Grund. Grenzt die Klappe an keine eingelesene Karte oder liegt eine Rolle dazwischen nicht vor: „Karte fehlt“. **„Ohne Aufnahme bestätigen“** (auch alle sicheren auf einmal; Marlon, 17.10.2026: „Ich möchte immer alles behalten, es werden mir nie Takes gelöscht“): Der Take bleibt im Plate Assistant, die Bestätigung steht mit Zeit und Rechner in jeder Zusammenfassung der Karte (`ohne_aufnahme`, gilt auf Mac und Windows, bleibt beim Ergänzen). Löschen nur ausnahmsweise über das Papierkorb-Symbol mit deutlicher Warnung (`take.geloescht`, im Plate Assistant wiederherstellbar). An bestehenden Takes schreibt der Ingest nur `take.clip` und `take.geloescht` (Sperre in `src-tauri/src/plate.rs`, mit Test).
- **Unstimmigkeiten** (Marlon, 17.10.2026: „Unstimmigkeiten müssen in Zu klären erscheinen“): Jede Zuordnung wird nur mit den Belegen der Clips gegengeprüft (QR, Take-ID, Name von der Kamera, Timecode, Zeit; ohne die Zuordnungen selbst). Gemeldet wird, wenn ein sicherer Beleg einen anderen Take nennt, der Clip nicht im Abschnitt seines Takes beginnt, der Take laut Beleg einen anderen Clip derselben Kamera hat, oder ein Take zwei Clips derselben Kamera hat. Je Fall: auf den Take der Belege umhängen, lösen (zurück zu den offenen Clips) oder „Stimmt so“ (`stimmt_so` in der Zusammenfassung; ändert sich die Lage, erscheint er wieder). Für Zusammenfassungen vor 0.1.50 ohne `info1` liest die App die Take-ID aus den ersten 64 KB der Clipdatei nach (ARRI legt sie nach etwa 7 KB ab), nur im Speicher. Echte Daten 17.10.2026: genau die drei falschen Zuordnungen im Treppenhaus (C010 bei Take 6, C011 bei Take 7, C012 bei Take 8; laut Take-ID und REC gehört C010 zu Take 8, C011 und C012 sind weitere Aufnahmen danach).

**Clips aus abgebrochenem Einlesen:** liegen im Kartenordner, stehen aber in keiner Zusammenfassung (Läufe, die vor 0.1.47 an Fehler 45 bzw. „Unzulässige Funktion“ scheiterten). Sie machen keinen Take „sicher“, erscheinen in „Zu klären“ mit Vermerk „nicht gegen die Karte geprüft“ und Vorschlag; beim Bestätigen kommen sie mit `ohne_pruefung` in die Zusammenfassung und als Zeile `clip` in die Datenbank. Das endgültige Löschen verweigert, solange eine Kopie Dateien enthält, die die ASC-MHL-Historie nicht kennt (sonst würde die neue Historie sie als geprüft führen).

**Darstellung je Take (Marlon, 17.10.2026, „zweimal dasselbe Bild“):** Die Take-Tabelle zeigt den Clip, wie er auf der Karte heisst (der geplante Name der App bleibt als `clip_geplant` daneben); ein Take ohne Aufnahme heisst „Ohne Aufnahme“ und zeigt den Namen nur als „geplant“; ein aus abgebrochenem Einlesen aufgenommener Clip macht den Take „Ungeprüft“, nie „Sicher“. Je Take höchstens eine Vorschau-Kachel: hat er ein Mittelbild (`foto.quelle = clip`), fällt das Standbild der Kamera (`quelle = kamera`) weg; Setup-Fotos zuerst, dann die Takes nach Nummer. Fehlende Mittelbilder holt die App nach dem Laden der Projektseite im Hintergrund nach (ART CMD, eines nach dem anderen, je Take höchstens eines). Ein Foto, das im Speicher fehlt, heisst „noch nicht hochgeladen“. Das Löschprotokoll in `04_BERICHTE/` gilt auch, wenn ein abgebrochener Lauf den Clip in der Zusammenfassung liess. Ein Clip, der einem Drehort ohne Plates zugeordnet ist, gilt als erledigt.

**Endgültig löschen (Marlon, 15.10.2026):** In der Klärungsliste lässt sich ein Clip, der nicht ins Projekt gehört (z. B. ein alter Clip, der noch auf der Karte lag), endgültig löschen: auf allen Kopien, nie auf der Karte, nie während eines Kopiervorgangs, nach einer einfachen Bestätigung. Je Kopie (`kern/src/loeschen.rs`): Clipdatei (bei ARRIRAW der Bildordner) und `05_METADATEN/<Clip>.*` weg; die ASC-MHL-Historie nennt den Clip und wandert deshalb vollständig nach `05_METADATEN/frueher/<Karte>_<Zeit>_vor_loeschen/`, die Kopie bekommt eine neue Historie über das Verbleibende (Hashes aus der alten übernommen; besteht `ascmhl verify` der Referenz, Test `nach_endgueltigem_loeschen_besteht_verify`); Löschprotokoll `04_BERICHTE/<Karte>_GELOESCHT_<Clip>_<Zeit>.txt` mit Benutzer, Rechner, Pfad, Grösse, XXH3-128. In der Zusammenfassung verschwindet der Clip und steht unter `geloescht`. Wird die Karte später nochmals eingelesen und hat den Clip noch, kommt er beim Ergänzen wieder herein und steht wieder in „Zu klären“. In der Datenbank nur weich: `clip.geloescht = true` (Plate Assistant 15.10.2026: nie hart entfernen; die App blendet ihn überall aus).

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

## 7a. Abspielen

Doppelklick auf einen Clip (Take-Tabellen, „Zu klären“) spielt ihn in der App, Leertaste/Esc schliesst, ← → nächster Take, K Pause, `,` `.` Bild für Bild. **ARRIRAW in MXF** (ALEXA Mini) entwickelt die App selbst (`kern/src/arriraw.rs`, Protokoll `arriraw` in `src-tauri/src/arri_abspielen.rs`): Bild für Bild direkt aus dem Original, halbe Auflösung, Rec.709 wie ARRI 709, keine Zusatzdateien (Marlon, 15.10.2026: Vorschau-Videos „generieren zu viel Daten“; der ART Viewer lädt keinen Clip von aussen, ARRI dokumentiert nur Ziehen und Ablegen; einen freien ARRIRAW-Player gibt es nicht, ffmpeg kann es nicht). Gemessen an 40 Clips der Mini und gegen ART CMD abgeglichen (mittlere Abweichung unter 3 von 255): 12 Bit, stückweise verdichtet (oberhalb 1024 je 512 Werte doppelte Schrittweite), 44 Byte Vorspann je Bild, Bayer B G / G R, Weissabgleich und EI aus dem ARRI-Kopf. Volle 25 B/s brauchen etwa 175 MB/s vom Speicher; kommt das Netz nicht mit, überspringt der Player Bilder und sagt es. Anderer Aufbau (ALEXA 35/HDE, ARRICORE, ProRes in MXF) → Hinweis und „Im ART Viewer öffnen“. MOV/MP4 spielt das Video-Element.

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
- Laufzeit und VRAM von DiffHDR auf Ada (Phase 0).

## 14. Stand (07.10.2026)

Phase 1 gebaut und automatisch getestet (Linux, macOS, Windows in der CI):

| Teil | Stand |
| --- | --- |
| Karte einmal lesen, an alle Ziele gleichzeitig schreiben, XXH3-128 (+ MD5) | gebaut, getestet |
| Jedes Ziel ohne Cache zurücklesen (macOS `F_NOCACHE` beim Schreiben und Lesen, Windows `NO_BUFFERING`, Linux `fadvise`) | gebaut, getestet; macOS-Cache per `mincore` belegt |
| Geräteerkennung: physische Platte (macOS `diskutil`, Windows Plattennummer + Seriennummer), NAS = Server; Unbewiesenes zählt zusammen als eine Kopie | gebaut, an echten Geräten zu prüfen |
| Freigabe ab N unabhängigen Kopien, Hinweise (NAS-Cache, unbestimmte Platte) | gebaut, getestet |
| Vorab-Prüfung: Ziel auf der Karte, Platz, Gross/Klein-Namen, Ziele auf derselben Platte | gebaut, getestet |
| ASC MHL v2 mit Kette, Ordner- und Wurzel-Hashes; mitgebrachte Historie wird fortgesetzt | gebaut, gegen `ascmhl-debug verify` und das XSD geprüft |
| PDF-Bericht (Typst, Geist) auf jedes Ziel, neben dem Kartenordner | gebaut |
| Verlauf in der App, Ziele/Einstellungen pro Rechner gemerkt | gebaut |
| Abbruch nur auf zweiten Klick; Fenster schliesst nicht während des Kopierens; halbe Ziele werden weggeräumt | gebaut |
| Updater mit Banner und Pflicht-Update (`MINDESTVERSION`) | gebaut; erstes Release braucht die Secrets (`docs/RELEASING.md`) |
| Entwerten | bewusst nicht gebaut (erst nach Test an der Amira) |

Noch zu tun vor dem Plate-Dreh: Test mit echten Karten und Platten an Mac und Windows, erstes Release.
