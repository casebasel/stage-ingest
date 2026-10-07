# Quellen aus der Stage

Stand 07.10.2026. Formate und Fakten aus dem Stage-Repo (`casebasel/vp-companion-app`, im Folgenden „Stage“), die Stage Ingest braucht. Verweise als `Datei:Zeile` relativ zum Stage-Repo. Die Zeilennummern gelten für diesen Stand. Dort laufen Arbeiten weiter, deshalb im Zweifel nach dem zitierten Text suchen.

Markierung:
- **geprüft** heisst, es wurde an echten Dateien oder Geräten nachgewiesen und in der Stage so dokumentiert.
- **ungeprüft** heisst, es steht nur im Code, in Tests mit erfundenen Daten oder in einem Konzept.
- **offen** heisst, in der Stage steht nichts dazu.

---

## 1. ARRI ART CMD (Bewegungs- und Objektivdaten pro Bild)

### Werkzeug und Plattform

| Punkt | Inhalt | Quelle | Stand |
| --- | --- | --- | --- |
| Programm | ARRI Reference Tool CMD **1.0.0** (`art-cmd`) | `docs/KONZEPT.md:766` | geprüft |
| Plattform | Windows-Build `art-cmd.exe`. Das Stage-Werkzeug läuft unter Linux und ruft ART CMD per SSH und PowerShell auf einem Windows-Rechner auf. | `werkzeuge/leinwand-messen/bilder_holen.py:24-31` | geprüft (am echten Mini-Clip, `docs/KONZEPT.md:804`) |
| Mac-Build | In der Stage nicht verwendet | – | offen |

### Aufrufe

Export der Metadaten pro Bild als CSV (`bilder_holen.py:83`):

```
art-cmd --mode export --input <clip> --output <name>.csv
```

Einzelbild als TIFF rendern (`bilder_holen.py:102-103`):

```
art-cmd --mode process --input <clip> --start <bildindex> --duration 1 --output <name>_%07d.tif
```

- Der Platzhalter `%07d` wird zur Bildnummer. Die Stage holt danach `<name>_0000000.tif` ab, also das erste Bild ab `--start` (`bilder_holen.py:101-103`).
- Ob der Export geklappt hat, prüft die Stage nur daran, dass die CSV-Datei existiert. Der Exit-Code wird nicht ausgewertet, die Ausgabe wird verworfen (`bilder_holen.py:83`).
- Die CSV-Datei heisst wie der Clip ohne Endung (`bilder_holen.py:81-82`).

### CSV-Format

- **Trennzeichen `;`**, erste Zeile mit Spaltennamen, eine Zeile pro Bild (`bilder_holen.py:85`).
- Die Spaltennamen sind Pfade mit `/`, etwa `positional/orientation/tilt`.

| Spalte (ART CMD) | Bedeutung | Einheit, Format | Verwendung in der Stage | Quelle | Stand |
| --- | --- | --- | --- | --- | --- |
| `timecode` | Timecode des Bildes | `HH:MM:SS:FF` | TC des Standbilds | `bilder_holen.py:114` | geprüft |
| `positional/orientation/tilt` | Neigung (Bewegungssensor) | Grad, Auflösung 0,1° | ruhige Abschnitte, Mittelwert | `bilder_holen.py:47`, `docs/KONZEPT.md:766` | geprüft (ALEXA Mini) |
| `positional/orientation/roll` | Rollen (Bewegungssensor) | Grad, Auflösung 0,1° | wie tilt | `bilder_holen.py:48`, `docs/KONZEPT.md:766` | geprüft (ALEXA Mini) |
| `projectRate/timebase` | Projekt-fps | Bruch als Text, z. B. `25/1`, `24000/1001` | fps | `bilder_holen.py:38-41, 90` | ungeprüft (Format aus dem Code) |
| `sensorState/sensorSampleRate` | Sensor-fps | Bruch wie oben | Rückfall für fps | `bilder_holen.py:90` | ungeprüft |
| `lensState/lensFocalLength` | Brennweite | **1/1000 mm** (die Stage teilt durch 1000), 0 oder leer ohne Objektivdaten | `brennweiteMm` | `bilder_holen.py:114, 121-123` | ungeprüft (Einheit aus dem Code) |
| `sensorDevice/sensorPixelPitch` | Pixelabstand des Sensors | µm (Vorgabe im Code 8,25) | Bildauswertung | `bilder_holen.py:95` | ungeprüft |
| `imageSize/sampledRect/width` | Breite des Sensorausschnitts | Pixel (Vorgabe im Code 2880) | Skalierung | `bilder_holen.py:96` | ungeprüft |
| `lensEntrancePupilOffset` | Lage der Eintrittspupille | Einheit offen. Im SDI-Signal heisst das Feld LDI27 und ist in µm angegeben. Mit LDS-Objektiven gefüllt, mit einem Sony-CineAlta-Objektiv −1 (leer). | in der Stage nicht ausgelesen | `docs/KONZEPT.md:767, 770` | Feld vorhanden: geprüft. Voller Spaltenpfad und Einheit in der CSV: offen |

