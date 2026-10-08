# Releases und Updates

Wie Syncomat: GitHub Actions baut die App für Mac (Apple Silicon) und Windows, signiert die Updates und legt sie ins öffentliche Release. Die App holt `https://github.com/casebasel/stage-ingest/releases/latest/download/latest.json` und zeigt oben einen Banner.

## Schlüssel (einmalig)

Der öffentliche Schlüssel steht in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`). Der private Schlüssel liegt **nie** im Repo.

1. Den privaten Schlüssel als Repository-Secret `TAURI_SIGNING_PRIVATE_KEY` anlegen (Repo → Settings → Secrets and variables → Actions). Er hat kein Passwort; das leere Passwort setzt `release.yml` selbst (ein Secret mit einem Leerzeichen bricht das Signieren).
2. Den privaten Schlüssel sicher aufbewahren (Passwortmanager, verschlüsselt auf dem NAS). Geht er verloren, können bestehende Installationen keine Updates mehr bekommen und müssen einmal von Hand neu installiert werden.

## Plattformen

Vorerst nur **Mac** (Apple Silicon) im Release (09.10.2026, kein Windows im Einsatz). Unter Windows prüft `ci.yml` bei jedem
Push weiterhin den Kern (Tests) und die App (übersetzen), damit nichts unbemerkt bricht. Windows wieder ins Release:
in `release.yml` die drei auskommentierten Zeilen der Matrix einschalten; bestehende Windows-Installationen bekommen
ab dann wieder Updates.

## Release auslösen

Version in allen drei Dateien gleich setzen: `package.json`, `Cargo.toml` (`[workspace.package]`), `src-tauri/tauri.conf.json`.

```bash
git commit -am "v0.1.1"
git tag v0.1.1
git push && git push --tags
```

## Pflicht-Update

Nur wenn sich **Bericht, Ordnerstruktur oder eine Schnittstelle** ändern: in `MINDESTVERSION` die neue Version eintragen. Der Release-Workflow schreibt sie als `mindestVersion` in `latest.json`. Ältere Apps zeigen dann „Pflicht-Update“ und lassen keine neue Karte mehr beginnen. Ein laufender Kopiervorgang wird nie unterbrochen; installiert wird erst danach.

## Vorbelegung der Anmeldung (Plate Assistant / gemeinsame Supabase)

Damit man in der App nur E-Mail und Passwort eingibt, setzt der Release-Build Adresse und Anon-Key ein. Sie stehen **nicht** im Quelltext, sondern als Repository-Variablen (Repo → Settings → Secrets and variables → Actions → Variables):

- `SUPABASE_ADRESSE`, z. B. `https://<supabase>`
- `SUPABASE_ANON_KEY`, der öffentliche Anon-Key (wie in der iOS-App)

Fehlen sie, fragt die App danach.
