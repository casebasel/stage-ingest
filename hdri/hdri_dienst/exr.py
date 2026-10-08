"""EXR schreiben: linear, 32-Bit-Float, RGB plus Maskenkanäle.

Kanäle (Briefing, Kapitel 6): `R`, `G`, `B` = gemessene Strahldichte; `clip.Y` = 1, wo die Messung gesättigt war
(Wert ist nur eine Untergrenze); `abdeckung.Y` = 0, wo keine Aufnahme hinsieht (Loch, meist Nadir).
Die rein gemessene Version wird immer abgelegt; KI-Stufen schreiben später eine eigene Datei.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import OpenEXR


def schreiben(pfad: Path, bild: np.ndarray, masken: dict[str, np.ndarray] | None = None, angaben: dict | None = None) -> None:
    """`bild` H×W×3 float32 (linear), `masken` Name → H×W float32. `angaben` landen als Kopfzeilen (Text)."""
    pfad = Path(pfad)
    pfad.parent.mkdir(parents=True, exist_ok=True)
    kanaele = {
        "R": np.ascontiguousarray(bild[..., 0], dtype=np.float32),
        "G": np.ascontiguousarray(bild[..., 1], dtype=np.float32),
        "B": np.ascontiguousarray(bild[..., 2], dtype=np.float32),
    }
    for name, m in (masken or {}).items():
        kanaele[f"{name}.Y"] = np.ascontiguousarray(m, dtype=np.float32)
    kopf = {"compression": OpenEXR.ZIP_COMPRESSION, "type": OpenEXR.scanlineimage}
    for k, v in (angaben or {}).items():
        kopf[k] = str(v)
    # Erst unter anderem Namen schreiben, dann umbenennen: nie ein halbes EXR am Zielort.
    teil = pfad.with_suffix(pfad.suffix + ".teil")
    with OpenEXR.File(kopf, kanaele) as datei:
        datei.write(str(teil))
    teil.replace(pfad)


def lesen(pfad: Path) -> tuple[np.ndarray, dict[str, np.ndarray]]:
    """Gibt (RGB H×W×3, weitere Kanäle) zurück."""
    with OpenEXR.File(str(pfad), separate_channels=True) as datei:
        k = {name: np.asarray(c.pixels) for name, c in datei.channels().items()}
    rgb = np.stack([k.pop("R"), k.pop("G"), k.pop("B")], axis=-1)
    return rgb, k