- Fehlen die tilt-Spalten, nimmt die Stage drei Bilder über die Länge verteilt (`bilder_holen.py:86-88`). Clips ohne Sensordaten gibt es also, das muss der Ingest abfangen.
- Weitere Spalten der CSV sind in der Stage nicht dokumentiert. Eine vollständige Spaltenliste fehlt (**offen**). Dafür einmal einen echten Export ansehen.

### Was geprüft ist (04.10.2026, `docs/KONZEPT.md:766`)

- 19 Clips einer **ALEXA Mini**, SUP 6.01.02, rund 17 000 Bilder.
- Auflösung 0,1°. In ruhigen Sekunden steht der Wert still (Spanne 0,0°). In ruhigen Clips schwankt er um 0,06 bis 0,09°.
- Der Sensor misst Neigen und Rollen gegen die Schwerkraft, absolut und ohne Drift. Schwenk und Position liefert er nicht (`docs/KONZEPT.md:772`).

### Was nicht geprüft ist

| Frage | Quelle |
| --- | --- |
| **Absolute Genauigkeit** des Sensors. Geplant ist ein Test mit Wasserwaage, bei dem die Kamera um 180° gedreht wird. | `docs/KONZEPT.md:766`, `docs/IM-STUDIO.md` Block „Kamera misst“ |
| **Amira:** Ob sie tilt und roll in die Datei schreibt, ist nicht geprüft. Es steht als Aufgabe offen. | `docs/IM-STUDIO.md:134`, `docs/KONZEPT.md:769` |
| **Vorzeichen** von ART CMD gegenüber der gemeinsamen Konvention (siehe unten) | nirgends belegt |
| **Containerformat:** `docs/KONZEPT.md:766` sagt, ART CMD lese „aus ARRIRAW-MXF“. Die Mini-Clips des geprüften Reels sind laut `docs/IM-STUDIO.md:108-109` aber **ProRes 422 HQ in `.mov`**. Welches Format geprüft wurde, ist widersprüchlich. ART CMD mit ProRes-MOV einmal ausdrücklich bestätigen. | `docs/KONZEPT.md:766`, `docs/IM-STUDIO.md:108-109` |
| Ob die Kamera intern filtert | `docs/KONZEPT.md:812` (Frage an ARRI) |

### Vorzeichen und gemeinsame Konvention

**Gemeinsame Konvention** (Absprache mit dem Plate Assistant, `docs/KONZEPT.md:883`):
- **Neigung +** heisst: Kamera nach oben.
- **Rollen +** heisst: im Uhrzeigersinn aus Sicht der Kamera.
- Das gilt gleich für iPhone, Clip und Import.

**Umrechnung in der Stage:** Es gibt keine.
- `bilder_holen.py:111-116` übernimmt `tilt` und `roll` von ART CMD unverändert.
- `leinwand_messen.py:335-343` vergleicht sie direkt mit Pitch und Roll der Kamera im Unreal-Koordinatensystem (`tiltDiff = pitch − tilt`). Das setzt stillschweigend gleiche Vorzeichen voraus.

