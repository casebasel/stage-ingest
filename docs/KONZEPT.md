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

**Simulation (`kern/tests/simulation_kopie.rs`, Marlon 17.10.2026: „da darf wirklich kein Fehler passieren“).** Zufällige Karten gehen durch den echten Ablauf (`ablauf::sichern`). Sie enthalten Clips, ARRIRAW-Bildordner, Begleitdateien, Dateien des Betriebssystems, leere Ordner und Grössen an den Blockgrenzen. Gestört wird so:
- Abbruch mitten im Kopieren,
- halb geschriebener Lauf mit `…ingest-teil`,
- weitergedrehte Karte (Ergänzen),
- frühere Kopie nachprüfen, auch verfälscht,
- Karte mit verändertem Clip,
- Bitfehler zwischen Schreiben und Zurücklesen,
- Karte liefert beim zweiten Lesen anderes,
- zwei Ziele auf derselben Platte,
- endgültiges Löschen eines Clips.

Geprüft wird:
- Freigabe genau dann, wenn genug unabhängige Kopien Byte für Byte der Karte gleichen.
- Eine gestörte Kopie zählt nie.
- Eine frühere Kopie wird nie überschrieben.
- Nach einem Abbruch sind nur neu begonnene Ziele weg.
- Jede gute Kopie besteht ihre ASC-MHL-Historie, eigene Prüfung und Referenz `ascmhl verify`.

Stand 17.10.2026: 2000 Karten ohne Fehler (`SIM_KOPIE_ANZAHL=2000 ASCMHL_DEBUG=<ascmhl-debug> cargo test -p ingest-kern --release --test simulation_kopie -- --nocapture`).

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
  07_RENDER/                 Renders aus Unreal (gehört der Stage)
