"""Befehle des HDRI-Dienstes.

    python -m hdri_dienst verarbeiten <aufnahmeordner> [--aus <datei.exr>] [--hoehe 2048] [--halb]

Liest `metadata.json` und die DNGs, führt die Belichtungsreihen zusammen, legt sie nach der Lage zum Panorama und
schreibt das gemessene EXR (RGB + `clip` + `abdeckung`) und eine Vorschau (JPEG, nur zum Ansehen).
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

import numpy as np

from . import exr
from .aufnahme import AufnahmeFehler, laden, panorama_hoehe, positionen_zusammenfuehren
from .projektion import panorama_multiband


def vorschau(bild: np.ndarray, pfad: Path) -> None:
    """Einfaches Tonemapping (Reinhard auf die mittlere Helligkeit) nach sRGB, nur zum Ansehen."""
    import cv2

    lum = 0.2126 * bild[..., 0] + 0.7152 * bild[..., 1] + 0.0722 * bild[..., 2]
    mittel = np.exp(np.mean(np.log(np.maximum(lum[lum > 0], 1e-6)))) if (lum > 0).any() else 1.0
    t = bild * (0.18 / mittel)
    t = t / (1.0 + t)
    srgb = np.where(t <= 0.0031308, 12.92 * t, 1.055 * np.power(np.maximum(t, 0), 1 / 2.4) - 0.055)
    cv2.imwrite(str(pfad), (np.clip(srgb, 0, 1)[..., ::-1] * 255).astype(np.uint8), [cv2.IMWRITE_JPEG_QUALITY, 90])


def verarbeiten(ordner: Path, aus: Path | None, hoehe: int | None, halb: bool, verfeinern: bool = True) -> Path:
    start = time.time()
    a = laden(ordner)
    print(f"{a.hdri.get('id')}: {len(a.frames)} Bilder, {len(a.positionen())} Positionen", flush=True)
    from .waechter import pruefen

    def melden(t: str, f: float) -> None:
        print(f"  {t} ({f:.0%})", flush=True)
        pruefen()  # startet nDisplay, sofort aufhören (Ada gehört dann der Stage)

    positionen = positionen_zusammenfuehren(a, halb=halb, melden=melden)
    bericht = {}
    if verfeinern and len(positionen) > 1:
        from .verfeinern import verfeinern as lage_verfeinern

        positionen, bericht = lage_verfeinern(positionen, melden=lambda t, f: print(f"  {t}", flush=True))
    h = hoehe or panorama_hoehe(positionen)
    print(f"  Panorama {2 * h}×{h}", flush=True)
    # Multiband-Nähte: Details scharf aus genau einem Bild, grobe Übergänge weich (keine Geister bei Parallaxe).
    bild, clip, abdeckung = panorama_multiband(positionen, h)
    ziel = aus or (Path(ordner) / f"{a.hdri.get('id', 'hdri')}_gemessen.exr")
    exr.schreiben(
        ziel,
        bild,
        {"clip": clip, "abdeckung": np.clip(abdeckung, 0, 1)},
        {
            "stage_ingest_hdri": str(a.hdri.get("id")),
            "stage_ingest_stufe": "gemessen (Merge, Objektivkorrektur aus dem DNG, Lage verfeinert, Multiband-Nähte, ohne KI)"
            if bericht.get("paare")
            else "gemessen (Merge, Objektivkorrektur aus dem DNG, Lage nur aus der IMU, ohne KI)",
            "stage_ingest_verfeinerung": json.dumps(bericht),
            "stage_ingest_farbraum": "linear, Primärfarben Rec.709/sRGB",
        },
    )
    vorschau(bild, ziel.with_suffix(".jpg"))
    loch = float((abdeckung <= 0).mean())
    print(f"  geschrieben {ziel} · Clip {float((clip > 0.5).mean()):.2%} · Löcher {loch:.2%} · {time.time() - start:.0f} s")
    return ziel


def zugang_pruefen(datei: Path) -> int:
    """Nur lesen: Anmeldung, Kennzeichen des Kontos, offene Aufnahmen, Zustand von nDisplay. Zeigt keine Geheimnisse."""
    from .server import Server, ServerFehler
    from .waechter import stage_aktiv

    try:
        s = Server.aus_datei(datei)
        s.token()
        print(f"Anmeldung ok: {s.email} (Dienst-Konto, app = hdri)")
        alle = s.lesen("hdri?geloescht=eq.false&select=id,zustand,format")
        print(f"Aufnahmen sichtbar: {len(alle)}")
        for a in alle:
            n = len(s.lesen(f"hdri_frame?hdri_id=eq.{a['id']}&geloescht=eq.false&select=id"))
            print(f"  {a['id']} · {a['zustand']} · {a.get('format')} · {n} Bilder")
        print(f"Bereit zum Rechnen (uploaded, ohne fertigen Job): {len(s.offene_aufnahmen())}")
        try:
            print(f"Jobs lesbar: {len(s.lesen('hdri_job?select=id'))}")
        except ServerFehler as e:
            print(f"Jobs nicht lesbar (Migration 0019 fehlt?): {e}")
    except ServerFehler as e:
        print(f"Zugang nicht ok: {e}", file=sys.stderr)
        return 2
    p = stage_aktiv()
    print(f"nDisplay/Unreal: {'läuft (' + p + ')' if p else 'läuft nicht'}")
    return 0


def rechnen(hdri_id: str, datei: Path, arbeit: Path, hoehe: int | None, halb: bool) -> int:
    """Eine Aufnahme aus der Supabase holen und rechnen (noch ohne Job-Eintrag; der kommt mit Migration 0019)."""
    from .server import Server, ServerFehler
    from .waechter import StageAktiv, pruefen

    try:
        pruefen()
        s = Server.aus_datei(datei)
        h = s.lesen(f"hdri?id=eq.{hdri_id}&select=id,zustand,format")
        if not h:
            print(f"Aufnahme {hdri_id} nicht gefunden", file=sys.stderr)
            return 2
        if h[0]["zustand"] != "uploaded":
            print(f"Hinweis: Zustand {h[0]['zustand']}, noch nicht alles hochgeladen; gerechnet wird, was da ist.")
        ordner = s.aufnahme_holen(hdri_id, arbeit, melden=lambda t, f: print(f"  geladen {t}", flush=True))
        pruefen()
        verarbeiten(ordner, None, hoehe, halb)
    except (ServerFehler, AufnahmeFehler, StageAktiv) as e:
        print(f"Nicht gerechnet: {e}", file=sys.stderr)
        return 2
    return 0


def klein_jpg(pfad: Path, breite: int = 2048) -> bytes:
    """Vorschau-JPEG des Panoramas auf `breite` verkleinert (2:1), zum Hochladen."""
    import cv2

    bild = cv2.imread(str(pfad), cv2.IMREAD_COLOR)
    if bild is None:
        raise FileNotFoundError(pfad)
    if bild.shape[1] > breite:
        bild = cv2.resize(bild, (breite, breite * bild.shape[0] // bild.shape[1]), interpolation=cv2.INTER_AREA)
    ok, daten = cv2.imencode(".jpg", bild, [cv2.IMWRITE_JPEG_QUALITY, 85])
    if not ok:
        raise ValueError("JPEG nicht erzeugt")
    return daten.tobytes()


def ergebnis_angaben(hid: str, b: dict) -> dict:
    """`hdri_job.ergebnis`: Pfade auf Ada (relativ zu ergebnisse/) und, falls hochgeladen, das Bild im Speicher."""
    e = {"exr_gemessen": f"{hid}/{b['exr_gemessen']}", "vorschau": f"{hid}/{b['vorschau']}"}
    if b.get("vorschau_speicher"):
        e["vorschau_speicher"] = b["vorschau_speicher"]
    return e


def laufen(datei: Path, wurzel: Path, takt_s: int, halb: bool) -> int:
    """Dauerbetrieb: neue, vollständig hochgeladene Aufnahmen holen und rechnen; pausiert, solange nDisplay läuft.

    Ergebnisse nach `<wurzel>/ergebnisse/<hdri_id>/` (EXR, Vorschau, bericht.json). Den Job in `hdri_job` setzt
    der Dienst, sobald die Tabelle da ist (Migration 0019); vorher merkt er sich Erledigtes nur über die Dateien.
    """
    import shutil
    from datetime import datetime

    from .server import Server, ServerFehler
    from .waechter import StageAktiv, pruefen, stage_aktiv

    def log(text: str) -> None:
        print(f"{datetime.now():%Y-%m-%d %H:%M:%S} {text}", flush=True)

    server = Server.aus_datei(datei)
    jobs_da = True

    def job(hdri_id: str, felder: dict, anlegen: bool = False) -> None:
        nonlocal jobs_da
        if not jobs_da:
            return
        try:
            server.job_setzen(hdri_id, felder, anlegen=anlegen)
        except ServerFehler as e:
            if "hdri_job" in str(e) or "unbekannt" in str(e).lower() or "PGRST" in str(e):
                jobs_da = False
                log("Tabelle hdri_job fehlt noch (Migration 0019): Stand nur in den Ergebnisordnern")
            else:
                log(f"Job nicht gesetzt: {e}")

    versucht: dict[str, float] = {}

    def hochladen(hid: str, ziel: Path) -> dict | None:
        """Lädt das Panorama hoch (höchstens alle 30 min je Aufnahme versucht); schreibt den Pfad in bericht.json."""
        if time.time() - versucht.get(hid, 0) < 1800:
            return None
        erster = hid not in versucht
        versucht[hid] = time.time()
        b = json.loads((ziel / "bericht.json").read_text(encoding="utf-8"))
        try:
            b["vorschau_speicher"] = server.ergebnis_hochladen(hid, klein_jpg(ziel / b["vorschau"]))
        except (ServerFehler, OSError, ValueError) as e:
            if erster:
                log(f"{hid}: Vorschau nicht hochgeladen ({e}); neuer Versuch alle 30 min")
            return None
        (ziel / "bericht.json").write_text(json.dumps(b, indent=2), encoding="utf-8")
        log(f"{hid}: Vorschau hochgeladen ({b['vorschau_speicher']})")
        return b

    def ausstehende_hochladen() -> None:
        for bericht in sorted((wurzel / "ergebnisse").glob("*/bericht.json")):
            try:
                b = json.loads(bericht.read_text(encoding="utf-8"))
            except (OSError, ValueError):
                continue
            if b.get("exr_gemessen") and not b.get("vorschau_speicher"):
                hid = bericht.parent.name
                neu = hochladen(hid, bericht.parent)
                if neu:
                    job(hid, {"ergebnis": ergebnis_angaben(hid, neu)})

    log(f"HDRI-Dienst läuft (Takt {takt_s} s, Ergebnisse in {wurzel / 'ergebnisse'})")
    while True:
        try:
            p = stage_aktiv()
            if p:
                log(f"pausiert: {p} läuft")
                time.sleep(takt_s)
                continue
            for a in server.offene_aufnahmen():
                hid = a["id"]
                ziel = wurzel / "ergebnisse" / hid
                if (ziel / "bericht.json").exists():
                    # Schon gerechnet (z. B. bevor es hdri_job gab): den Job nachtragen, damit die App es sieht.
                    b = json.loads((ziel / "bericht.json").read_text(encoding="utf-8"))
                    if b.get("exr_gemessen") and jobs_da:
                        job(hid, {"zustand": "processed", "stufe": "fertig", "fortschritt": 1.0, "rechner": "Ada",
                                  "fertig_am": b.get("fertig"), "ergebnis": ergebnis_angaben(hid, b)},
                            anlegen=True)
                        if jobs_da:
                            log(f"{hid}: Job nachgetragen (processed)")
                    continue
                if a.get("format") != "dng":
                    log(f"{hid}: Format {a.get('format')}, nicht messbar, übersprungen")
                    ziel.mkdir(parents=True, exist_ok=True)
                    (ziel / "bericht.json").write_text(json.dumps({"verworfen": "kein DNG"}), encoding="utf-8")
                    job(hid, {"zustand": "verworfen", "fehler": "Nur DNG (Bayer-RAW) ist linear und messbar"}, anlegen=True)
                    continue
                log(f"{hid}: beginnt")
                beginn = datetime.now().astimezone().isoformat()
                job(hid, {"zustand": "laeuft", "stufe": "laden", "rechner": "Ada", "begonnen_am": beginn, "fehler": None}, anlegen=True)
                try:
                    pruefen()
                    ordner = server.aufnahme_holen(hid, wurzel / "arbeit")
                    pruefen()
                    job(hid, {"stufe": "rechnen"})
                    exr = verarbeiten(ordner, ziel / f"{hid}_gemessen.exr", None, halb)
                    bericht = {"exr_gemessen": exr.name, "vorschau": exr.with_suffix(".jpg").name, "fertig": datetime.now().astimezone().isoformat()}
                    (ziel / "bericht.json").write_text(json.dumps(bericht, indent=2), encoding="utf-8")
                    shutil.rmtree(ordner, ignore_errors=True)  # Rohdaten liegen weiter im Bucket
                    bericht = hochladen(hid, ziel) or bericht
                    job(hid, {"zustand": "processed", "stufe": "fertig", "fortschritt": 1.0, "fertig_am": bericht["fertig"],
                              "ergebnis": ergebnis_angaben(hid, bericht)})
                    log(f"{hid}: fertig")
                except StageAktiv as e:
                    log(f"{hid}: {e}")
                    job(hid, {"zustand": "pausiert", "stufe": "wartet auf Ende nDisplay"})
                    break
                except (ServerFehler, AufnahmeFehler) as e:
                    log(f"{hid}: nicht gerechnet: {e}")
                    job(hid, {"zustand": "fehler", "fehler": str(e)[:2000]})
                    ziel.mkdir(parents=True, exist_ok=True)
                    (ziel / "fehler.txt").write_text(str(e), encoding="utf-8")
                    if isinstance(e, AufnahmeFehler):
                        (ziel / "bericht.json").write_text(json.dumps({"fehler": str(e)}), encoding="utf-8")
            ausstehende_hochladen()
        except ServerFehler as e:
            log(f"Server: {e}")
        except Exception as e:  # nie still sterben: melden und im nächsten Takt weiter
            log(f"Unerwarteter Fehler: {type(e).__name__}: {e}")
        time.sleep(takt_s)


def main(argv: list[str] | None = None) -> int:
    # Konsole unter Windows (SSH, Dienst) zeigt sonst Umlaute falsch.
    for strom in (sys.stdout, sys.stderr):
        if hasattr(strom, "reconfigure"):
            strom.reconfigure(encoding="utf-8", errors="replace")
    p = argparse.ArgumentParser(prog="hdri_dienst")
    sub = p.add_subparsers(dest="befehl", required=True)
    v = sub.add_parser("verarbeiten", help="eine Aufnahme (Ordner mit metadata.json und DNGs) zum EXR rechnen")
    v.add_argument("ordner", type=Path)
    v.add_argument("--aus", type=Path)
    v.add_argument("--hoehe", type=int, help="Höhe des Panoramas in Pixeln (Breite = 2 × Höhe)")
    v.add_argument("--halb", action="store_true", help="DNGs in halber Auflösung entwickeln (schneller)")
    v.add_argument("--ohne-verfeinerung", action="store_true", help="nur die IMU-Lage verwenden")
    z = sub.add_parser("zugang", help="Zugang zur Supabase prüfen (nur lesen)")
    z.add_argument("--datei", type=Path, default=Path(r"D:\hdri-dienst\zugang.env"))
    r = sub.add_parser("rechnen", help="Aufnahme aus der Supabase holen und rechnen")
    r.add_argument("hdri_id")
    r.add_argument("--datei", type=Path, default=Path(r"D:\hdri-dienst\zugang.env"))
    r.add_argument("--arbeit", type=Path, default=Path(r"D:\hdri-dienst\arbeit"))
    r.add_argument("--hoehe", type=int)
    r.add_argument("--halb", action="store_true")
    lf = sub.add_parser("laufen", help="Dauerbetrieb: neue Aufnahmen automatisch rechnen")
    lf.add_argument("--datei", type=Path, default=Path(r"D:\hdri-dienst\zugang.env"))
    lf.add_argument("--wurzel", type=Path, default=Path(r"D:\hdri-dienst"))
    lf.add_argument("--takt", type=int, default=60)
    lf.add_argument("--halb", action="store_true")
    args = p.parse_args(argv)
    if args.befehl == "laufen":
        return laufen(args.datei, args.wurzel, args.takt, args.halb)
    if args.befehl == "rechnen":
        return rechnen(args.hdri_id, args.datei, args.arbeit, args.hoehe, args.halb)
    if args.befehl == "zugang":
        return zugang_pruefen(args.datei)
    try:
        verarbeiten(args.ordner, args.aus, args.hoehe, args.halb, not args.ohne_verfeinerung)
    except AufnahmeFehler as e:
        print(f"Nicht verarbeitet: {e}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