Ob das Vorzeichen von ART CMD der Konvention entspricht, ist **ungeprüft**. Prüfvorschlag für den Ingest: Kamera sichtbar nach oben neigen und nach rechts rollen, je 3 s aufnehmen, Vorzeichen in der CSV ablesen.

### Ruhige Abschnitte (Stage-Logik, nur zur Orientierung)

`bilder_holen.py:44-62`: Ein Fenster von `fps × halt` Bildern gilt als ruhig, wenn tilt und roll darin um höchstens `spanne` Grad schwanken. Die Vorgaben sind 1 s und 0,1° (`bilder_holen.py:71-72`). Pro ruhigem Abschnitt werden Mittelwert von tilt und roll und das mittlere Bild genommen.

> Für `aus_clip` gilt etwas anderes: **Mittel über den ganzen Clip**, Bereich = grösster minus kleinster Wert (`docs/KONZEPT.md:883`), siehe Abschnitt 4.

### SDI und CAP

- **SDI:** Dieselben Daten kommen pro Bild auch im SDI-Signal (VANC-Zeilen 9 bis 11, VFX08 Tilt, VFX09 Roll, LDI27 Eintrittspupille). Das stammt aus den Whitepapern und ist für den Ingest nicht nötig (`docs/KONZEPT.md:767`).
- **CAP** liefert **keine** Bewegungsdaten und keine Metadaten pro Bild (`docs/KONZEPT.md:768`, `docs/CAP.md:74`).

---

## 2. ALE (Avid Log Exchange)

### Format, wie die Stage es liest (`apps/server/src/ale.ts:64-136`)

Textdatei mit drei Abschnitten. Jeder Abschnitt beginnt mit einer Markenzeile, Gross- und Kleinschreibung ist egal.

| Abschnitt | Aufbau |
| --- | --- |
| `Heading` | Schlüssel und Wert, getrennt durch Tab (`FIELD_DELIM`, `VIDEO_FORMAT`, `FPS` …) |
| `Column` | eine Zeile mit den Spaltennamen, tabgetrennt |
| `Data` | eine Zeile pro Clip, tabgetrennt |

- BOM am Anfang wird entfernt. Zeilenende CRLF oder LF. Leerzeilen werden ignoriert (`ale.ts:69-81`).
- **Pflicht:** Abschnitt `Column` sowie die Spalten `Start` und `End`. Sonst bricht der Import mit Fehler ab (`ale.ts:88, 101`). Zeilen ohne Start- oder End-TC werden übersprungen (`ale.ts:111`).
- fps kommt aus dem Kopf `FPS`, sonst aus der Projekt-fps-Spalte, sonst 25 (`ale.ts:104, 130`).
- **Schlüssel eines Clips** ist der Dateiname (Spalte `Source File`), ersatzweise `Name`. Derselbe Clip in zwei ALE-Dateien bleibt so derselbe Clip (`ale.ts:7, 128`).

### Spalten, die die Stage erkennt (`ale.ts:25-46`)

Die Spaltennamen werden normalisiert: klein geschrieben, ohne Leer- und Sonderzeichen. Aus `Reel_Name` wird so `reelname`. Die erste passende Spalte gewinnt.

| Feld | erkannte Spaltennamen (normalisiert) |
| --- | --- |
| name | `name`, `clipname` |
| dateiname | `sourcefile`, `filename`, `sourcefilename`, `clipfilename` |
| reel | `reelname`, `reel`, `tape`, `tapename`, `cameraroll` |
| start / end | `start`, `starttc`, `starttimecode` / `end`, `endtc`, `endtimecode` |
| datum | `recorddate`, `recdate`, `date`, `shootingdate` (lesbar als JJJJ-MM-TT, JJJJMMTT oder TT.MM.JJJJ, `ale.ts:56-62`) |
| modell, seriennummer | `cameramodel` … / `cameraserialnumber` … |
| sensorFps, projektFps | `sensorfps` … / `projectfps`, `projectframerate`, `fps` |
| shutter, iso, weissK, tint, nd, look | `shutterangle` …, `exposureindex`, `ei`, `iso` …, `whitebalance` …, `whitebalancetint` …, `ndfilter` …, `lookname` … |
| objektiv, brennweiteMm, fokus, blende | `lensmodel` …, `lensfocallength` …, `lensfocusdistance` …, `lensiris`, `tstop` … |

