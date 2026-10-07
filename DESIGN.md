---
name: Stage Ingest
description: Kamerakarten mit Beweis kopieren – im Kategorie-Standard von Silverstack, ShotPut Pro und DaVinci Resolve.
colors:
  fenster: "oklch(0.150 0.004 255)"
  panel: "oklch(0.190 0.004 255)"
  panel-2: "oklch(0.215 0.005 255)"
  feld: "oklch(0.140 0.004 255)"
  hover: "oklch(0.240 0.006 255)"
  linie: "oklch(0.290 0.006 255)"
  linie-stark: "oklch(0.420 0.008 255)"
  text: "oklch(0.940 0.003 255)"
  text-2: "oklch(0.820 0.006 255)"
  text-3: "oklch(0.745 0.008 255)"
  akzent: "oklch(0.470 0.150 252)"
  akzent-hover: "oklch(0.520 0.150 252)"
  akzent-text: "oklch(0.760 0.120 250)"
  auf-akzent: "oklch(0.990 0 0)"
  auswahl: "oklch(0.340 0.085 252)"
  auswahl-leise: "oklch(0.260 0.050 252)"
  ok: "oklch(0.760 0.150 150)"
  warn: "oklch(0.820 0.140 80)"
  fehler: "oklch(0.710 0.170 25)"
  ok-grund: "oklch(0.285 0.065 150)"
  warn-grund: "oklch(0.300 0.060 80)"
  fehler-grund: "oklch(0.290 0.075 25)"
  gefahr: "oklch(0.480 0.190 25)"
typography:
  urteil:
    fontFamily: "Geist Variable, -apple-system, BlinkMacSystemFont, Segoe UI, system-ui, sans-serif"
    fontSize: "26px"
    fontWeight: 700
    lineHeight: 1.15
    letterSpacing: "-0.015em"
  headline:
    fontFamily: "Geist Variable, -apple-system, BlinkMacSystemFont, Segoe UI, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 650
    letterSpacing: "-0.01em"
  title:
    fontFamily: "Geist Variable, -apple-system, BlinkMacSystemFont, Segoe UI, system-ui, sans-serif"
    fontSize: "18px"
    fontWeight: 650
    letterSpacing: "-0.01em"
  block-titel:
    fontFamily: "Geist Variable, -apple-system, BlinkMacSystemFont, Segoe UI, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 650
  body:
    fontFamily: "Geist Variable, -apple-system, BlinkMacSystemFont, Segoe UI, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
  label:
    fontFamily: "Geist Variable, -apple-system, BlinkMacSystemFont, Segoe UI, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 500
  mono:
    fontFamily: "Geist Mono Variable, ui-monospace, SF Mono, Consolas, monospace"
    fontSize: "0.94em"
    fontFeature: "tnum"
rounded:
  klein: "4px"
  rund: "6px"
  urteil: "8px"
spacing:
  "2": "2px"
  "4": "4px"
  "6": "6px"
  "8": "8px"
  "12": "12px"
  "16": "16px"
  "20": "20px"
