# CLAUDE.md – Stage Ingest (Filmstudio Basel)

Sprache: Deutsch. Stil: direkt, klare Empfehlungen, keine Floskeln.

**Stage Ingest** kopiert Kamerakarten Byte für Byte mit Beweis (XXH3-128, Zurücklesen ohne Cache, ASC MHL, PDF-Bericht, Freigabe) und führt alles eines Plate-Drehs in eine VFX-taugliche Ordnerstruktur zusammen. Eine App für Mac und Windows (Tauri 2, Rust-Kern, React), dazu später ein HDRI-Dienst im selben Repo. Konzept und Entscheidungen: `docs/KONZEPT.md`.

| Pfad | Inhalt |
| --- | --- |
| `kern/` | Rust-Bibliothek ohne Oberfläche: Kopieren, Prüfsummen, Zurücklesen, Geräteerkennung, Freigabe, Vorab-Prüfung, ASC MHL (Tests: `cargo test -p ingest-kern`; MHL gegen die Referenz mit `ASCMHL_DEBUG=<pfad zu ascmhl-debug>`) |
| `bericht/` | PDF-Bericht (Typst-Vorlage `vorlage.typ`, Geist eingebettet) |
| `src-tauri/` | App-Hülle, Befehle und Fortschritts-Ereignisse |
| `src/` | Oberfläche; Design aus der Stage Companion App (deren `DESIGN.md`, Konsole). **Nacht** = Stage-Werte unverändert, **Tag** = eigener Satz für Tageslicht in `src/stil/stil.css` (Text ≥ 7:1, Signalfarben ≥ 5:1 auf dem Grund); Schalter oben rechts, Standard folgt dem System |
| `docs/` | Konzept, HDRI-Briefing, Lesesicht Plate Assistant, Quellen aus der Stage, Recherche Kopieren/MHL |

## 🔴 Öffentliches Repo

Keine Schlüssel, Token, Passwörter, Studio-IPs, Hostnamen oder internen Pfade in Code, Doku oder Commits. Alles Ortsabhängige kommt aus einer lokalen Einstellung pro Rechner (nicht eingecheckt). Vor jedem Push danach suchen.

## Systemkarte: drei Apps, eine Wahrheit

Stage Ingest gehört zu drei Apps: Stage Companion, Plate Assistant (iOS) und diese. Ihre gemeinsame Wahrheit ist die **Systemkarte** im Repo `casebasel/stage-system` (Klon neben diesem Repo). Der Start-Hook (`.claude/settings.json`) holt sie bei jedem Sessionstart und blendet sie ein.

- **Vor allem, was eine andere App betrifft oder ihr gehört:** zuerst in der Karte nachsehen (`BESITZ.md`, `SCHNITTSTELLEN.md`). Fehlt es dort, zuerst der zuständigen Session schreiben (SendMessage: `vp-companion-app` für die Stage, `plate-assistant IOS app` für den Plate Assistant), dann bauen.
- **Nachricht = Abstimmung, Karte = Ergebnis.** Eine app-übergreifende Entscheidung gilt erst, wenn sie in der Karte steht.
- Diese Session pflegt den Abschnitt „Stage Ingest“ in `SYSTEM.md` und darf Entscheidungen in `ENTSCHEIDUNGEN.md` anfügen; Besitz, Schnittstellen, Begriffe nur nach Absprache. Nach jeder Änderung den anderen Sessions eine Zeile schicken (was, warum, Commit).
- Nie ohne Marlon: Schlüssel, Server oder Datenbank einer anderen App ändern, in deren Repo schreiben.

## Grundsätze

- **Immer voll prüfen**, kein schneller Modus. Ein laufender Kopiervorgang wird nie unterbrochen, auch nicht durch ein Update.
- Ein bestehendes Ziel wird nie überschrieben. Die Freigabe zählt Platten, nicht Ziele.
- **Ada ist der nDisplay-Node der Stage:** der HDRI-Dienst rechnet nur, wenn nDisplay nicht läuft; nichts darf einen Take gefährden.
- **Entwerten** bleibt aus, bis es an einer Ersatzkarte an der echten Amira getestet ist.
- Auf Windows-Rechnern Dateien nur unter `C:\claude\tasks\JJJJ-MM-TT_kurzname` ablegen.
