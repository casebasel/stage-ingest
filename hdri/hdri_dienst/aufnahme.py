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


@dataclass
class Rohbild:
    """Entwickeltes Bild in Sensor-Ausrichtung, noch ohne Objektivkorrektur und ohne Drehung."""

    bild: np.ndarray
    drehung: int = 0  # LibRaw-flip: 0 keine, 3 = 180°, 5 = 90° gegen, 6 = 90° im Uhrzeigersinn
    verzerrung: object | None = None
    vignette: object | None = None


def roh_laden(pfad: Path, halb: bool = False) -> Rohbild:
    """DNG linear entwickeln (keine Gamma-Kurve, keine Aufhellung, Weissabgleich der Aufnahme), in Sensor-Ausrichtung,
    dazu die Korrekturen aus `OpcodeList3`. `.npy` (Tests): unverändert, ohne Korrekturen."""
    pfad = Path(pfad)
    if pfad.suffix.lower() == ".npy":
        return Rohbild(np.load(pfad).astype(np.float32))
    import rawpy
    import tifffile

    from . import opcodes

    with rawpy.imread(str(pfad)) as raw:
        drehung = raw.sizes.flip
        rgb = raw.postprocess(
            gamma=(1, 1),
            no_auto_bright=True,
            output_bps=16,
            use_camera_wb=True,
            half_size=halb,
            user_flip=0,
            output_color=rawpy.ColorSpace.sRGB,  # Primärfarben sRGB/Rec.709, linear
        )
    with tifffile.TiffFile(str(pfad)) as t:
        tag = t.pages[0].tags.get("OpcodeList3")
        daten = bytes(tag.value) if tag is not None else b""
    verz, vign = opcodes.lesen(daten)
    return Rohbild(rgb.astype(np.float32) / 65535.0, drehung, verz, vign)


def geometrie(bild: np.ndarray, roh: Rohbild) -> np.ndarray:
    """Nur Geometrie: Entzerrung und Drehung in die Anzeige-Ausrichtung (keine Helligkeit)."""
    from . import opcodes

    if roh.verzerrung is not None:
        bild = opcodes.verzerrung_anwenden(bild, roh.verzerrung)
    k = {0: 0, 3: 2, 5: 1, 6: -1}.get(roh.drehung, 0)
    return np.ascontiguousarray(np.rot90(bild, k=k)) if k else bild


def _k(kamera: Kamera) -> np.ndarray:
    """Kameramatrix für unsere Konvention (x rechts, y oben, Blick −z): [u·w, v·w, w] = K · v_kamera."""
    return np.array(
        [[kamera.fx, 0.0, -kamera.breite / 2], [0.0, -kamera.fy, -kamera.hoehe / 2], [0.0, 0.0, -1.0]]
    )


def ausrichten(bild: np.ndarray, kamera: Kamera, lage_bild: np.ndarray, lage_ziel: np.ndarray) -> np.ndarray:
    """Bild einer zweiten Auslösung auf die Lage der ersten drehen (reine Drehung: Homographie K·Rbᵀ·Rz·K⁻¹)."""
    import cv2

    if np.allclose(lage_bild, lage_ziel, atol=1e-6):
        return bild
    k = _k(kamera)
    h = k @ lage_bild.T @ lage_ziel @ np.linalg.inv(k)  # Zielpixel → Pixel im Bild
    return cv2.warpPerspective(
        bild, h, (bild.shape[1], bild.shape[0]), flags=cv2.INTER_LINEAR | cv2.WARP_INVERSE_MAP, borderMode=cv2.BORDER_CONSTANT
    )


def positionen_zusammenfuehren(a: Aufnahme, halb: bool = False, melden=None) -> list[Position]:
    """Je Position die Belichtungsreihe zusammenführen und mit Lage und Kamera versehen.

    Jedes Bild wird zuerst entzerrt und gedreht (Geometrie), dann auf die Lage des ersten Bildes der Position
    ausgerichtet: Ab Build 12 trägt jede Auslösung ihre eigene Lage (bis ~1,5° Unterschied innerhalb einer
    Position). Die Randabdunklung wird erst nach dem Zusammenführen korrigiert, damit Sättigung und Gewichte für
    die Rohwerte des Sensors gelten; ihre Verstärkung geht dafür durch dieselbe Geometrie.
    """
    from . import opcodes

    h = a.hdri
    gruppen = a.positionen()
    if not any((a.ordner / Path(f["pfad"]).name).exists() for f in a.frames):
        raise AufnahmeFehler("Keines der Bilder ist da")
    aus: list[Position] = []
    for n, (pos, frames) in enumerate(gruppen.items()):
        vorhanden = [f for f in frames if (a.ordner / Path(f["pfad"]).name).exists()]
        if not vorhanden:
            if melden:
                melden(f"Position {pos}: keine Bilder da, ausgelassen", (n + 1) / len(gruppen))
            continue
        reihe, kamera, lage_ziel, roh = [], None, None, None
        for f in vorhanden:
            zeit, iso = f.get("belichtung_s"), f.get("iso")
            if not zeit or not iso:
                raise AufnahmeFehler(f"Position {pos}: Belichtungszeit oder ISO fehlt")
            roh = roh_laden(datei(a, f), halb=halb)
            bild = geometrie(roh.bild, roh)
            lage = quaternion_zu_matrix(tuple(f["lage_quaternion"]))
            if kamera is None:
                hoehe, breite = bild.shape[:2]
                # Sichtfeld: hfov = kurze Seite, vfov = lange Seite (Hochformat). Liegt das Bild quer, tauschen.
                kurz, lang = float(h["hfov_grad"]), float(h["vfov_grad"])
                hfov, vfov = (kurz, lang) if breite <= hoehe else (lang, kurz)
                kamera, lage_ziel = Kamera(breite, hoehe, hfov, vfov), lage
            reihe.append(Belichtung(ausrichten(bild, kamera, lage, lage_ziel), float(zeit), float(iso)))
        hdr, clip = zusammenfuehren(reihe)
        if roh.vignette is not None:
            eins = np.ones(roh.bild.shape[:2] + (1,), np.float32)
            verstaerkung = geometrie(opcodes.vignette_anwenden(eins, roh.vignette)[..., 0], roh)
            hdr = (hdr * verstaerkung[..., None]).astype(np.float32)
        aus.append(Position(hdr, lage_ziel, kamera, clip))
        if melden:
            melden(f"Position {pos} zusammengeführt", (n + 1) / len(gruppen))
    return aus


def panorama_hoehe(positionen: list[Position], hoechstens: int = 4096) -> int:
    """Höhe des Panoramas aus der Auflösung der Bilder (Pixel pro Grad), gerundet auf 256, höchstens `hoechstens`."""
    k = positionen[0].kamera
    pixel_pro_rad = k.fy
    hoehe = int(np.pi * pixel_pro_rad)
    return max(256, min(hoechstens, (hoehe // 256) * 256))
