# Stage Ingest

Kamerakarten Byte für Byte mit Beweis kopieren: Die Karte wird einmal gelesen und gleichzeitig an alle Ziele geschrieben. Dabei wird XXH3-128 gerechnet, optional MD5. Danach wird jedes Ziel vollständig ohne Cache zurückgelesen. Auf jedes Ziel kommen ASC MHL und ein Bericht. Die Karte ist erst freigegeben, wenn genug unabhängige Kopien geprüft sind.

Eine App für macOS und Windows (Tauri 2, Kern in Rust, Oberfläche in React), dazu später ein HDRI-Dienst. Gebaut für das Filmstudio Basel.

| Pfad | Inhalt |
| --- | --- |
| `kern/` | Rust-Bibliothek ohne Oberfläche: Kopieren, Prüfsummen, Zurücklesen, Geräteerkennung, Freigabe, Vorab-Prüfung, ASC MHL |
| `bericht/` | PDF-Bericht (Typst-Vorlage `vorlage.typ`, Geist eingebettet) |
| `src-tauri/` | App-Hülle |
| `src/` | Oberfläche (Design aus der Stage Companion App) |
| `docs/KONZEPT.md` | Konzept und Entscheidungen |

```bash
cargo test -p ingest-kern   # Kern
npm install && npm run tauri dev
```

Lizenz: MIT.