components:
  knopf:
    backgroundColor: "{colors.panel-2}"
    textColor: "{colors.text}"
    rounded: "{rounded.klein}"
    padding: "0 12px"
    height: "28px"
  knopf-hover:
    backgroundColor: "{colors.hover}"
  knopf-haupt:
    backgroundColor: "{colors.akzent}"
    textColor: "{colors.auf-akzent}"
    rounded: "{rounded.klein}"
    padding: "0 12px"
    height: "28px"
  knopf-haupt-hover:
    backgroundColor: "{colors.akzent-hover}"
  knopf-haupt-gross:
    backgroundColor: "{colors.akzent}"
    textColor: "{colors.auf-akzent}"
    rounded: "{rounded.rund}"
    padding: "0 20px"
    height: "38px"
  knopf-gefahr:
    backgroundColor: "{colors.gefahr}"
    textColor: "{colors.auf-akzent}"
    rounded: "{rounded.klein}"
    padding: "0 12px"
    height: "28px"
  eingabe:
    backgroundColor: "{colors.feld}"
    textColor: "{colors.text}"
    rounded: "{rounded.klein}"
    padding: "0 8px"
    height: "28px"
  block:
    backgroundColor: "{colors.panel}"
    rounded: "{rounded.rund}"
  block-kopf:
    backgroundColor: "{colors.panel-2}"
    typography: "{typography.block-titel}"
    padding: "6px 12px"
    height: "40px"
  tabellen-kopf:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text-3}"
    typography: "{typography.label}"
    padding: "6px 12px"
  tabellen-zeile-gewaehlt:
    backgroundColor: "{colors.auswahl}"
    textColor: "{colors.text}"
  kopfleiste:
    backgroundColor: "{colors.panel-2}"
    height: "44px"
  urteil-ok:
    backgroundColor: "{colors.ok-grund}"
    textColor: "{colors.text}"
    typography: "{typography.urteil}"
    rounded: "{rounded.urteil}"
    padding: "20px 22px"
  urteil-warn:
    backgroundColor: "{colors.warn-grund}"
    textColor: "{colors.text}"
    rounded: "{rounded.urteil}"
    padding: "20px 22px"
  urteil-fehler:
    backgroundColor: "{colors.fehler-grund}"
    textColor: "{colors.text}"
    rounded: "{rounded.urteil}"
    padding: "20px 22px"
---

# Design System: Stage Ingest

## Overview

**Creative North Star: "Das vertraute Offload-Pult"**

Stage Ingest sieht aus und bedient sich wie die etablierten Werkzeuge der Kategorie: Silverstack und ShotPut Pro für den Ablauf (Quellen links, Auftrag in der Mitte, Ziele als Tabelle), DaVinci Resolve für die Seitenreiter im Kopf und die neutralen Graphit-Flächen. Vertrautheit ist gewollt; wer diese Programme kennt, findet sich ohne Einweisung zurecht. Eigenheiten werden nicht erfunden, die Sorgfalt steckt in Dichte, Lesbarkeit und Zuständen.

Die Oberfläche ist dicht und ruhig: 13-px-Grundschrift, Haarlinien statt Schatten, Blöcke mit Kopfleiste, Tabellen mit klebenden Kopfzeilen. Farbe ist Information. Ein einziges Blau trägt Hauptaktion, Auswahl und Fokus; Grün, Gold und Rot erscheinen nur als Zustand und immer zusammen mit einem Lucide-Zeichen und einem Wort. Die einzige grosse Geste ist das Urteilsband nach dem Lauf: über die ganze Breite, farbig hinterlegt, mit Belegen und der nächsten Handlung.

Zwei vollwertige Sätze: **Nacht** (Standard, `:root`) für das dunkle Studio und **Tag** (`:root[data-thema="tag"]`) für Sonne draussen. Beide sind normativ; die Frontmatter führt Nacht, die Tag-Werte stehen unten und im Sidecar. Bewusst verworfen (Richtungswahl 07.10.2026): die frühere eigenwillige Konsole-Optik und Karten-Kacheln.

**Key Characteristics:**
- Neutrales Graphit (Hue 255, Chroma ≤ 0,01) in drei Flächenstufen plus Feld und Hover.
- Ein Blau für Hauptaktion, Auswahl und Fokus; nichts sonst ist blau.
- Zustände nie nur über Farbe: Zeichen + Wort, in Tabellen, im Kopf und im Urteil.
- Geist für die Oberfläche, Geist Mono mit Tabellenziffern für Pfade, Prüfsummen, Zahlen.
- Dichte Tabellen mit Haarlinien, Blöcke mit 40-px-Kopfleiste, flach bis auf einen 1-px-Knopfschatten.
- Tag und Nacht als eigene Sätze mit Kontrastmindestwerten aus PRODUCT.md.

## Colors

Graphit ohne Farbstich, ein kühles Arbeitsblau und drei Signalfarben, jede mit einem eigenen dunklen (Nacht) bzw. hellen (Tag) Grund.

