"""Befehle des HDRI-Dienstes.

    python -m hdri_dienst verarbeiten <aufnahmeordner> [--aus <datei.exr>] [--hoehe 2048] [--halb]

Liest `metadata.json` und die DNGs, führt die Belichtungsreihen zusammen, legt sie nach der Lage zum Panorama und
schreibt das gemessene EXR (RGB + `clip` + `abdeckung`) und eine Vorschau (JPEG, nur zum Ansehen).
"""

from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

import numpy as np

from . import exr
from .aufnahme import AufnahmeFehler, laden, panorama_hoehe, positionen_zusammenfuehren
from .projektion import panorama


def vorschau(bild: np.ndarray, pfad: Path) -> None:
    """Einfaches Tonemapping (Reinhard auf die mittlere Helligkeit) nach sRGB, nur zum Ansehen."""
    import cv2

    lum = 0.2126 * bild[..., 0] + 0.7152 * bild[..., 1] + 0.0722 * bild[..., 2]
    mittel = np.exp(np.mean(np.log(np.maximum(lum[lum > 0], 1e-6)))) if (lum > 0).any() else 1.0
    t = bild * (0.18 / mittel)
    t = t / (1.0 + t)
    srgb = np.where(t <= 0.0031308, 12.92 * t, 1.055 * np.power(np.maximum(t, 0), 1 / 2.4) - 0.055)
    cv2.imwrite(str(pfad), (np.clip(srgb, 0, 1)[..., ::-1] * 255).astype(np.uint8), [cv2.IMWRITE_JPEG_QUALITY, 90])


def verarbeiten(ordner: Path, aus: Path | None, hoehe: int | None, halb: bool) -> Path:
    start = time.time()
    a = laden(ordner)
    print(f"{a.hdri.get('id')}: {len(a.frames)} Bilder, {len(a.positionen())} Positionen", flush=True)
    positionen = positionen_zusammenfuehren(a, halb=halb, melden=lambda t, f: print(f"  {t} ({f:.0%})", flush=True))
    h = hoehe or panorama_hoehe(positionen)
    print(f"  Panorama {2 * h}×{h}", flush=True)
    bild, clip, abdeckung = panorama(positionen, h)
    ziel = aus or (Path(ordner) / f"{a.hdri.get('id', 'hdri')}_gemessen.exr")
    exr.schreiben(
        ziel,
        bild,
        {"clip": clip, "abdeckung": np.clip(abdeckung, 0, 1)},
        {
            "stage_ingest_hdri": str(a.hdri.get("id")),
            "stage_ingest_stufe": "gemessen (Merge + Lage, ohne Verfeinerung, ohne KI)",
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


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(prog="hdri_dienst")
    sub = p.add_subparsers(dest="befehl", required=True)
    v = sub.add_parser("verarbeiten", help="eine Aufnahme (Ordner mit metadata.json und DNGs) zum EXR rechnen")
    v.add_argument("ordner", type=Path)
    v.add_argument("--aus", type=Path)
    v.add_argument("--hoehe", type=int, help="Höhe des Panoramas in Pixeln (Breite = 2 × Höhe)")
    v.add_argument("--halb", action="store_true", help="DNGs in halber Auflösung entwickeln (schneller)")
    z = sub.add_parser("zugang", help="Zugang zur Supabase prüfen (nur lesen)")
    z.add_argument("--datei", type=Path, default=Path(r"D:\hdri-dienst\zugang.env"))
    args = p.parse_args(argv)
    if args.befehl == "zugang":
        return zugang_pruefen(args.datei)
    try:
        verarbeiten(args.ordner, args.aus, args.hoehe, args.halb)
    except AufnahmeFehler as e:
        print(f"Nicht verarbeitet: {e}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
