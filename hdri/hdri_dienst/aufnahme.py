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


def korrigieren(bild: np.ndarray, roh: Rohbild) -> np.ndarray:
    """Objektivkorrektur (Randabdunklung, dann Entzerrung) und Drehung in die Anzeige-Ausrichtung."""
    from . import opcodes

    if roh.vignette is not None:
        bild = opcodes.vignette_anwenden(bild, roh.vignette)
    if roh.verzerrung is not None:
        bild = opcodes.verzerrung_anwenden(bild, roh.verzerrung)
    k = {0: 0, 3: 2, 5: 1, 6: -1}.get(roh.drehung, 0)
    return np.ascontiguousarray(np.rot90(bild, k=k)) if k else bild


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
    if not any((a.ordner / Path(f["pfad"]).name).exists() for f in a.frames):
        raise AufnahmeFehler("Keines der Bilder ist da")
    aus: list[Position] = []
    for n, (pos, frames) in enumerate(gruppen.items()):
        reihe = []
        vorhanden = [f for f in frames if (a.ordner / Path(f["pfad"]).name).exists()]
        if not vorhanden:
            if melden:
                melden(f"Position {pos}: keine Bilder da, ausgelassen", (n + 1) / len(gruppen))
            continue
        roh = None
        for f in vorhanden:
            roh = roh_laden(datei(a, f), halb=halb)
            bild = roh.bild
            zeit, iso = f.get("belichtung_s"), f.get("iso")
            if not zeit or not iso:
                raise AufnahmeFehler(f"Position {pos}: Belichtungszeit oder ISO fehlt")
            reihe.append(Belichtung(bild, float(zeit), float(iso)))
        hdr, clip = zusammenfuehren(reihe)
        # Objektivkorrektur erst nach dem Zusammenführen: Sättigung und Gewichte gelten für die Rohwerte des Sensors.
        hdr = korrigieren(hdr, roh)
        clip = korrigieren(clip.astype(np.float32), Rohbild(clip, roh.drehung, roh.verzerrung, None)) > 0.5
        hoehe, breite = hdr.shape[:2]
        # Sichtfeld: hfov = kurze Seite, vfov = lange Seite (Hochformat). Liegt das Bild quer, tauschen.
        kurz, lang = float(h["hfov_grad"]), float(h["vfov_grad"])
        hfov, vfov = (kurz, lang) if breite <= hoehe else (lang, kurz)
        q = vorhanden[0]["lage_quaternion"]
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
