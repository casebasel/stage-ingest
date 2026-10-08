# Roadmap Stage Ingest

Stand 07.10.2026 (Release 0.1.7). Nur diese App; app-übergreifende Abhängigkeiten stehen in der Systemkarte (`casebasel/stage-system`). Phasen wie in `docs/KONZEPT.md`, Kapitel 11.

## Jetzt: Phase 1 abschliessen (Ziel: Plate-Dreh mit der ALEXA Mini, Ende Oktober 2026)

Gebaut ist der Kern (siehe KONZEPT Kapitel 14). Offen, in dieser Reihenfolge:

1. ~~Releases 0.1.1~~ (Update-Banner getestet); 0.1.2 mit allen Funden der Code-Prüfung als Pflicht-Update; ~~0.1.7 neue Oberfläche~~ (Kategorie-Standard wie Silverstack/ShotPut/Resolve, Seiten Einlesen · Projekt · Prüfen · Einrichtung, eingesteckte Karten erkennen und anbieten, Karte auswerfen).
2. **Sicherheit nachschärfen**
   - ~~Optional: Karte ein zweites Mal lesen~~ gebaut
   - ~~„Ziel nachprüfen“ gegen ASC MHL~~ gebaut
   - ~~Seriennummer der Platte auch am Mac~~ gebaut (system_profiler)
   - ~~Unabhängige Code-Prüfung des Kerns~~ gemacht, alle Funde behoben (5 kritische, 7 mittlere)
3. **Test mit echter Hardware** (Marlon)
   - Mac: echte Karte über den Kartenleser auf zwei externe Platten und aufs NAS
   - Windows: auf Ada mit `A001R132`
   - Bericht und Gerätekennung kontrollieren
   - Kartenerkennung und Auswerfen mit einer echten ALEXA-Karte
   - ~~Echter ALEXA-Clip~~ (Ada, A001R132, 08.10.2026): Karte ist ARRIRAW in MXF; MXF-Leser gebaut und bestätigt
     (Timecode, Dauer, fps, ARRIRAW, 2880x1620). Offen: Sensor-fps steht im ARRIRAW-Bildkopf (Kandidaten bei
     Byte 416/420/432 = 25000); eindeutig erst mit einem Clip, bei dem Sensor- und Projekt-fps verschieden sind
4. Erst danach: Stage Ingest am Dreh einsetzen, die ersten Tage parallel zum bisherigen Werkzeug.

## Laufend, ohne fremde Abhängigkeit

- ~~Clips gegen die Kameraeinstellungen des Projekts (fps, Codec, Auflösung `BxH`), nur Warnung~~ gebaut (Systemkarte 0d270db); aktiv, sobald der Plate Assistant Migration 0016 liefert. Sensor-fps/-modus nach dem Test an echtem Clip.
- Kopieransicht: Dateiliste mit Stand pro Ziel (wie Silverstack)
- ~~Mehrere Karten nacheinander (Warteschlange)~~ entschieden: vorerst nicht, eine Karte nach der anderen (Marlon, 08.10.2026)

## Clips und Karten in der gemeinsamen Datenbank (Systemkarte c996e96/fc42079, freigegeben 08.10.2026)

- Eigene Tabellen des Ingest `clip` (Zuordnung zu Take oder Drehort, auch „Zu klären“ von Hand, Neigung/Rollen aus
  ART CMD) und `karte` (Projekt, Reel, freigegeben, Kopien, Speicherort, Bericht). Ersetzen `ingest_meldung`.
  Nur der Ingest schreibt; Plate Assistant und Stage lesen. Gebaut (1c53eaf): nach jedem Einlesen mit Projekt,
  „Zu klären“ hängt den Clip dort mit um. Gegen den Server ungeprüft, solange PostgREST die Tabellen nicht kennt.

## Projektmanager (Grill-Me 08.10.2026, Systemkarte 4a44307 + ce9e976)

- **Schritt 1 (gebaut):** Projektwahl oben links für alle Seiten, gemerkt; Projektseite als Baum Drehort → Plate mit
  Details: Fotos als Vorschaubilder (Klick gross, ← →), HDRI mit Stand des Dienstes, Takes, Karten je Drehort; leere
  Drehorte und Plates sichtbar. Nur lesen.
- **HDRI-Vorschau:** Der HDRI-Dienst lädt `hdri/<hdri_id>/vorschau.jpg` hoch und trägt den Pfad in
  `hdri_job.ergebnis.vorschau_speicher` ein (gebaut e54ea57; wartet auf Migration 0021). **Beim späteren Löschen der
  Rohdaten nach der Freigabe `ergebnis.jpg` ausnehmen** (0015/0020 erlauben das Löschen jeder Datei der Aufnahme).