### Primary
- **Arbeitsblau** (`akzent`): Füllung der Hauptaktion („Einlesen“, „Karte auswerfen“), aktive Seitenreiter- und Reiter-Unterstreichung (2 px), Fortschrittsbalken, aktiver Schritt, eingeschalteter Schalter, Rand der Angebotsleiste. `akzent-hover` ist die Hover-Stufe.
- **Blauer Text** (`akzent-text`): Fokusring (2 px), Verweise, Zeichen gewählter Elemente (aktiver Seitenreiter, gewählte Quelle, Karte), Laufzeichen im Status.
- **Auswahl** (`auswahl`): gewählte Tabellenzeile, gewählte Quelle in der Seitenleiste, gedrückter Segmentknopf, Textauswahl.
- **Leise Auswahl** (`auswahl-leise`): Grund der Angebotsleiste, des Aktualisierungsbanners, der laufenden Anzeige im Kopf und des aktuellen Takes im Inspektor.

### Secondary
- **Sicher-Grün** (`ok`, Grund `ok-grund`): bewiesene Kopien, „Sicher zum Formatieren“, erledigte Schritte.
- **Achtung-Gold** (`warn`, Grund `warn-grund`): Hinweise, die nicht sperren; Pflicht-Update-Banner; Rückfrage beim Senken der Schwelle.
- **Fehler-Rot** (`fehler`, Grund `fehler-grund`): Fehler und „Nicht freigegeben · Karte nicht formatieren“.
- **Gefahr** (`gefahr`): nur Füllung zerstörerischer oder riskanter Bestätigungen (Abbruch bestätigen, Schwelle senken). Kein Text in dieser Farbe.

### Neutral
- **Fenster** (`fenster`): App-Grund hinter allem, Startleiste.
- **Panel** (`panel`): Blöcke, Seitenleiste, Inspektor, Tabellenkopf.
- **Panel 2** (`panel-2`): Kopfleiste, Blockköpfe, Werkzeugleiste, Reiterleiste, normaler Knopf.
- **Feld** (`feld`): Eingaben, Selects, Schalter-Grund, Balkenbett, Codeblöcke. Nachts dunkler als das Panel (eingelassen), tags reines Weiss.
- **Hover** (`hover`): Hover-Grund für Knöpfe, Seitenreiter, Quellen; Tabellenzeilen mit 60 % davon.
- **Linie / Linie stark** (`linie`, `linie-stark`): Haarlinien zwischen Zeilen und Flächen / Ränder von Bedienelementen und Unterkante des Tabellenkopfs.
- **Text, Text 2, Text 3** (`text`, `text-2`, `text-3`): Inhalt / Bedienbeschriftung im Ruhezustand / Nebeninfo, Spaltenköpfe, Hilfetexte.

### Tag-Satz
Tag ersetzt jeden Wert, nicht nur Hell und Dunkel: Fenster `oklch(0.880 0.004 255)`, Panel `oklch(0.975 0.002 255)`, Panel 2 `oklch(0.935 0.004 255)`, Feld `oklch(1 0 0)`, Hover `oklch(0.915 0.006 255)`, Linie `oklch(0.820 0.006 255)`, Linie stark `oklch(0.600 0.010 255)`; Text `oklch(0.190 0.010 255)` / `oklch(0.320 0.010 255)` / `oklch(0.420 0.010 255)`; Akzent `oklch(0.500 0.160 255)`, Akzent-Hover und Akzent-Text `oklch(0.450 0.160 255)`; Auswahl `oklch(0.870 0.055 252)`, leise `oklch(0.925 0.030 252)`; OK `oklch(0.470 0.130 150)`, Warn `oklch(0.490 0.115 70)`, Fehler `oklch(0.480 0.180 25)`, Gefahr `oklch(0.500 0.190 25)`; Gründe `oklch(0.920 0.055 150)`, `oklch(0.935 0.065 88)`, `oklch(0.925 0.045 25)`. Tags wird der Akzent dunkler beim Hover (nachts heller), und die Grundschrift steht auf Gewicht 450.

### Named Rules
**The Ein-Blau Rule.** Blau bedeutet „hier handeln“ oder „das ist gewählt“. Keine blauen Dekorflächen, keine blauen Überschriften, keine zweite Akzentfarbe.

