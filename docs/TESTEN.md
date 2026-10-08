# Testbau auf dem Mac

Für schnelles Ausprobieren ohne Release (Marlon, 08.10.2026): `scripts/testen.sh` holt den neuesten Stand von `main`,
baut die App auf dem Mac und **ersetzt damit die installierte Stage Ingest**. Es gibt nie zwei Versionen; Einstellungen,
Ziele und Anmeldung bleiben. Oben neben der Versionsnummer steht „Test <Commit>“. Das nächste offizielle Update
(GitHub-Release, Mac und Windows) bringt die App wieder auf den offiziellen Stand: Das Update-Banner erscheint
nur, wenn GitHub eine neuere Version hat als der Testbau, und ersetzt ihn dann.

Läuft die App, bricht das Skript ab: vorher sicherstellen, dass nichts kopiert wird, und die App beenden.

## Einmal einrichten

1. Werkzeuge von Apple: `xcode-select --install`
2. Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`, danach das Terminal neu öffnen
3. Node 22: Installer von nodejs.org (LTS) oder `brew install node@22`
4. Das Repo holen: `git clone https://github.com/casebasel/stage-ingest.git ~/stage-ingest`

5. Vorbelegung der Anmeldung (sonst fragt der Testbau nach Adresse und Schlüssel): die Werte der GitHub-Variablen
   `SUPABASE_ADRESSE` und `SUPABASE_ANON_KEY` (Repo → Settings → Secrets and variables → Actions → Variables) in
   `~/.config/stage-ingest/vorbelegung.env` eintragen, nur auf dem Mac:

   ```
   VITE_SUPABASE_ADRESSE=<Wert von SUPABASE_ADRESSE>
   VITE_SUPABASE_ANON_KEY=<Wert von SUPABASE_ANON_KEY>
   ```

## Testen

```bash
~/stage-ingest/scripts/testen.sh
```

Der erste Bau dauert einige Minuten, danach meist 1–3 Minuten. Der Testbau ist ein echter, schneller Bau wie das
Release, nur ohne Update-Signatur.

## Release

Wie bisher über GitHub (`docs/RELEASING.md`). Der Zwischenspeicher wird auf `main` vorgewärmt (`ci.yml`, gleicher
Schlüssel wie `release.yml`), damit ein Release nicht mehr bei null anfängt (erwartet etwa die halbe Zeit, noch nicht gemessen).