- **Ziel existiert / Kopien ergänzen** (gebaut, v0.1.15): frühere vollständige Kopie wird nur nachgeprüft (gegen Karte
  und frühere MHL-Prüfsumme, Generation in-place/verified) und gezählt; abweichende Ordner nur auf Bestätigung zur
  Seite gelegt (_ALT_<Datum>_<Zeit>); Start mit zu wenigen Kopien nur nach Rückfrage (steht im Bericht); „Kopie
  ergänzen“; Kopie aus Kopie (Kaskade) unter Prüfen, gegen die ursprünglichen Karten-Prüfsummen. Kopienzahl pro
  Projekt (vorerst lokal; Feld `projekt.kopien` beim Plate Assistant angefragt). Offen: Löschen zur Seite gelegter
  Fassungen in der App (mit Bestätigung, nur wenn eine geprüfte Kopie existiert).
- **Fotos am Drehort** (0023/0024 beim Plate Assistant gebaut, noch nicht angewendet): Fotos vom Fotoapparat beim
  Einlesen nach `foto` (dreh_id, plate_id über Zeit/GPS, quelle fotoapparat, xxh128 eindeutig, original relativ),
  JPEG ≤ 3072 px ohne Metadaten in den Bucket.
- **Kamera-Register** (Systemkarte KAMERAS.md, Schema beim Plate Assistant): Clips über die Seriennummer aus dem Clip
  einer Kamera zuordnen (`clip.kamera` = Rolle A/B des Projekts, dazu Seriennummer). Unglaubwürdiges Aufnahmedatum
  (nicht gestellte Kamerauhr, z. B. 2012) erzeugt eine Warnung im Bericht und kein `karte.erste_aufnahme` (gebaut).
- **Studio im Baum** (Systemkarte „Übersicht, Studio-Spiegel, VFX“): Drehort STUDIO mit gespiegelten Einstellungen und
  Studio-Takes der Stage; Studio-Karten zusätzlich in `karte`/`clip` mit `clip.studio_take_id` (nicht `take_id`).
  Studio-Takes darf der Ingest **bewerten und mit Notiz versehen** (nur diese zwei Felder, Recht kommt mit 0022).
  Regeln 0022 (plate-assistant `docs/SCHEMA-STUDIO-VFX.md` Fassung 2): höchstens `take_id` oder `studio_take_id`;
  beim Umhängen im selben Aufruf zuerst `take_id` = null, dann `studio_take_id`. `einstellung` nur lesen.
