"""Zugang zur gemeinsamen Supabase für den HDRI-Dienst (Konto `hdri-dienst@…`, `app_metadata.app = "hdri"`).

Lesen: `hdri`, `hdri_frame`, `hdri_job`, Bucket `hdri`. Schreiben: nur `hdri_job` über `aenderungen_anwenden`
(Migration 0019); der Server lehnt alles andere für dieses Konto ab, und dieser Code schickt auch nichts anderes.
Zugang aus `D:\\hdri-dienst\\zugang.env` (nie im Repo): SUPABASE_ADRESSE, SUPABASE_ANON_KEY, HDRI_EMAIL,
HDRI_PASSWORT. Nur die Standardbibliothek, damit auf Ada nichts weiter zu installieren ist.
"""

from __future__ import annotations

import json
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

#: Einzige Tabelle, die der Dienst schreibt.
DARF_SCHREIBEN = {"hdri_job"}
#: Felder von `hdri_job`, die der Dienst setzt (0019).
JOB_FELDER = {
    "_anlegen", "zustand", "stufe", "fortschritt", "rechner", "begonnen_am", "fertig_am", "fehler", "ergebnis",
    "ausrichtung_grad", "nord_grad",
}


class ServerFehler(Exception):
    pass


def zugang_lesen(pfad: Path) -> dict[str, str]:
    werte = {}
    for zeile in Path(pfad).read_text(encoding="utf-8-sig").splitlines():
        zeile = zeile.strip()
        if not zeile or zeile.startswith("#") or "=" not in zeile:
            continue
        k, v = zeile.split("=", 1)
        werte[k.strip()] = v.strip().strip('"').strip("'")
    fehlt = [k for k in ("SUPABASE_ADRESSE", "SUPABASE_ANON_KEY", "HDRI_EMAIL", "HDRI_PASSWORT") if not werte.get(k)]
    if fehlt:
        raise ServerFehler(f"In {pfad} fehlt: {', '.join(fehlt)}")
    return werte


def jetzt() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