**The Zeichen-und-Wort Rule.** Grün, Gold und Rot erscheinen nie allein: jeder Zustand trägt ein Lucide-Zeichen (CircleCheck, CircleAlert, CircleX, CircleDashed, LoaderCircle) und ein Wort. Die Signalfarbe färbt das Zeichen, der Text bleibt in Textfarbe.

**The Kontrast-Boden Rule.** Text nachts ≥ 6,8:1, tags ≥ 5,4:1; Signalfarben ≥ 5:1 auf ihrem Grund; Ränder von Bedienelementen tags ≥ 3:1 auf dem Panel. Ein neuer Wert, der darunter liegt, gehört nicht ins System.

## Typography

**Oberflächenschrift:** Geist Variable (mit -apple-system, Segoe UI, system-ui)
**Mono:** Geist Mono Variable (mit ui-monospace, SF Mono, Consolas)

**Character:** Geist ist sachlich und schmal genug für dichte Tabellen; Geist Mono mit Tabellenziffern lässt Pfade, Seriennummern, Prüfsummen und Grössen spaltengenau stehen. Beide über @fontsource eingebettet, nie nur Systemschrift.

### Hierarchy
- **Urteil** (700, 26 px, 1,15, −0,015 em, `text-wrap: balance`): nur der Titel im Urteilsband („Sicher zum Formatieren“).
- **Headline** (650, 20 px, −0,01 em): Kopf der Laufansicht.
- **Title** (650, 18 px, −0,01 em): Kartenname im Block Karte. Leerzustands-Titel 16 px/650.
- **Kennwert** (500–600, 15 px): Werte unter dem Fortschrittsbalken und Kennzahlen der Projektseite.
- **Body** (400, 13 px, 1,45; tags 450): alles Übrige, Tabellen, Blocktitel (650). Längere Texte auf 60–80 ch begrenzt.
- **Gross-Body** (14 px): Marke (650), grosser Knopf, Urteils-Unterzeilen.
- **Label** (500, 12 px): Spaltenköpfe, Feldbeschriftungen, Hilfetexte, Seitenleisten-Gruppen (600), Unterzeilen. In Satzschreibung, nie Versalien.
- **Klein** (11 px): Versionsnummer, Kennzahlen-Bezeichnung, Schrittnummer.
- **Mono** (0,94 em der Umgebung, Tabellenziffern): Pfade, Seriennummern, Zahlen, Kennwertzeile im Urteil.

### Named Rules
**The Tag-ist-kräftiger Rule.** Im Tag-Satz steht die Grundschrift auf 450 und ohne Antialiasing-Glättung, weil dünne Schrift in der Sonne zuerst verschwindet.

**The Zahlen-in-Mono Rule.** Jede Zahl, die man vergleicht oder abliest (Bytes, Prozent, Dauer, Seriennummer, Prüfsumme, Pfad), steht in Geist Mono mit Tabellenziffern. Dezimalkomma, Schweizer Format.

## Layout

Fenster-App mit festem Gerüst über `100dvh`: Kopfleiste 44 px (Marke, vier Seitenreiter mit Zeichen, Verbindungsstand und Darstellungsknopf rechts), optional ein Aktualisierungsbanner, darunter der Inhalt, der intern scrollt; das Fenster selbst scrollt nie.

- **Einlesen:** Seitenleiste 264 px (Karten, weitere Laufwerke, „Ordner wählen …“ unten) + Arbeitsbereich mit 16/20 px Rand und 12 px Abstand zwischen Blöcken. Unten eine klebende Startleiste auf dem Fenstergrund mit Befunden links und dem grossen Hauptknopf rechts.
- **Projekt:** Werkzeugleiste (min. 52 px) mit Projektwahl und Kennzahlen rechts, darunter Liste + Inspektor 300 px (unter 1240 px Breite 260 px). Reiter und Tabellenkopf kleben beim Scrollen.
- **Prüfen:** eine Spalte Blöcke, 16 px Abstand.
- **Einrichtung:** zentrierte Spalte, max. 880 px; jedes Feld zweispaltig (Bezeichnung und Hilfe links ≥ 220 px, Bedienelement rechts ≥ 300 px), getrennt durch Haarlinien.

