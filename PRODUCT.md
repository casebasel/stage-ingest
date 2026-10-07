# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

(Desktop-App für macOS und Windows: Tauri 2 mit Oberfläche in der WebView, Kern in Rust. Keine native Plattform-Sprache.)

## Users

**Wechselnde Leute im Team des Filmstudios Basel** bedienen Stage Ingest am Drehtag: wer gerade Zeit hat, nicht zwingend eine Fachperson für Datensicherung. Die App muss ohne Einweisung sicher bedienbar sein, Fehler abfangen und unmissverständlich sagen, ob eine Karte formatiert werden darf. (Marlon, 07.10.2026)

Weitere Beteiligte, die Ergebnisse lesen: VFX und Schnitt (Ordnerstruktur, ASC MHL, ALE, Bewegungsdaten), Stage Companion und Plate Assistant (Meldungen).

## Product Purpose

Kamerakarten (ARRI ALEXA Mini, Amira) Byte für Byte mit Beweis kopieren und alles eines Drehs in eine VFX-taugliche Ordnerstruktur zusammenführen. Erfolg heisst: Keine Karte wird formatiert, bevor genug unabhängige, geprüfte Kopien existieren, und vor dem Formatieren ist sichtbar, ob alles Gedrehte da ist.

## Positioning

Stage Ingest ist kein allgemeines Offload-Werkzeug, sondern der Ingest der drei Studio-Apps (Stage Companion, Plate Assistant, Stage Ingest). Es kennt, was gedreht wurde (Soll-Liste aus Stage und Plate Assistant), ordnet Clips den Takes zu (Clipname, Timecode, Zeitfenster der Klappe), legt Plates mit Referenzfotos ab und meldet Karten an die Stage zurück. Zusammen mit der vollen Prüfung (einmal lesen, an alle Ziele schreiben, jedes Ziel ohne Cache zurücklesen, ASC MHL) beantwortet es „sicher zum Formatieren?“ und „ist von diesem Projekt alles da?“.

## Operating Context

Drei Umgebungen, alle häufig (Marlon, 07.10.2026):

- **Laptop draussen am Set** beim Plate-Dreh: Tageslicht, wenig Platz, Zeitdruck zwischen zwei Setups.
- **Im dunklen Studio** neben der VP-Stage: abgedunkelt, die Karte kommt aus der Kamera zurück, während weitergedreht wird.
- **Im Büro oder am Schnittplatz**: ruhig, nach dem Dreh; Karten nachbearbeiten, Projektübersicht, Nachprüfen.

Ziele sind externe Platten (USB/Thunderbolt) und das NAS (SMB). Ein Kopiervorgang dauert Minuten bis lange Minuten und darf nie unterbrochen werden. Online-Zugang ist nicht garantiert (draussen); die Kopie selbst braucht kein Netz.

## Capabilities and Constraints

- Volle Prüfung immer, kein schneller Modus: XXH3-128 (optional MD5), Zurücklesen ohne Cache, ASC MHL v2, PDF-Bericht, Zusammenfassung pro Karte.
- „Sicher zum Formatieren“ nur bei N unabhängigen Kopien auf bewiesen verschiedenen Platten (Standard 2), nur für die ganze Karte, nie bei leerer Karte. Entwerten der Karte bleibt aus bis zum Test an der Amira.
- Ordnerstruktur `<KURZNAME>/<Datum>_<Dreh>/01_KAMERA … 05_METADATEN`; Projekt mit festem Kurznamen aus der gemeinsamen Supabase.
- Anmeldung mit dem persönlichen Konto wie in der iPhone-App; Löschen von HDRI-Rohdaten nur mit Kennzeichen am Konto.
- Stage Ingest schreibt nie Daten, die anderen Apps gehören (Systemkarte `casebasel/stage-system`).
- Öffentliches Repo: keine Schlüssel, Adressen oder internen Pfade im Quelltext.
- Begriffe aus der Systemkarte (`BEGRIFFE.md`): Dreh, Plate, Take, Clip, Karte, Reel, Freigabe, Entwerten, Bericht.

## Brand Commitments

Name „Stage Ingest“, Teil der Studio-Apps des Filmstudios Basel. Sprache der Oberfläche: Deutsch (Schweiz, „ss“ statt „ß“). Die heutige Nähe zur Stage Companion App ist **keine feste Vorgabe** (Marlon, 07.10.2026).

**Gestaltung: der Kategorie-Standard, mit voller Sorgfalt** (Marlon, 07.10.2026, Richtungswahl). Stage Ingest soll aussehen und sich bedienen wie die etablierten Profi-Werkzeuge der Kategorie, ohne eigene Eigenheiten. Massstab für Handwerk und Dichte sind **Silverstack / ShotPut Pro** (DIT-Offload) und **DaVinci Resolve** (Postproduktion). Vertrautheit ist hier gewollt: Wer diese Programme kennt, findet sich sofort zurecht.

## Evidence on Hand

- Konzept und Entscheidungen: `docs/KONZEPT.md`, `docs/ROADMAP.md`, Systemkarte `casebasel/stage-system`.
- Echte Daten zum Testen: Stage-Export der Studio-Takes, ALEXA-Mini-Testclips (ProRes 422 HQ), Plate-Assistant-Daten in der gemeinsamen Supabase.
- Es gibt keine Kundenzitate, Referenzen oder Messwerte zur Geschwindigkeit; keine erfinden.

## Product Principles

1. **Lieber nein als ein falsches Ja.** Was nicht bewiesen ist, wird nicht freigegeben; die App sagt klar, warum.
2. **Am Set ohne Einweisung.** Wer die App zum ersten Mal sieht, kommt sicher von „Karte rein“ zu „sicher zum Formatieren“.
3. **Überall lesbar.** Tageslicht draussen und dunkles Studio sind gleich wichtig.
4. **Nie unterbrechen, nie still scheitern.** Laufende Kopien sind heilig; jeder Fehler wird gezeigt und belegt.
5. **Ein System, drei Apps.** Daten, Begriffe und Zuständigkeiten folgen der Systemkarte.

## Accessibility & Inclusion

Lesbarkeit bei Sonne und im Dunkeln ist Pflicht: Mindestkontraste wie im Plate Assistant (Text tags ≥ 5,4:1, nachts ≥ 6,8:1, Linien tags ≥ 3:1), kein Zustand nur über Farbe (immer Wort oder Zeichen dazu).
