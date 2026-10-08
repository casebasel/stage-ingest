#!/usr/bin/env bash
# Testbau auf dem Mac: holt den neuesten Stand, baut Stage Ingest und ersetzt damit die installierte App.
# Es gibt nie zwei Versionen (Marlon, 08.10.2026): gleiche App, gleiche Einstellungen; oben steht „Test <Commit>“.
# Das nächste offizielle Update (GitHub-Release) bringt die App wieder auf den offiziellen Stand.
# Läuft die App, bricht das Skript ab: ein Kopiervorgang wird nie unterbrochen.
set -euo pipefail
cd "$(dirname "$0")/.."

APP="/Applications/Stage Ingest.app"
BAU="target/release/bundle/macos/Stage Ingest.app"

[ "$(uname)" = "Darwin" ] || { echo "Nur auf dem Mac."; exit 1; }
for werkzeug in git npm cargo; do
  command -v "$werkzeug" >/dev/null || { echo "$werkzeug fehlt. Einrichtung: docs/TESTEN.md"; exit 1; }
done
if pgrep -f "Stage Ingest.app/Contents/MacOS/" >/dev/null; then
  echo "Stage Ingest läuft noch. Bitte zuerst prüfen, dass nichts kopiert wird, die App beenden und nochmals starten."
  exit 1
fi

echo "› Neuester Stand"
git pull --ff-only
# Pakete nur neu holen, wenn sich die Liste geändert hat.
if [ ! -f node_modules/.stand ] || ! cmp -s package-lock.json node_modules/.stand; then
  echo "› Pakete"
  npm ci --no-audit --no-fund
  cp package-lock.json node_modules/.stand
fi

COMMIT="$(git rev-parse --short HEAD)"
echo "› Bauen (Test $COMMIT); beim ersten Mal einige Minuten, danach meist 1–3"
VITE_TESTBAU="$COMMIT" npx tauri build --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'

echo "› Ersetzen: $APP"
rm -rf "$APP"
ditto "$BAU" "$APP"
open "$APP"
echo "Fertig: Stage Ingest Test $COMMIT läuft."
