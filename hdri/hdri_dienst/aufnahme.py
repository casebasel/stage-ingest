"""Eine HDRI-Aufnahme des Plate Assistant lesen: `<hdri_id>/metadata.json` (format_version 1) und die Bilder.

Format (Plate Assistant `docs/HDRI.md`, Auskunft 08.10.2026):
- `hdri`: u. a. `format` (`dng` = Bayer-RAW linear | `heic` = verarbeitet, nicht linear), `hfov_grad` (kurze Seite,
  Hochformat), `vfov_grad` (lange Seite), `kompass_grad`, `bezugssystem`, `ev_stufen`.
- `frames`: je Bild `position`, `ev`, `lage_quaternion` [w, x, y, z] (Kamera → Welt; Kamera x = rechte Bildkante,
  y = Oberkante im Hochformat, Blick −z), `belichtung_s` und `iso` (tatsächlich, aus dem EXIF), `pfad`
  (`<hdri_id>/<frame_id>.dng`), `breite`/`hoehe` (Sensor-Ausrichtung).
- Gruppieren nach `position`, innerhalb nach `ev`; gelöschte Bilder fehlen schon.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

import numpy as np

from .merge import Belichtung, zusammenfuehren
from .projektion import Kamera, Position, quaternion_zu_matrix


class AufnahmeFehler(Exception):
    pass


@dataclass
class Aufnahme:
    ordner: Path
    meta: dict

    @property
    def hdri(self) -> dict:
        return self.meta["hdri"]

    @property
    def frames(self) -> list[dict]:
        return self.meta["frames"]

    def positionen(self) -> dict[int, list[dict]]:
        gruppen: dict[int, list[dict]] = {}
        for f in self.frames:
            gruppen.setdefault(int(f["position"]), []).append(f)
        for liste in gruppen.values():
            liste.sort(key=lambda f: f.get("ev") or 0)
        return dict(sorted(gruppen.items()))


def laden(ordner: Path) -> Aufnahme:
    ordner = Path(ordner)
    pfad = ordner / "metadata.json"
    try:
        meta = json.loads(pfad.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise AufnahmeFehler(f"metadata.json fehlt in {ordner}") from None
    if meta.get("format_version") != 1:
        raise AufnahmeFehler(f"Unbekannte format_version {meta.get('format_version')!r} (erwartet 1)")
    if not meta.get("frames"):
        raise AufnahmeFehler("Die Aufnahme enthält keine Bilder")
    if meta["hdri"].get("format") != "dng":
        # HEIC ist verarbeitet und nicht linear: ein Merge daraus wäre keine Messung.
        raise AufnahmeFehler(f"Format {meta['hdri'].get('format')!r}: nur DNG (Bayer-RAW) ist linear und messbar")
    return Aufnahme(ordner, meta)


def datei(a: Aufnahme, frame: dict) -> Path:
    """Bilddatei zu einem Frame: `pfad` ist `<hdri_id>/<frame_id>.dng`; gesucht wird im Aufnahmeordner."""
    name = Path(frame["pfad"]).name
    p = a.ordner / name
    if not p.exists():
        raise AufnahmeFehler(f"Bild fehlt: {name}")
    return p


def bild_laden(pfad: Path, halb: bool = False) -> np.ndarray:
    """Lineares RGB (float32, 0..1, 1 = Weisspunkt) in Anzeige-Ausrichtung (Orientation-Tag angewendet).

    DNG über LibRaw: keine Gamma-Kurve, keine automatische Aufhellung, Weissabgleich der Aufnahme (fest je Session).
    `.npy` (für Tests): wird unverändert geladen.
    """
    pfad = Path(pfad)
    if pfad.suffix.lower() == ".npy":
        return np.load(pfad).astype(np.float32)
    import rawpy

    with rawpy.imread(str(pfad)) as raw:
        rgb = raw.postprocess(
            gamma=(1, 1),
            no_auto_bright=True,
            output_bps=16,
            use_camera_wb=True,
            half_size=halb,
            output_color=rawpy.ColorSpace.sRGB,  # Primärfarben sRGB/Rec.709, linear
        )
    return rgb.astype(np.float32) / 65535.0


def positionen_zusammenfuehren(a: Aufnahme, halb: bool = False, melden=None) -> list[Position]:
    """Je Position die Belichtungsreihe zusammenführen und mit Lage und Kamera versehen."""
    h = a.hdri
    gruppen = a.positionen()
    aus: list[Position] = []
    for n, (pos, frames) in enumerate(gruppen.items()):
        reihe = []
        for f in frames:
            bild = bild_laden(datei(a, f), halb=halb)
            zeit, iso = f.get("belichtung_s"), f.get("iso")
            if not zeit or not iso:
                raise AufnahmeFehler(f"Position {pos}: Belichtungszeit oder ISO fehlt")
            reihe.append(Belichtung(bild, float(zeit), float(iso)))
        hdr, clip = zusammenfuehren(reihe)
        hoehe, breite = hdr.shape[:2]
        # Sichtfeld: hfov = kurze Seite, vfov = lange Seite (Hochformat). Liegt das Bild quer, tauschen.
        kurz, lang = float(h["hfov_grad"]), float(h["vfov_grad"])
        hfov, vfov = (kurz, lang) if breite <= hoehe else (lang, kurz)
        q = frames[0]["lage_quaternion"]
        aus.append(Position(hdr, quaternion_zu_matrix(tuple(q)), Kamera(breite, hoehe, hfov, vfov), clip))
        if melden:
            melden(f"Position {pos} zusammengeführt", (n + 1) / len(gruppen))
    return aus


def panorama_hoehe(positionen: list[Position], hoechstens: int = 4096) -> int:
    """Höhe des Panoramas aus der Auflösung der Bilder (Pixel pro Grad), gerundet auf 256, höchstens `hoechstens`."""
    k = positionen[0].kamera
    pixel_pro_rad = k.fy
    hoehe = int(np.pi * pixel_pro_rad)
    return max(256, min(hoechstens, (hoehe // 256) * 256))