Rhythmus in kleinen Schritten: 2, 4, 6, 8, 12, 16, 20 px; Zellen und Blockinhalt 12 px seitlich, Zeilen 6 px vertikal.

## Elevation & Depth

Flach. Tiefe entsteht durch Tonstufen (Fenster → Panel → Panel 2, Feld eingelassen) und Haarlinien. Einziger Schatten ist ein 1-px-Kontaktschatten unter normalen Knöpfen, damit sie als drückbar lesen; deaktivierte Knöpfe verlieren ihn.

### Shadow Vocabulary
- **Knopf-Kontakt** (`box-shadow: 0 1px 2px oklch(0 0 0 / 0.35)`, tags `/ 0.12`): nur `.knopf`.

### Named Rules
**The Linie-statt-Schatten Rule.** Blöcke, Tabellen, Inspektor und Leisten werden durch 1-px-Linien getrennt, nie durch Schatten oder schwebende Karten.

## Shapes

Kleine, gleichmässige Rundungen wie in Profi-Werkzeugen: 4 px für Bedienelemente (Knöpfe, Felder, Segmente, Quellen, Codeblöcke), 6 px für Blöcke, Angebotsleiste und den grossen Knopf, 8 px nur für das Urteilsband. Kreise nur für Schalter-Knopf und Schrittnummern; Pillen nur für den Schalter (10 px) und den Fortschrittsbalken (5 px). Reiter unter einer Leiste verbinden sich mit ihrem Inhalt (oben 6 px rund, unten 6 px rund), Unterstreichungen 2 px hoch mit 2 px Rundung.

## Components

### Buttons
Nüchtern und klar abgestuft.
- **Shape:** 4 px Rundung, 28 px hoch, 12 px seitlich, Gewicht 500, Zeichen 16 px mit 6 px Abstand.
- **Normal:** Panel-2-Grund, Rand `linie-stark`, Kontaktschatten. Hover `hover`, gedrückt `linie`, deaktiviert 45 % Deckkraft ohne Schatten.
- **Haupt:** Akzent gefüllt, Text `auf-akzent`, Gewicht 600. Eine Hauptaktion pro Bereich.
- **Gross:** 38 px, 20 px seitlich, 14 px, 6 px Rundung: nur „Einlesen“ in der Startleiste.
- **Gefahr:** `gefahr` gefüllt, weisser Text, Hover per Helligkeit 1,08: nur zweiter Schritt einer riskanten Bestätigung.
- **Klein:** 24 px, 8 px seitlich, 12 px (z. B. „Öffnen“ in Tabellen).
- **Symbolknopf:** 28 × 28 px, ohne Rand, `text-2`, Hover `hover`.

### Segment und Schalter
- **Segment:** Feldgrund, Rand `linie-stark`, 4 px, Knöpfe 26 px hoch durch Haarlinien getrennt; gedrückt = `auswahl` + 600. Für Filter und Darstellungswahl.
- **Schalter:** 34 × 20 px, Kreis 14 px; an = Akzent gefüllt, Kreis `auf-akzent`, 150 ms Gleiten.

### Blocks / Containers
- **Corner Style:** 6 px.
- **Background:** Panel; Kopfleiste Panel 2 mit Haarlinie unten, min. 40 px, Titel 13 px/650 links, eine Aktion rechts.
- **Shadow Strategy:** keiner (siehe Elevation).
- **Border:** 1 px `linie`.
- **Internal Padding:** 12 px.

### Tabellen
Dichte Liste wie in Silverstack. Kopf klebend, 12 px `text-3`/500, Unterkante `linie-stark`; Zellen 6 × 12 px mit Haarlinie, Zahlen rechtsbündig in Mono, Pfadspalte von vorne gekürzt (das Ende bleibt sichtbar). Hover 60 % `hover`; wählbare Tabellen markieren die Zeile mit `auswahl` und Fokus innen. Unterzeilen 12 px `text-3`.

### Inputs / Fields
- **Style:** 28 px, Feldgrund, 1 px `linie-stark`, 4 px, 8 px seitlich; Platzhalter `text-3`. Beschriftung immer sichtbar darüber (12 px `text-3`) oder links im Feld.
- **Focus:** 2 px Ring `akzent-text` innen (−1 px Versatz), Rand transparent.
- **Disabled:** 50 % Deckkraft.