- Unbekannte Spalten werden nicht übernommen, aber in der Antwort gemeldet (`ale.ts:102-103`).
- **ungeprüft:** Die Stage kennt die echten Spaltennamen der Amira noch nicht. Die Erkennung ist deshalb bewusst tolerant (`ale.ts:3`, `ale.test.ts:5`).
- **geprüft:** Für das Mini-Reel vom 02.04.2026 gibt es eine echte ALE. Darin steht Start `00:00:00:00`, weil kein Timecode lief (`docs/IM-STUDIO.md:138`). Eine Zuordnung über den Timecode war dort also nicht möglich.

### Beispielkopf (aus dem Stage-Test, Werte erfunden, `apps/server/src/ale.test.ts:6-19`)

```
Heading
FIELD_DELIM	TABS
VIDEO_FORMAT	1080
FPS	50

Column
Name	Start	End	Source File	Reel_Name	Camera_Model	Exposure_Index	Shutter_Angle	White_Balance	Tint	ND_Filter	Sensor_FPS	Lens_Model	Record_Date

Data
A001C001	10:00:00:00	10:00:30:00	A001C001_260924_R1AB.mxf	A001R1AB	ALEXA Mini	800	180.0	5600	-2	0.6	50.000	Signature Prime 35	2026-09-24
```

### „ALE einlesen“ heute

- **Ablauf:** Marlon lädt die ALE nach dem Kartenwechsel in der Konsole hoch. Der Server ordnet zu. Die Konsole zeigt die Liste mit den Gruppen zugeordnet, mehrdeutig, ohne Clip und Clip ohne Take. Einzelfälle werden von Hand korrigiert (`docs/ABLAEUFE.md:116-120`).
- **Wo die ALE liegt,** ist offen. Heute wird sie in der Konsole hochgeladen, ein beobachteter Ordner ist als spätere Lösung vorgesehen (`docs/ABLAEUFE.md:138`). Im Projekte-Konzept soll die Konsole die ALE im Kameraordner automatisch erkennen (`docs/projekte/KONZEPT-PROJEKTE.md:313`).
- **Befehl:** `ale.importieren` mit den Daten `{ dateiname (1–200 Zeichen), inhalt (Text der ALE, bis 10 MB) }` (`packages/shared/src/protokoll.ts:205-206`).
  - Er geht als WebSocket-Umschlag an den Stage-Server, Pfad `/client` (`apps/server/src/index.ts:1362-1366`).
  - Aufbau des Umschlags: `{ v: 1, art: "befehl", id, bezug, zeit, typ: "ale.importieren", daten }` (`protokoll.ts:18-29`).
- **Verarbeitung** (`apps/server/src/index.ts:1023-1059`):
  1. Die ALE wird gelesen. Enthält sie keine Clips, gibt es einen Fehler.
  2. Nur beendete Takes ohne Clip werden zugeordnet.
  3. Clips, die die Kamera schon per CAP gemeldet hat, werden nicht noch einmal über den Timecode vergeben. Der Abgleich läuft über den Clipnamen **ohne Endung**: CAP 1.3 liefert den Namen mit Endung, neuere Firmware ohne. Die ALE trägt `.mxf` bzw. `.mov` (`index.ts:233-234`).
  4. Die Rohdaten werden gespeichert (`ale_import`, `docs/DATENMODELL.md:159`).
  5. Takes, deren Clip die Kamera gemeldet hat, bekommen die ALE-Werte ergänzt.
  6. Antwort: `{ importId, clips, zugeordnet, mehrdeutig, unbekannteSpalten }`.