```

**`07_RENDER/` (Stage, von Marlon bestätigt am 17.10.2026):** Das Plugin der Stage (0.18.0) legt Renders aus Unreal unter `07_RENDER/<Clipname>_<Sequence>.mov` ab, ohne Clipnamen `<Slate>_T<NN>_<Sequence>.mov`, nie überschrieben. Es findet die Ablage über die Merkdatei des Ingest und den Drehordner über `karte.speicherort` (bevorzugt der des Aufnahmetags), sonst nach derselben Regel wie der Ingest. Stage Ingest schreibt nie in `07_RENDER/` und zählt es nicht zur Karte.

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

**Simulation (`src-tauri/src/simulation.rs`):** Zufällige, realistische Drehtage mit bekannter Wahrheit durch denselben Code wie die App (Plan → Soll-Liste → Abgleich → Zusammenfassung → Projektseite): Verbindung reisst ab und kommt wieder, Klappen ohne REC, Aufnahmen nur an der Kamera, QR, Kamerauhr auf 2012, verschobener Timecode und Sprung mitten im Tag, B-Kamera, Platzhalter, Kartenwechsel, zweiter Drehort auf derselben Karte, gelöschte Takes. Danach übernimmt die Simulation alle Vorschläge wie Marlon und rechnet neu, bis keiner mehr kommt, und ordnet zur Gegenprobe einen Clip von Hand falsch zu. Geprüft: keine falsche Zuordnung, kein falscher Vorschlag, kein Clip verloren oder doppelt, kein Take mit Aufnahme als „ohne Aufnahme“, kein „Karte fehlt“ bei eingelesenen Karten, keine Unstimmigkeit ohne Grund. Seit 17.10.2026 auch der Plate Assistant ab Build 34 an der Hälfte der Tage: Takes ohne REC entfernt die App mit Verbindung selbst (die Kamera behält ihre Take-ID in Info 1), für Aufnahmen nur an der Kamera legt sie einen Take mit Nachfrage an, und der erwartete Name kommt von der Kamera mit ihrem Datum (oft genau der Name des nächsten echten Clips). Dazu ein Fehler im Plate Assistant: 2 % der Takes mit Aufnahme entfernt er trotzdem (die Meldung der Kamera kam nicht an); Marlon stellt sie oben in „Zu klären“ wieder her, wenn der Clip laut Wahrheit dazugehört, sonst „Gehört nicht dazu“. Geprüft zusätzlich: Jeder solche Hinweis hat einen Beleg im Clip, über den QR nie ein Fehlalarm, kein Clip steht oben und zugleich unter „Clips ohne Take“. Stand 17.10.2026: 100 000 Drehtage (1,2 Mio. Clips; 49 650 Tage mit Build 34, 47 089 entfernte Takes, 14 365 Takes mit Nachfrage; 9 840 fälschlich entfernte Takes, davon 8 258 oben gemeldet und wiederhergestellt, die übrigen ohne QR und Take-ID im Clip unter „Clips ohne Take“; 6 346× „Gehört nicht dazu“, weil die Kamera die Take-ID des entfernten Takes in Info 1 behielt) ohne Fehler, 98 % der falschen Zuordnungen von Hand gemeldet (`SIM_ANZAHL=100000 cargo test -p stage-ingest --release drehtage -- --nocapture`, einzeln `SIM_NR=<n>`). Mit echten Daten: Test `echtes_projekt_pruefen` (Plan und Zusammenfassungen nur gelesen).

**„Zu klären“ (Projektseite):**
- **Offene Clips** bekommen Vorschläge nach denselben Regeln (je Karte, schon zugeordnete Clips als sichere Bezugspunkte, alle Takes des Projekts ohne Clip auf einer anderen Karte); „Alle Vorschläge übernehmen“. Ein neuer Take wird nur vorgeschlagen, wenn der Clip sicher im Abschnitt eines Takes beginnt, der seinen Clip schon hat, und kein Take (auch kein gelöschter) diesen Clip nennt. Warum ein Clip offen bleibt, steht darunter (z. B. „Take-ID im ersten Clip der Karte“). Beim Zuordnen schreibt der Ingest `take.clip {name, startTc, bilder}` wie CAP nach dem Stopp, wenn die Kamera am Take keinen Namen bestätigt hat (`clipBestaetigt`); ein früher von Hand geschriebener Name wird dabei ersetzt. Ein von Hand geschriebenes `take.clip` (ohne `cap` und ohne Timecode beim REC) gilt nur als geplanter Name; für Abgleich und Anzeige zählt dann der Vorschlag der App (`plate::clipname_im_plan`), damit ein umgehängter oder gelöster Clip nicht am alten Take hängen bleibt. Löschen kann der Ingest `take.clip` nicht (nur Name, Timecode, Bilder erlaubt), der Plate Assistant zeigt den alten Namen also weiter.
- **Takes ohne Clip** (bis 0.1.55 „Takes ohne Aufnahme“): Takes, deren Klappe an eine eingelesene Karte grenzt, ohne Clip darauf. **Eindeutig** ohne Clip nur, wenn die Klappe zwischen zwei Takes mit Clip auf derselben Karte (oder auf zwei aufeinanderfolgenden Rollen) liegt, kein offener Clip für ihn in Frage kommt, die Kamera kein REC gemeldet hat, an dem Tag nur eine Kamera drehte, jeder offene Clip dort eine Zeit hat und er kein Platzhalter ist; sonst **unklar** mit Grund. Die Begründung nennt die Nachbarn („Zwischen Take 1 (A009C008) und Take 5 (A009C009) liegt kein Clip für diesen Take“). Grenzt die Klappe an keine eingelesene Karte oder liegt eine Rolle dazwischen nicht vor: „Karte fehlt“. **„Ohne Clip bestätigen“** (auch alle eindeutigen auf einmal; Marlon, 17.10.2026: „Ich möchte immer alles behalten, es werden mir nie Takes gelöscht“): Der Take bleibt im Plate Assistant, die Bestätigung steht mit Zeit und Rechner in jeder Zusammenfassung der Karte (`ohne_aufnahme`, gilt auf Mac und Windows, bleibt beim Ergänzen); in der Take-Liste heisst er danach „Ohne Clip“, ohne den vom iPhone vorhergesagten Namen. Bestätigen braucht die Karte auf einem verbundenen Ziel; steht die Liste nur aus der Datenbank, sagt die Seite das einmal oben und sperrt die Knöpfe. Löschen nur ausnahmsweise über das Papierkorb-Symbol mit deutlicher Warnung (`take.geloescht`, im Plate Assistant wiederherstellbar). Der Ingest löscht nie selbst. Der Plate Assistant entfernt ab Build 34 einen Take ohne Clip weich selbst (mit Rückgängig), aber nur, wenn die Kamera verbunden war und sicher nichts aufgenommen hat (Systemkarte 84f9061, von Marlon bestätigt); solche Takes kommen gelöscht an und erscheinen nicht unter „Takes ohne Clip“ (Simulation: 47 089 solche Takes, ohne Fehler); trägt ein Clip ihre ID, stehen sie unter „Widersprüche“ (unten). An bestehenden Takes schreibt der Ingest nur `take.clip` und `take.geloescht`, dazu beim Wiederherstellen `geloescht = false` nur als Paar mit `clip_name` desselben Takes (Sperre in `src-tauri/src/plate.rs`, mit Test).
- **Widersprüche** (bis 0.1.55 „Unstimmigkeiten“; Marlon, 17.10.2026: „Unstimmigkeiten müssen in Zu klären erscheinen“): Jede Zuordnung wird nur mit den Belegen der Clips gegengeprüft (QR, Take-ID, Name von der Kamera, Timecode, Zeit; ohne die Zuordnungen selbst). Gemeldet wird, wenn ein sicherer Beleg einen anderen Take nennt, der Clip nicht im Abschnitt seines Takes beginnt, der Take laut Beleg einen anderen Clip derselben Kamera hat, oder ein Take zwei Clips derselben Kamera hat. Je Fall: „Neu zuordnen“ (Take der Belege), „Zuordnung aufheben“ (der Clip steht wieder unter „Clips ohne Take“) oder „Beibehalten“ (`stimmt_so` in der Zusammenfassung; ändert sich die Lage, erscheint er wieder). Für Zusammenfassungen vor 0.1.50 ohne `info1` liest die App die Take-ID aus den ersten 64 KB der Clipdatei nach (ARRI legt sie nach etwa 7 KB ab), nur im Speicher. Echte Daten 17.10.2026: genau die drei falschen Zuordnungen im Treppenhaus (C010 bei Take 6, C011 bei Take 7, C012 bei Take 8; laut Take-ID und REC gehört C010 zu Take 8, C011 und C012 sind weitere Aufnahmen danach).
- **Clip zu einem automatisch entfernten Take** (Systemkarte 499a1fc, Marlon 17.10.2026: „Die App stellt nichts selbst wieder her“): Trägt ein Clip ohne Take die ID eines Takes, den der Plate Assistant automatisch entfernt hat (gelöscht, ohne Clipnamen, ohne Clip, ohne REC-Timecode; von Hand Gelöschte behalten ihren Namen), im QR der Klappe oder in Info 1, oder ist er ihm zugeordnet, steht er unter „Widersprüche“: „Plate 1 · Take 2 wurde automatisch entfernt (die Kamera meldete dem Plate Assistant keine Aufnahme). Dieser Clip trägt seine ID …“, bei Info 1 mit dem Hinweis, dass die Kamera sie bis zur nächsten Klappe behält. Er steht nicht zusätzlich unter „Clips ohne Take“. **„Take wiederherstellen“** (mit Rückfrage): Der Ingest prüft, dass der Clip die ID trägt und der Take noch automatisch entfernt ist, setzt `take.geloescht = false` und `take.clip_name` (Name des Clips) in einer Anfrage, ordnet den Clip von Hand zu (Zusammenfassungen und Datenbank) und trägt `take.clip` ein. Hat der Plate Assistant den Take inzwischen geändert, meldet er das, statt Erfolg zu melden. **„Gehört nicht dazu“**: `stimmt_so`; hing der Clip am entfernten Take, wird die Zuordnung vorher aufgehoben, damit er nicht an einem gelöschten Take verschwindet. Danach steht er unter „Clips ohne Take“, mit Vorschlag, wenn es einen gibt. Bewusst ohne Ausnahme für Info 1: Hätte die Zuordnung einen anderen Vorschlag (Simulation: 96 von 3298 Fällen, stets eine veraltete Info 1), bleibt der Hinweis trotzdem oben, weil der entfernte Take beim Zeitfenster fehlt und der Vorschlag gerade dann irren kann; Marlon entscheidet am Bild. Echte Daten (Take 2 im Predigerhof als entfernt nachgestellt): erscheint mit und ohne bestehende Zuordnung genau so, sonst ändert sich nichts.

**Eindeutige Zuordnungen nachtragen (Marlon, 17.10.2026: „automatisch nachtragen“):** Die Projektseite zeigt einen Take als „Sicher“, sobald der Clipname am Take von der Kamera bestätigt ist (CAP, gleicher Timecode) und der Clip auf einer eingelesenen Karte liegt, auch wenn das Einlesen die Zuordnung damals noch nicht kannte. In Zusammenfassung und Datenbank stand dann kein Take, der Plate Assistant zeigte den Take ohne Clip (4 stock P2 T2/T3, Treppenhaus T10). Beim Öffnen der Projektseite trägt die App solche Zuordnungen nach (`Uebersicht::nachzutragen`, Befehl `zuordnungen_nachtragen`): erst die Datenbank (`clip.take_id`, `dreh_id`, `zuordnung = clipname`), dann jede erreichbare Kopie der Zusammenfassung. Nur wenn der Clip nirgends einem Take, einem Drehort oder von Hand zugeordnet ist, nicht gelöscht, nicht ungeprüft, keinem zweiten Take zugeordnet, ohne Widerspruch, und die Karte in der Datenbank steht; nie an gelöschte Takes. Die Simulation trägt in jeder Runde nach und prüft, dass sich die Ansicht dabei nicht ändert und danach nichts mehr nachzutragen bleibt.

**Wortwahl (Marlon, 17.10.2026: „branchenübliche Bezeichnungen … wie in der Filmindustrie Ingest-App gesprochen wird“):** abgeglichen mit den Handbüchern von Silverstack, Offload Manager, ShotPut Pro, OffShoot, DaVinci Resolve (Clone), LockitScript und der deutschen Offload-App Offloadr. „sicher“ heisst nur noch freigegeben („Sicher zum Formatieren“, Take „Sicher“); die Gewissheit einer Zuordnung heisst „eindeutig“ oder „unklar“ (in einer Ingest-App las sich „Alle sicheren bestätigen“ wie „alle sichern“). Gegenstücke heissen spiegelbildlich: „Clips ohne Take“ (Pomfort/Lockit: unmatched clips) und „Takes ohne Clip“ (nicht „ohne Aufnahme“: Im Filmdeutsch heisst ein Take selbst Aufnahme). Widersprüche statt Unstimmigkeiten; Knöpfe nennen ihr Ergebnis: „Neu zuordnen“, „Zuordnung aufheben“, „Beibehalten“, „Ohne Clip bestätigen“. Ein Wort, eine Bedeutung: „Kopie ergänzen“ nur beim Einlesen (weitere Kopie von der Karte); eine Kopie aus einer Kopie heisst „Fehlende Kopie erstellen (Kaskade)“, englisch „Cascading Copy“. Je Teil von „Zu klären“ ein Satz, was er ist und was zu tun ist. Tilt/Roll statt Neigung/Rollen und Drehort statt Dreh wie in `BEGRIFFE.md`.

**Clips aus abgebrochenem Einlesen:** liegen im Kartenordner, stehen aber in keiner Zusammenfassung (Läufe, die vor 0.1.47 an Fehler 45 bzw. „Unzulässige Funktion“ scheiterten). Sie machen keinen Take „sicher“, erscheinen in „Zu klären“ mit Vermerk „nicht gegen die Karte geprüft“ und Vorschlag; beim Bestätigen kommen sie mit `ohne_pruefung` in die Zusammenfassung und als Zeile `clip` in die Datenbank. Das endgültige Löschen verweigert, solange eine Kopie Dateien enthält, die die ASC-MHL-Historie nicht kennt (sonst würde die neue Historie sie als geprüft führen).

**Geplanter Name und Karte nicht erreichbar (Marlon, 17.10.2026: „Der geplant relativiert sich ja, wenn er gedreht wurde“):** Der Plate Assistant schlägt bei jeder Klappe „voriger Name plus eins“ mit dem Datum des iPhones vor und lässt den Vorschlag stehen, wenn die Klappe ohne REC endet. Der nächste Take bekommt dann denselben Zähler, bestätigt von der Kamera mit ihrem Datum (Predigerhof P6: T2 geplant `A010C023_261016`, T3 hat `A010C023_120101`). Bekam ein anderer Take den Clip mit gleicher Rolle und gleichem Zähler, zeigt der Ingest den geplanten Namen nicht mehr. Ist die Karte gerade nicht erreichbar (z. B. Mac ohne NAS), rechnet die Projektseite mit den Clips der gemeinsamen Datenbank (Timecode, Zuordnung, `ordner_datum`/`speicherort`), damit ein Take ohne Clip nicht als „Karte fehlt“ erscheint. Bestätigen geht dann erst, wenn die Karte wieder erreichbar ist.

**Darstellung je Take (Marlon, 17.10.2026, „zweimal dasselbe Bild“):** Die Take-Tabelle zeigt den Clip, wie er auf der Karte heisst (der geplante Name der App bleibt als `clip_geplant` daneben); ein Take ohne Clip heisst „Ohne Clip“ und zeigt keinen Namen; liegt seine Karte noch nicht vor, steht der Name laut Plate Assistant als „erwartet“ da; ein aus abgebrochenem Einlesen aufgenommener Clip macht den Take „Ungeprüft“, nie „Sicher“. Je Take höchstens eine Vorschau-Kachel: hat er ein Mittelbild (`foto.quelle = clip`), fällt das Standbild der Kamera (`quelle = kamera`) weg; Setup-Fotos zuerst, dann die Takes nach Nummer. Fehlende Mittelbilder holt die App nach dem Laden der Projektseite im Hintergrund nach (ART CMD, eines nach dem anderen, je Take höchstens eines). Ein Foto, das im Speicher fehlt, heisst „noch nicht hochgeladen“. Das Löschprotokoll in `04_BERICHTE/` gilt auch, wenn ein abgebrochener Lauf den Clip in der Zusammenfassung liess. Ein Clip, der einem Drehort ohne Plates zugeordnet ist, gilt als erledigt.

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

**Protokoll (Marlon, 17.10.2026: „einen Verlauf der Importvorgänge, wo man visuell sieht, was alles auf der Karte war und dass alles kopiert ist … ein sicheres Gefühl“):** eigene Seite neben „Einlesen“. Links alle eingelesenen Karten auf allen erreichbaren Zielen und Ablagen (auch was ein anderer Rechner eingelesen hat) und mit Netz alle aus der gemeinsamen Datenbank (siehe „Alle Ingest-Rechner synchron“), rechts je Karte: Urteil, Kopien mit Pfad und Stand, Verlauf, Dateien. Gelesen werden nur die ASC-MHL-Historien, die Lösch- und Bereinigungsprotokolle in `04_BERICHTE/` und die Ordnerliste, nicht der Inhalt der Dateien; das volle Nachlesen bleibt „Nachprüfen“ (`src-tauri/src/protokoll.rs`, `src/seiten/Protokoll.tsx`).

- **Dateien wie im Finder** (Marlon, 17.10.2026): Liste mit Dreieck zum Aufklappen, Ordner- und Dateizeichen nach Art, Zebrastreifen, Spalten Name · Geändert (`lastmodificationdate` der Karte) · Grösse · Art und je Kopie der Stand mit Zeichen und Wort. Sortieren über die Spaltenköpfe, alle Ordner auf- oder zuklappen. Ordner mit mehr als 40 Dateien (ARRIRAW-Bilder) sind anfangs zu. **Als Liste oder als Symbole** (Marlon, 17.10.2026: „eine Listenansicht und eine Symbolansicht mit der Bildvorschau … so wie bei macOS“), Umschalter wie im Finder, gemerkt je Rechner. Die Symbolansicht zeigt je Ordner Kacheln mit dem Mittelbild jedes Clips (ART CMD, sonst Quick Look; erst wenn die Kachel sichtbar wird, ein Bild je Clip, im Zwischenspeicher der App), dazu Name, Grösse, Datum und den Stand über alle Kopien; ein ARRIRAW-Ordner zeigt das Bild aus seiner Mitte. Pfadleiste und Zurück wie im Finder; höchstens 240 Kacheln je Ordner. **Doppelklick** (oder Eingabetaste) öffnet: Clips im Player der App (mit den übrigen Clips zum Weiterschalten), Ordner in der Symbolansicht, alles andere im Standardprogramm; geöffnet wird die Datei auf der ersten verbundenen Kopie, auf der sie liegt.
- **Geprüft** heisst: Die Datei steht mit derselben Prüfsumme in einer Generation, die beim Einlesen geschrieben wurde (aktuelle oder beiseitegelegte Historie in `05_METADATEN/frueher/`), und liegt heute mit gleicher Grösse da. Eine Generation, die beim Löschen oder Bereinigen entstand (gleiche Sekunde wie das Protokoll), belegt das nicht. **Ungeprüft** (gold): liegt da, aber kein Einlesen hat sie mit der Karte verglichen. Rot: fehlt, andere Prüfsumme, Prüfung fehlgeschlagen, nicht mehr da. **Gelöscht** (grau, durchgestrichen, mit Zeit und Benutzer): laut Löschprotokoll von Hand gelöscht; Angaben aus der beiseitegelegten Historie.
- **Verlauf** aus allen Historien und Protokollen: eingelesen und geprüft, nachgeprüft, von Hand gelöscht (mit oder ohne neue Historie), Historie bereinigt; je Vorgang Rechner, Benutzer, Werkzeug und Kopien.

**Alle Ingest-Rechner synchron (Marlon, 17.10.2026: „Es wäre einfach grandios, wenn alle Ingest-Apps synchron sind mit allem, was sie haben und gemacht haben. Die Files liegen ja auf dem NAS, sind ja nur Metadaten“; gewählt: „Alles über Datenbank“):** Anlass: Der i9 las A010R132 auf das NAS ein, das MacBook ohne verbundenes NAS sah die Karte im Protokoll nicht. Bis dahin lagen Verlauf (`verlauf.jsonl`) und Kartengedächtnis nur auf dem Rechner, der eingelesen hatte; Prüfsummen, Lösch- und Bereinigungsprotokolle und die Entscheidungen in „Zu klären“ nur bei den Kopien. Der Austausch läuft über die gemeinsame Datenbank, die auch ohne NAS erreichbar ist; die Dateien bleiben, wo sie sind.
- **Stufe 1 (0.1.59, ohne Änderung an der Datenbank):** Das Protokoll zeigt mit Netz auch alle Karten aus `karte`/`clip`, die hier auf keinem Ziel liegen: Datum, Projekt, Drehordner, Freigabe, unabhängige Kopien, Art der Ablage (`karte.ablage`: „NAS“ oder „Platte …“; nicht der Name der Merkdatei) mit dem Ort im Projektordner als Kopie „nicht verbunden“, dazu die Clipnamen. Dieselbe Kopie ist, was hier am selben Ort im Projektordner liegt. Das Kartengedächtnis fragt nach dem eigenen Verlauf die Datenbank, unter dem Namen des Kartenlaufwerks und der Rolle (Reel): gleiche Clips heisst „schon eingelesen auf einem anderen Rechner (Kopie auf „NAS“)“, alle von damals da und neue dazu heisst „nicht formatiert“. Ohne Netz bleibt alles wie bisher. Echte Daten: ohne NAS alle sieben Karten der Datenbank (A010R132 mit 27 Clips), mit NAS-Spiegel keine doppelte Kopie.
- **Lesen in Seiten:** Der Server liefert je Abfrage höchstens 1000 Zeilen (`PGRST_DB_MAX_ROWS`). Der Ingest fragt mit `Prefer: count=exact` und holt den Rest geordnet nach `id` in Seiten; eine Abfrage mit eigenem `limit` bleibt, wie sie ist (`plate.rs` `alle_seiten`, Test). Vorher hätte die Projektseite ab 1000 Clips je Projekt still Clips verloren.
- **Stufe 2 (geplant, Migration 0030 des Plate Assistant, erst mit Marlons OK an ihn und nach dem Probelauf):** Tabelle `ingest_vorgang`, eine Zeile je Vorgang (eingelesen, ergänzt, Kaskade, nachgeprüft, gelöscht, bereinigt, einsortiert, Ohne Clip bestätigt, Beibehalten) mit Zeit, Rechner, Benutzer, Version, Kopien (`ablage_id` aus der Merkdatei, Name, Art, Pfad im Projektordner, Ergebnis), Dateiliste mit Grösse und XXH3, Fingerabdruck, Freigabe. Nur anhängen (danach ist nur `geloescht` änderbar), Schreiben über `aenderungen_anwenden` nur mit Ingest-Recht; ohne Netz in einem Ausgangskorb, der die Änderungs-ID behält (Wiederholung = „doppelt“); die Karte geht vor ihrem Vorgang hinaus. Damit zeigt jedes Protokoll Verlauf, Dateien und Prüfsummen aller Rechner auch ohne NAS, das Kartengedächtnis vergleicht genau, und „Zu klären“ lässt sich ohne verbundene Kopie bestätigen.

**Löschen nacheinander, Historie bereinigen (17.10.2026):** Am 16.10.2026 liefen zwei Löschvorgänge auf derselben Kopie gleichzeitig (0.1.47, macOS über SMB). Der eine las die Historie, während der andere sie beiseitelegte: Die Historie von A008R56E nennt seither den gelöschten A008C001 noch, und in A007R132 liegt „0001“ zweimal im Ordner `ascmhl` (die Kette nennt eine; die Referenz liest beide). Ausserdem nahm das Löschen damals Dateien, die die Historie nicht kannte, aus der Kopie selbst in die neue Historie auf (seit 0.1.50 gesperrt); das Protokoll zeigt sie als ungeprüft. Seither: Im Kern läuft immer nur ein Lösch- oder Bereinigungslauf (Test `gleichzeitig_loeschen_laeuft_nacheinander`, ohne Sperre 6 von 6 Läufen falsch); die App belegt dafür denselben Zustand wie Kopieren und Prüfen, und die Projektseite reiht mehrere Löschaufträge nacheinander ein. „Bereinigen“ im Protokoll (`loeschen::historie_bereinigen`) bringt solche Kopien in Ordnung: Nennt die Historie von Hand gelöschte Dateien, wandert sie nach `05_METADATEN/frueher/<Karte>_<Zeit>_vor_bereinigen/` und die Kopie bekommt eine neue über das Verbleibende (Hashes aus der alten); sonst wandern nur Generationen ausserhalb der Kette nach `…_ausserhalb_kette/`. Nichts wird gelöscht oder neu gelesen; Protokoll `04_BERICHTE/<Karte>_BEREINIGT_<Zeit>.txt`. Fehlt eine Datei ohne Löschprotokoll oder liegt eine Datei da, die die Historie nicht kennt, geschieht nichts.

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