@dataclass
class Server:
    adresse: str
    anon_key: str
    email: str
    passwort: str
    _token: str | None = field(default=None, repr=False)
    _bis: float = 0.0

    @classmethod
    def aus_datei(cls, pfad: Path) -> "Server":
        z = zugang_lesen(pfad)
        return cls(z["SUPABASE_ADRESSE"].rstrip("/"), z["SUPABASE_ANON_KEY"], z["HDRI_EMAIL"], z["HDRI_PASSWORT"])

    # --- Netz -------------------------------------------------------------------------------------------------

    def _anfrage(self, methode: str, pfad: str, daten=None, token: bool = True, roh: bool = False, zeit: int = 60):
        # Eigene Kennung: Cloudflare weist die Standardkennung „Python-urllib“ ab (Fehler 1010).
        kopf = {"apikey": self.anon_key, "User-Agent": "StageIngest-HDRI-Dienst/1"}
        if token:
            kopf["Authorization"] = f"Bearer {self.token()}"
        body = None
        if daten is not None:
            body = json.dumps(daten).encode()
            kopf["Content-Type"] = "application/json"
        req = urllib.request.Request(self.adresse + pfad, data=body, headers=kopf, method=methode)
        try:
            with urllib.request.urlopen(req, timeout=zeit) as a:
                inhalt = a.read()
        except urllib.error.HTTPError as e:
            text = e.read().decode(errors="replace")[:300]
            if e.code == 401:
                raise ServerFehler("Anmeldung abgelehnt (Anon-Key oder Konto prüfen)") from None
            raise ServerFehler(f"{methode} {pfad.split('?')[0]}: HTTP {e.code} {text}") from None
        except urllib.error.URLError as e:
            raise ServerFehler(f"Server nicht erreichbar: {e.reason}") from None
        return inhalt if roh else (json.loads(inhalt) if inhalt else None)

    def token(self) -> str:
        if self._token and time.time() < self._bis - 60:
            return self._token
        a = self._anfrage(
            "POST", "/auth/v1/token?grant_type=password", {"email": self.email, "password": self.passwort}, token=False
        )
        meta = (a.get("user") or {}).get("app_metadata") or {}
        if meta.get("app") != "hdri":
            # Sicherheitsgurt: Nur das Technik-Konto des Dienstes darf hier laufen, nie ein persönliches Konto.
            raise ServerFehler("Das Konto ist nicht das Dienst-Konto (app_metadata.app = \"hdri\" fehlt)")
        self._token, self._bis = a["access_token"], time.time() + float(a.get("expires_in", 3600))
        return self._token

    # --- Lesen ------------------------------------------------------------------------------------------------

    def lesen(self, abfrage: str):
        return self._anfrage("GET", f"/rest/v1/{abfrage}")

    def offene_aufnahmen(self) -> list[dict]:
        """Aufnahmen mit vollständig hochgeladenen Rohdaten, deren Job noch nicht fertig, verworfen oder freigegeben ist."""
        aufnahmen = self.lesen("hdri?zustand=eq.uploaded&geloescht=eq.false&select=id,dreh_id,plate_id,format&order=erstellt_am.asc")
        try:
            jobs = {j["hdri_id"]: j for j in self.lesen("hdri_job?geloescht=eq.false&select=hdri_id,zustand")}
        except ServerFehler as e:
            if "PGRST205" not in str(e):
                raise
            jobs = {}  # Tabelle hdri_job gibt es erst ab Migration 0019
        erledigt = {"processed", "verworfen", "linked"}
        return [a for a in aufnahmen if jobs.get(a["id"], {}).get("zustand") not in erledigt]

    def datei(self, pfad: str) -> bytes:
        teil = urllib.parse.quote(pfad)
        return self._anfrage("GET", f"/storage/v1/object/authenticated/hdri/{teil}", roh=True, zeit=600)

    def aufnahme_holen(self, hdri_id: str, ordner: Path, melden=None) -> Path:
        """`metadata.json` und alle Bilder nach `ordner/<hdri_id>/` (vorhandene, vollständige Dateien bleiben)."""
        ziel = Path(ordner) / hdri_id
        ziel.mkdir(parents=True, exist_ok=True)
        meta_bytes = self.datei(f"{hdri_id}/metadata.json")
        (ziel / "metadata.json").write_bytes(meta_bytes)
        frames = json.loads(meta_bytes)["frames"]
        for n, f in enumerate(frames, 1):
            name = Path(f["pfad"]).name
            p = ziel / name
            if p.exists() and (not f.get("bytes") or p.stat().st_size == int(f["bytes"])):
                continue
            daten = self.datei(f["pfad"])
            if f.get("bytes") and len(daten) != int(f["bytes"]):
                raise ServerFehler(f"{name}: {len(daten)} statt {f['bytes']} Bytes geladen")
            teil = p.with_suffix(p.suffix + ".teil")
            teil.write_bytes(daten)
            teil.replace(p)
            if melden:
                melden(f"{n}/{len(frames)} {name}", n / len(frames))
        return ziel

    # --- Schreiben (nur hdri_job) -----------------------------------------------------------------------------

    def job_setzen(self, hdri_id: str, felder: dict, anlegen: bool = False, projekt_id: str | None = None) -> None:
        """Ändert den Job einer Aufnahme; `anlegen` schickt zuerst `_anlegen` (der Server führt Doppeltes zusammen)."""
        job = f"job-{hdri_id}"
        zeit = int(time.time() * 1_000_000)
        aenderungen = []
        if anlegen:
            wert = {"hdri_id": hdri_id, "zustand": "wartet", "geloescht": False, "erstellt_am": jetzt()}
            if projekt_id:
                wert["projekt_id"] = projekt_id
            aenderungen.append(self._aenderung(job, "_anlegen", wert, zeit))
        for feld, wert in felder.items():
            aenderungen.append(self._aenderung(job, feld, wert, zeit + 1))
        self._anwenden(aenderungen)

    @staticmethod
    def _aenderung(datensatz: str, feld: str, wert, zeit: int) -> dict:
        return {"id": uuid.uuid4().hex, "tabelle": "hdri_job", "datensatz": datensatz, "feld": feld, "wert": wert, "zeit": zeit}

    def _anwenden(self, aenderungen: list[dict]) -> None:
        for a in aenderungen:
            if a["tabelle"] not in DARF_SCHREIBEN or a["feld"] not in JOB_FELDER:
                raise ServerFehler(f"Der HDRI-Dienst schreibt {a['tabelle']}.{a['feld']} nicht")
        antwort = self._anfrage(
            "POST", "/rest/v1/rpc/aenderungen_anwenden", {"p_geraet": "HDRI-Dienst (Ada)", "p_aenderungen": aenderungen}
        )
        abgelehnt = [e for e in antwort or [] if e.get("ergebnis") not in ("uebernommen", "aelter", "doppelt")]
        if abgelehnt:
            raise ServerFehler("Job nicht gespeichert: " + "; ".join(str(e.get("grund") or e.get("ergebnis")) for e in abgelehnt))