- **VFX-Rücklauf** (`plate_version`, Entwurf Plate Assistant SCHEMA-STUDIO-VFX 1.3): Version 0 = Kamera-Clip ohne
  Transcode (Abspielfassung von Hand); VFX-Fassungen 1, 2 … mit Pfad relativ zum Projektordner, Prüfsumme/MHL,
  Farbraum, Zustand geliefert/freigegeben/verworfen. Fest: projekt_id, take_id, version, art; Version eindeutig je
  Take auch unter gelöschten (`version_vergeben`); `pfad`/`mhl_pfad` relativ (kein /, ://, .., Laufwerk);
  `abspielbar` nur mit pfad; `xxh128` 32 kleine Hex-Zeichen.
- **Schritt 2 (wartet auf Rechte vom Plate Assistant):** Takes bewerten + Notiz, Take (und Plate) aus einem Clip
  anlegen, Plates vorab planen, Umbenennen/Verschieben/Papierkorb. Jüngste Änderung gewinnt, Löschen nur Papierkorb.
- **Umbenennen samt Kurzname** (Drehort und Projekt): Ordner auf allen erreichbaren Zielen umbenennen, Pfade in
  Datenbank und `_ingest.json` nachführen, fehlende Platten beim nächsten Einstecken; nie während eines Kopiervorgangs.
- **ULID für alle IDs** (auch Drehort, Karte, Clip), Bestand einmal umstellen (Migration Plate Assistant). Danach
  sucht der Ingest eine Karte über Projekt + Reel statt über eine ausgerechnete ID. Auch **neue Projekte** bekommen dann
  eine ULID (heute legt `plate.rs` `projekt-<kurzname>` an und verlässt sich auf das Zusammenführen gleicher IDs; nach
  der Umstellung braucht es dafür die Eindeutigkeit des Kurznamens am Server). Studio-Slate ist `STUDIO-NN` (Stage).

## Filmlogik (Systemkarte 64edac1 … 64185e7, wartet auf die Migrationen des Plate Assistant)

- Projektseite: Motive, Szenen, Drehorte, geplante Plates anlegen und bearbeiten (Schreibrecht wie die Stage, Regel im Code)
- Stufe A (0017): Drehort anlegen mit Kurzname (Vorschlag aus dem Namen, änderbar, 2–12 Zeichen, eindeutig im Projekt, Warnung bei Doppel, ID `dreh-<projekt>-<drehort>` klein; STUDIO fürs Studio); Ordner nach `dreh.kurzname` (vorbereitet); `plate.verwendung`, `plate.plan`
- Stufe B (0018): Motiv, Szene, Motiv ↔ Drehort, Plate ↔ Szene, Buchstabenzähler
- Stufe C (0019): Drehort ohne Datum: Ordnerdatum = Aufnahmedatum der Clips, Soll-Liste über `take.start_zeit`

## Phase 0: Prüfungen (laufen nebenher)

| Prüfung | Wer | Wann |
| --- | --- | --- |
| CAP-Info-Felder (`PA:<take.id>`) kommen im Clip/ALE an | Plate Assistant schreibt, Ingest liest | Mini-Test um den 20.10. |
| Entwerten an einer Ersatzkarte an der echten Amira | Marlon + Ingest | vor Freischalten der Funktion |
| DiffHDR auf Ada messen (Zeit, VRAM) | Ingest | vor Phase 4 |

## Phase 2: Zuordnung, Ordnerstruktur, Anbindung

- ~~Ordnerstruktur `<KURZNAME>/<Datum>_<Dreh>/01_KAMERA … 05_METADATEN`, Bericht nach `04_BERICHTE`~~ bestätigt und gebaut (Projekt vorerst als Text)
- Projekt aus der gemeinsamen Supabase auswählen, anlegen und ändern (Tabelle `projekt`, Paket `casebasel/stage-projekt` mit der Stage; entschieden 07.10.2026, Systemkarte 58963fa)
- ~~Plate Assistant lesen (Projekt, Drehort, Takes als Soll-Liste) und Projekt anlegen~~ gebaut (Anmeldung mit eigenem Benutzer, Passwort im Schlüsselbund); offen: Test, sobald Marlon Migrationen 0007–0010 anwendet und den Benutzer anlegt; Fotos; Rückmeldungen (`ingest_meldung`, eigene Migration später)
- ~~Clip ↔ Take: Clipname → Timecode-Überlappung; Klärungsliste~~ gebaut; offen: Info 1 (`PA:<take.id>`, nach dem CAP-Test an der Mini), Zeitfenster der Plate
- ~~Soll-Liste Studio~~ gebaut (07.10.2026): Stage-CSV-Export, Abgleich nach Kamera+Reel, fehlende Clips als Hinweis vor der Freigabe; offen: Plate-Takes vom Plate Assistant als zweite Quelle
- ~~ART CMD: Neigung, Rollen, Objektiv pro Bild nach `05_METADATEN`, Mittel/Bereich~~ gebaut (optional, Pfad lokal); offen: Test mit echtem ART CMD und Clip, Vorzeichen prüfen, `aus_clip` an den Plate Assistant
- ~~ALE pro Karte aus den Clips (Start-TC, fps)~~ gebaut, gegen den Stage-Parser geprüft; ~~an den Stage-Server schicken~~ gebaut (`ingest.karte`), gemeinsam getestet; Stage ist seit 07.10.2026 in Betrieb, offen: erster Test im Studio mit einer echten Karte
- ~~`02_PLATES/`: pro Plate `plate.json` (Lage, Kamera, Takes mit Verweis auf den Clip, Referenz-Takes) und Referenzfotos~~ gebaut; offen: Test mit echten Daten
- ~~Projektseite: Plan und Stand eines Projekts (Drehorte, Plates, Takes, Fotos, HDRI), eingelesene Karten von den Zielen, Takes ohne Clip = Karte fehlt~~ gebaut (Wunsch Marlon 07.10.2026); offen: Studio-Takes der Stage (braucht Projekt am Take in der Stage), Test mit echten Daten

## Phase 3: HDRI ohne KI

HDRI-Dienst auf Ada (rechnet nur, wenn nDisplay nicht läuft): Debevec-Merge, Stitching, EXR mit Maske; Prüfen/Freigeben und Ausrichten in der App; Rohdaten aus Supabase Storage, Löschen erst nach geprüftem Ingest.

## Phase 4: KI-Stufen

Nadir (ComfyUI), Lichter (DiffHDR); „gemessen“ und „ergänzt“ getrennt sichtbar.

## Phase 5: DeckLink-Livebild

Neigung/Rollen aus dem SDI-Signal.

## Entscheidungen, die Marlon noch treffen muss

- ~~Farbwelt~~ entschieden: Kategorie-Standard (PRODUCT.md, DESIGN.md)
- Rolle `ingest_writer` in der Plate-Assistant-Supabase anlegen (Phase 2)
- Zugang von VM 170 zu Ada für Windows-Tests (oder Tests von Hand)