- **Zuordnungsregel** (`ale.ts:145-203`, `docs/DATENMODELL.md:151`):
  - Gewählt wird der Clip mit der grössten Timecode-Überlappung.
  - „Exakt“ heisst: Anfang und Ende liegen je höchstens 2 s auseinander.
  - Mehrdeutig ist ein Take, wenn ein zweiter Clip mindestens 50 % der besten Überlappung erreicht.
  - Trägt die ALE ein Datum, muss es zum Drehtag passen (Studiozeit Europe/Zurich).
  - Mehrere Takes dürfen denselben Clip haben.
- **Verwandte Befehle:**
  - `take.clipZuordnen` ordnet von Hand zu.
  - `take.ausClip` legt aus einem Clip ohne Take einen Take an (`protokoll.ts:207-210`).
  - **Achtung, Namensgleichheit:** `take.ausClip` hat nichts mit dem Plate-Feld `aus_clip` zu tun.
- **Zuordnungsarten** am Take: `exakt`, `ueberlappung`, `manuell`, `cap` (`protokoll.ts:460-466`).

### ALE direkt von der Kamera

- CAP kennt `CompleteALE` (`0x00a6`, ab SUP 6.0, `docs/CAP.md:44`). `docs/CAP.md` beschreibt es knapp als „ALE ergänzen“.
- Laut Spezifikation (Kapitel 7.28 und 7.29, Original nicht im Repo) liefern `GenerateALE` (`0x00a5`, pro Medium und Reel) und `CompleteALE` (pro Medium) die **ALE-Datei als Blob**.
- In der Stage ist das nicht eingebaut (**ungeprüft**).

---

## 3. ARRI CAP: was für den Ingest zählt

Quelle: `docs/CAP.md` (Zusammenfassung von CAP 1.19), `packages/shared/src/cap.ts`. CAP selbst braucht der Ingest nicht. Wichtig sind die Werte, die der Plate Assistant per CAP schreibt oder liest und die der Ingest gegen die Clips prüft.

| Feld | Typ und Grenze | Quelle | Stand |
| --- | --- | --- | --- |
| Clipname (GetClipList) | String, „Dateiname des Clips“. Je nach Firmware **mit oder ohne Endung**. | `docs/CAP.md:41`, `apps/server/src/index.ts:233-234` | **ungeprüft**: dass CAP 1.3 die Endung mitliefert, steht nur als Kommentar im Code; getestet ist nur gegen die Simulation |
| UUID | String, 36 Zeichen | `cap.ts:317-324`, CAP-Spezifikation 7.26 | ungeprüft (nur Simulation) |
| Start-TC | U32 BCD `HHMMSSFF` | `docs/CAP.md:62`, `cap.ts:255-262` | ungeprüft |
| Bilder | U32, Länge in Bildern | `cap.ts:324` | ungeprüft |
| Projekt-fps | Enum (0 23.976, 1 24, 2 25, … 6 50 …) | `docs/CAP.md:52` | ungeprüft |
| Clip Scene | String, beschreibbar, **höchstens 16 Zeichen**, nur ASCII | `docs/CAP.md:68`, `cap.ts:62, 329-331` | Spezifikation |
| Clip Take | String, beschreibbar, **höchstens 8 Zeichen** (bis 07.10.2026 stand fälschlich 16) | `docs/CAP.md:68`, `cap.ts:63, 330` | Spezifikation |
| User Info 1 / 2 | String, beschreibbar, **je höchstens 128 Zeichen**. Der Plate Assistant schreibt z. B. `PA:<take.id>`. | `docs/CAP.md:69`, `cap.ts:330` | Spezifikation, Nutzung ungeprüft |
| Current Reel / Clip Number | U16, nur Anzeige | `docs/CAP.md:71` | Spezifikation |
| Brennweite | S32 in 1/1000 mm, 0 = unbekannt | `docs/CAP.md:67` | Spezifikation |

