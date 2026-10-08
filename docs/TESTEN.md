# Testbau auf dem Mac

Für schnelles Ausprobieren ohne Release (Marlon, 08.10.2026): `scripts/testen.sh` holt den neuesten Stand von `main`,
baut die App auf dem Mac und **ersetzt damit die installierte Stage Ingest**. Es gibt nie zwei Versionen; Einstellungen,
Ziele und Anmeldung bleiben. Oben neben der Versionsnummer steht „Test <Commit>“. Das nächste offizielle Update
(GitHub-Release, Mac und Windows) bringt die App wieder auf den offiziellen Stand.

Läuft die App, bricht das Skript ab: vorher sicherstellen, dass nichts kopiert wird, und die App beenden.

## Einmal einrichten

1. Werkzeuge von Apple: `xcode-select --install`
2. Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`, danach das Terminal neu öffnen
3. Node 22: Installer von nodejs.org (LTS) oder `brew install node@22`
4. Das Repo holen: `git clone https://github.com/casebasel/stage-ingest.git ~/stage-ingest`

## Testen

```bash
~/stage-ingest/scripts/testen.sh
```

Der erste Bau dauert einige Minuten, danach meist 1–3 Minuten. Der Testbau ist ein echter, schneller Bau wie das
Release, nur ohne Update-Signatur.

## Release

Wie bisher über GitHub (`docs/RELEASING.md`). Der Zwischenspeicher wird auf `main` vorgewärmt (`ci.yml`, gleicher
Schlüssel wie `release.yml`), damit ein Release nicht mehr bei null anfängt (erwartet etwa die halbe Zeit, noch nicht gemessen).
