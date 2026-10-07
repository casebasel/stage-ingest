# Roadmap Stage Ingest

Stand 07.10.2026. Nur diese App; app-übergreifende Abhängigkeiten stehen in der Systemkarte (`casebasel/stage-system`). Phasen wie in `docs/KONZEPT.md`, Kapitel 11.

## Jetzt: Phase 1 abschliessen (Ziel: Plate-Dreh mit der ALEXA Mini, Ende Oktober 2026)

Gebaut ist der Kern (siehe KONZEPT Kapitel 14). Offen, in dieser Reihenfolge:

1. ~~Releases 0.1.1~~ (Update-Banner getestet); 0.1.2 mit allen Funden der Code-Prüfung als Pflicht-Update.
2. **Sicherheit nachschärfen**
   - ~~Optional: Karte ein zweites Mal lesen~~ gebaut
   - ~~„Ziel nachprüfen“ gegen ASC MHL~~ gebaut
   - ~~Seriennummer der Platte auch am Mac~~ gebaut (system_profiler)
   - ~~Unabhängige Code-Prüfung des Kerns~~ gemacht, alle Funde behoben (5 kritische, 7 mittlere)
3. **Test mit echter Hardware** (Marlon)
   - Mac: echte Karte über den Kartenleser auf zwei externe Platten und aufs NAS
   - Windows: auf Ada mit `A001R132`
   - Bericht und Gerätekennung kontrollieren
4. Erst danach: Stage Ingest am Dreh einsetzen, die ersten Tage parallel zum bisherigen Werkzeug.

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
- ~~ALE pro Karte aus den Clips (Start-TC, fps)~~ gebaut, gegen den Stage-Parser geprüft; ~~an den Stage-Server schicken~~ gebaut (`ingest.karte`), gemeinsam getestet; wartet auf den Deploy der Stage
- ~~`02_PLATES/`: pro Plate `plate.json` (Lage, Kamera, Takes mit Verweis auf den Clip, Referenz-Takes) und Referenzfotos~~ gebaut; offen: Test mit echten Daten
- ~~Projektseite: Plan und Stand eines Projekts (Drehorte, Plates, Takes, Fotos, HDRI), eingelesene Karten von den Zielen, Takes ohne Clip = Karte fehlt~~ gebaut (Wunsch Marlon 07.10.2026); offen: Studio-Takes der Stage (braucht Projekt am Take in der Stage), Test mit echten Daten

## Phase 3: HDRI ohne KI

HDRI-Dienst auf Ada (rechnet nur, wenn nDisplay nicht läuft): Debevec-Merge, Stitching, EXR mit Maske; Prüfen/Freigeben und Ausrichten in der App; Rohdaten aus Supabase Storage, Löschen erst nach geprüftem Ingest.

## Phase 4: KI-Stufen

Nadir (ComfyUI), Lichter (DiffHDR); „gemessen“ und „ergänzt“ getrennt sichtbar.

## Phase 5: DeckLink-Livebild

Neigung/Rollen aus dem SDI-Signal.

## Entscheidungen, die Marlon noch treffen muss

- Farbwelt: Stage-Design behalten oder an den Plate Assistant angleichen
- Rolle `ingest_writer` in der Plate-Assistant-Supabase anlegen (Phase 2)
- Zugang von VM 170 zu Ada für Windows-Tests (oder Tests von Hand)