- **Slate-Texte** werden auf ASCII 0x20 bis 0x7E gekürzt. Umlaute werden zu a, o, u (`cap.ts:331`, `docs/KONZEPT.md` 24.4).
- **Firmware im Studio:**
  - Amira: SUP **6.1.2**, CAP **1.3** (`docs/IM-STUDIO.md:81`). Damit fehlen Felder ab CAP 1.10 (Codec, Auflösung, Sensor-fps in der Clipliste).
  - ALEXA Mini: SUP 6.01.02 (`docs/KONZEPT.md:766`).
- An der echten Amira ist CAP **noch nicht geprüft**. Alles ist nur gegen die simulierte Kamera getestet (`docs/KONZEPT.md:816`, `CLAUDE.md:92`).

### Clipnamen-Muster `A001C004_260402_R132`

| Teil | Bedeutung | Beleg | Stand |
| --- | --- | --- | --- |
| `A` | Kamera-Index (Buchstabe) | Muster `^([A-Z]\d{3})C\d{3}` in `cap.ts:355-356` | Muster im Code. Bedeutung als „Kamera-Index“ nach ARRI-Gepflogenheit, in der Stage nicht erklärt |
| `001` | Reel-Nummer (Karte) | `capReel("A001C003_…") = "A001"`, `cap.ts:355-356`, `cap.test.ts:166` | Code |
| `C004` | Clip-Nummer auf dem Reel | Muster wie oben | Code |
| `260402` | Aufnahmedatum **JJMMTT** | `A001C004_260402_R132` stammt vom Mini-Dreh am 02.04.2026 (`docs/IM-STUDIO.md:108, 138`) | geprüft an einem Beispiel |
| `R132` | Kennung, die die Kamera vergibt. Taucht auch im Reel-Ordner `A001R132` auf. | `docs/IM-STUDIO.md:108` | Bedeutung **ungeprüft** |
| Endung | `.mov` (ProRes) oder `.mxf` (ARRIRAW bzw. MXF-Aufnahme) | `docs/IM-STUDIO.md:108`, `apps/server/src/index.ts:233` | `.mov` geprüft, `.mxf` nur in Tests |

**Uneinheitlicher Reel-Begriff:**
- `capReel()` liefert `A001`.
- Die ALE-Spalte `Reel_Name` im Stage-Test lautet `A001R1AB` (`ale.test.ts:16`). Der Ordner auf der Karte heisst `A001R132`.

Der Ingest sollte festlegen, was „Reel“ bei ihm heisst. Vorschlag: der volle Ordnername `A001R132`.

---

## 4. Plate-Schemas (`packages/shared/src/plate.ts`)

**Lese-Schemas der Stage** für die Tabellen `dreh`, `plate` und `take` des Plate Assistant (`plate.ts:1-4`).
- Spaltennamen sind snake_case, die Inhalte der JSON-Spalten camelCase. `aus_clip` wird zu `ausClip` (`plate.ts:59-67`).
- Textgrenzen: Namen und Stativ 1000, Team 2000, Notizen 20 000 Zeichen (`plate.ts:8-11`).
- Zeiten sind ISO 8601 mit Zeitzone (`plate.ts:12-13`).