### Navigation
- **Seitenreiter im Kopf:** Zeichen 16 px (Strich 1,75) + Wort, `text-2`/500; Hover `hover`-Grund; aktiv `text`/600, Zeichen `akzent-text`, 2-px-Akzent-Unterstreichung auf der Kopfunterkante.
- **Reiter in Blöcken:** gleiche Logik auf Panel-2-Leiste (38 px), Zähler in Klammern („Abgleich (1 offen)“).
- **Seitenleisten-Quellen:** Zeichen 18 px + Name 600 + Info 12 px `text-3`; gewählt = `auswahl`-Grund, Zeichen `akzent-text`.

### Status
Zeichen 14 px (Strich 2) + Wort in Textfarbe; nur das Zeichen trägt die Signalfarbe. „Läuft“ dreht das LoaderCircle in `akzent-text`. Im Kopf als 28-px-Knopf, der zur Einrichtung führt.

### Urteilsband (Signatur)
Nach dem Lauf über die ganze Breite: dreispaltig (Zeichen 44 px, Text, Knöpfe), 20 × 22 px Innenabstand, 8 px Rundung, Grund und Rand in der Zustandsfarbe (`ok-grund`/`ok`, `warn-grund`/`warn`, `fehler-grund`/`fehler`). Titel 26 px/700 in Textfarbe, darunter Karte und Kopienzahl (14 px), Kennwertzeile in Mono `text-3`, Belege pro Platte als Status-Zeilen, rechts „Karte auswerfen“ (Haupt) und „Bericht öffnen“. Bei Nein lautet der Titel immer „… · Karte nicht formatieren“. Darunter verbinden sich die Reiter Ziele · Abgleich · Dateien · Metadaten mit ihrem Inhalt.

### Fortschritt
Balken 10 px, Feldgrund, 5 px Rundung, Füllung Akzent per `scaleX` (300 ms linear). Darunter Werte in 15 px/500 mit 12-px-Bezeichnung; Schrittfolge mit 18-px-Nummernkreisen, aktiver Schritt gefüllt in Akzent, erledigte mit grünem Häkchen.

### Angebotsleiste
Erkannte Karte wird angeboten, nie selbst gewählt: 6 px, Rand Akzent, Grund `auswahl-leise`, Zeichen `akzent-text`, rechts Haupt- und Normalknopf.

## Do's and Don'ts

### Do:
- **Do** Flächen nur aus den Tokens Fenster, Panel, Panel 2, Feld nehmen und mit 1-px-Linien trennen.
- **Do** jeden Zustand mit Lucide-Zeichen und Wort zeigen; Signalfarbe nur auf Zeichen, Rand oder Grund.
- **Do** Zahlen, Pfade, Seriennummern und Prüfsummen in Geist Mono mit Tabellenziffern setzen; Pfade vorne kürzen.
- **Do** jeden neuen Wert in beiden Sätzen (Nacht und Tag) definieren und gegen die Kontrastböden prüfen.
- **Do** Lucide-Zeichen mit Strich 1,75 (Navigation, grosse Zeichen) bzw. 2 (Status) verwenden.
- **Do** eine Hauptaktion pro Bereich in Akzent; alles andere als normaler Knopf.

### Don't:
- **Don't** Konsole-Optik oder Karten-Kacheln zurückbringen (Richtungswahl 07.10.2026).
- **Don't** Blau für etwas anderes als Hauptaktion, Auswahl, Fokus und Laufanzeige verwenden.
- **Don't** Zustand nur über Farbe ausdrücken, und keine farbigen Fliesstexte in Grün oder Gold.
- **Don't** Schatten ausser dem Knopf-Kontaktschatten; keine schwebenden Karten, keine Verläufe.
- **Don't** Versal-Überschriften oder Kicker über Blocktiteln; Gruppenbezeichnungen stehen in Satzschreibung.
- **Don't** Unicode-Zeichen als Symbole (✓, ✗, →); Symbole kommen aus Lucide.