| Schema | Felder (Kurzfassung) | Quelle |
| --- | --- | --- |
| **Dreh** | `id`, `name`, `datum` (JJJJ-MM-TT), `ort`, `produktion`, `kamera`, `team`, `notiz`, `geloescht`, `erstelltAm`, `geaendertAm` | `plate.ts:21-25` |
| **Plate** | `id`, `drehId`, `nummer`, `name`, `gps {lat, lon, genauigkeitM, hoeheM}`, `richtung {azimutGrad, …}`, `neigungGrad`, `rollenGrad` (iPhone), `kameraHoeheCm`, `abstandCm`, `stativ`, `kamera` (Kamerawerte, alle optional), `referenzen [{art, gemacht, fotos, notiz}]`, `notiz`, `geloescht`, Zeiten | `plate.ts:28-38` |
| **Take** | `id`, `plateId`, `nummer`, `startZeit`, `endZeit`, `startTc`, `endTc` (je höchstens 20 Zeichen), `clip`, `kamera`, `sonne`, `bewertung` (`circle` · `gut` · `schlecht`), `notiz`, **`ausClip`** (nullable, Vorgabe null), `geloescht`, Zeiten | `plate.ts:45-52` |
| **Take.clip** | `{ name (≤1000), uuid (≤64, Vorgabe ""), startTc (≤20), bilder (Zahl) }` | `plate.ts:42` |
| **Take.aus_clip** | `{ tiltGrad, rollGrad, tiltBereich, rollBereich }`, alles endliche Zahlen in Grad | `plate.ts:43` |
| **Kamerawerte** | `quelle` (`ale` · `cap` · `hand`), `modell`, `seriennummer`, `sensorFps`, `projektFps`, `shutter`, `iso`, `weissK`, `tint`, `nd`, `look`, `objektiv`, `brennweiteMm`, `fokus`, `blende`. Im Plate-Schema ist jeder Wert optional. | `packages/shared/src/protokoll.ts:421-433`, `plate.ts:17-18` |

**Bedeutung von `aus_clip`** (`docs/KONZEPT.md:883`):
- `tiltGrad` und `rollGrad` sind das **Mittel über den ganzen Clip**.
- `…Bereich` ist der grösste minus der kleinste Wert.
- Ab 0,5° Bereich warnt die Stage-Konsole. Die Stage nimmt `max(tiltBereich, rollBereich)` als `lageBereichGrad` (`plate.ts:84-85, 105`).
- Vorzeichen wie in der Konvention in Abschnitt 1.

**Vorrang in der Stage** (`plate.ts:69-73, 98-102`, `docs/KONZEPT.md:880`):
- Neigung und Rollen aus `take.aus_clip` gehen vor dem iPhone (`plate.neigung_grad` bzw. `rollen_grad`). Die Quelle steht in `lageQuelle` (`clip` oder `iphone`).
- Ein gesetztes `aus_clip` überschreibt also die Werte vom iPhone in der Konsole.

**Testdaten:**
- Beispielzeilen des Plate Assistant liegen in `apps/server/src/testdaten/plate-*.json`. Ein Beispiel-Clip: `{"name":"A001C003_261020_R1AB","uuid":"F4B0…","bilder":1086,"startTc":"10:45:10:12"}` (`apps/server/src/testdaten/plate-zeilen.json:12`).
- Ids mit dem Präfix `test-` sind Testzeilen (`plate.ts:86-87`).

---

## 5. Projekte und Kunden (`docs/projekte/KONZEPT-PROJEKTE.md`)

Nur die Struktur. Das Konzept ist ein Entwurf vom 25.09.2026, vieles ist nicht gebaut.

- **Kunde zuerst** (Entscheid Marlon):
  - Oben steht der Kunde, darunter getrennt `Projekte\` und `Tests\`.
  - Eigene Versuche ohne Kunden liegen unter `_Studio\Tests\`, Vorlagen unter `_Vorlagen\` (`KONZEPT-PROJEKTE.md:133-153`).

  ```
  <KUNDE>\
    Projekte\<JJJJ>_<Projekt>\
    Tests\<JJJJ-MM-TT>_<Name>\
  _Studio\Tests\…
  _Vorlagen\…
  ```

- **Kunde anlegen:** In der Konsole aus einer Liste wählen oder neu anlegen, mit **Kürzel in Grossbuchstaben** (`KONZEPT-PROJEKTE.md:93`).
- **Projektname:**
  - Eingabe als Klartext, z. B. „Tramhaltestelle“.
  - Daraus wird der Ordnername `JJJJ_<Name>` (z. B. `2026_Tramhaltestelle`).
  - Für Unreal gelten nur Buchstaben, Ziffern und Unterstrich, und die Pfade sollen kurz bleiben (`KONZEPT-PROJEKTE.md:94`).
  - Ein älterer Vorschlag im selben Dokument lautet `JJJJ_Produktion_Titel` (`KONZEPT-PROJEKTE.md:268`), der Abschnitt „Kunde zuerst“ ist neuer.
- **Projektordner** (Vorschlag, zweiter Schritt): `00_Kunde`, `01_Referenzen`, `02_Texturen`, `03_DCC`, `04_Scans`, `05_Unreal`, `06_Export`, `07_Screenshots`, `projekt.json` (`KONZEPT-PROJEKTE.md:113-125`). Ein Ordner für Footage ist dort nicht vorgesehen.
- **Kamerakarten** im Konzept:
  - Ablage unter `Kamera/<Produktion>/<Drehtag>/<Reel>` mit MHL, nur lesend.
  - Kopie über Hedge OffShoot oder ShotPut Pro (`KONZEPT-PROJEKTE.md:263, 313`).
  - Das ist **nicht gebaut** und durch Stage Ingest überholt.
- **Gebaut ist nur ein einfaches Projekt-Register** in der Stage-Datenbank: Tabelle `projekt` mit `name`, `dataset`, `pfad`, `windows_pfad`, `status` (`aktiv` · `ruhend` · `archiv`) und `stuendlich` (`apps/server/src/db/schema.ts:205-213`, Befehl `projekt.anlegen` in `protokoll.ts:246`).
  - **Eine Kunden-Tabelle gibt es nicht.** Die Kundenliste der Konsole ist noch nicht gebaut.
  - Für „Kunden- und Projektnamen der Stage-Konsole“ gibt es heute also keine Schnittstelle (**offen**).

---

## 6. Aufbau einer ARRI-Karte (Mini/Amira)

**In der Stage nicht beschrieben (offen).** Belegt ist nur:

| Befund | Quelle | Stand |
| --- | --- | --- |
| Ein Mini-Reel liegt nach dem Kopieren als Ordner `A001R132`. Die Clips liegen darin direkt als `A001C004_260402_R132.mov`. | `docs/IM-STUDIO.md:108` | geprüft (ein Reel) |
| ProRes 422 HQ (`apch`), z. B. 1,3 GB für 33 s | `docs/IM-STUDIO.md:109` | geprüft |
| Die Kamera schreibt pro Medium eine ALE mit allen Clips | `docs/CHATVERLAUF.md:827` (Aussage im Gespräch), `docs/IM-STUDIO.md:138` (echte ALE des Mini-Reels) | geprüft, dass eine ALE existiert. Ort auf der Karte offen |
| Clips der Mini über 19 Clips, rund 17 000 Bilder | `docs/KONZEPT.md:766` | geprüft |

**Offen:**
- Dateisystem der Karte: ARRI-UDF bei CFast/Codex? Das steht nur im Ingest-Konzept.
- Ordnerebenen über dem Reel-Ordner.
- Begleitdateien wie XML, Thumbnails, MHL der Kamera oder Look-Dateien.
- Wo die ALE auf der Karte liegt.
- Wie ARRIRAW (`.ari`-Folgen bzw. MXF) auf der Karte aussieht.

Dafür eine echte Mini- und eine Amira-Karte einmal vollständig auflisten.

---

## Offene Punkte für den Ingest (Zusammenfassung)

1. Vorzeichen von ART CMD (tilt, roll) gegen die Konvention prüfen.
2. ART CMD mit ProRes-MOV der Mini und der Amira ausdrücklich bestätigen. Liefert die Amira überhaupt tilt und roll?
3. Vollständige Spaltenliste eines ART-CMD-Exports, voller Pfad und Einheit von `lensEntrancePupilOffset`.
4. Echte ALE der Amira und der Mini: Spaltennamen, Lage auf der Karte.
5. Ordnerstruktur und Begleitdateien einer echten Mini- und Amira-Karte.
6. Reel-Begriff festlegen (`A001` oder `A001R132`).
7. Schnittstelle für Kunden- und Projektnamen: in der Stage noch nicht vorhanden.
8. Übergabe der ALE an den Stage-Server: heute nur `ale.importieren` über die WebSocket-Konsole, kein Datei-Endpunkt.
