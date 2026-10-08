"""KI-Stufe „Lichter“: ausgebrannte Stellen der gemessenen EXR mit DiffHDR schätzen (docs/HDRI-KI-TEST.md).

Grundsatz: Die Messung bleibt. Nur wo `clip` = 1 ist (dort ist der Messwert eine Untergrenze), darf die Schätzung
den Wert erhöhen, nie senken. Jede so veränderte Stelle steht im Kanal `erfunden` der zweiten Datei `_ki.exr`.

DiffHDR (Apache 2.0) läuft in einer eigenen Python-Umgebung (Wan2.1-VACE-14B, ~9 min je Panorama auf der RTX 6000 Ada)
als eigener Prozess, damit der Dienst ihn sofort beenden kann, wenn nDisplay startet. Wo DiffHDR, seine Umgebung
und die Modelle liegen, steht nur auf dem Rechner in `ki.env` neben `zugang.env` (nie im Repo):

    DIFFHDR_ORDNER=<Ordner des DiffHDR-Codes>
    DIFFHDR_PYTHON=<python.exe der Umgebung>
    MODEL_BASE=<Ordner mit Wan-AI/Wan2.1-VACE-14B>
    DIFFHDR_LORA=<…/DiffHDR_Pano.safetensors>
    DIFFHDR_SCHRITTE=30

Fehlt `ki.env`, rechnet der Dienst wie bisher nur die Messung.
"""

from __future__ import annotations

import os
import subprocess
import time
from pathlib import Path

import cv2
import numpy as np

from . import exr

#: Unter diesem Anteil geclippter Pixel lohnt die Schätzung nicht (Rauschen, einzelne Glanzlichter).
MINDEST_CLIP = 1e-4
#: Auflösung, in der DiffHDR rechnet (Vorgabe des Modells). Licht ist niederfrequent; hochskaliert reicht das.
BREITE, HOEHE = 2048, 1024


class KiAbbruch(Exception):
    pass


def einstellungen(pfad: Path) -> dict | None:
    """Liest `ki.env` (Zeilen SCHLUESSEL=WERT). None, wenn die Datei fehlt oder unvollständig ist."""
    if not Path(pfad).exists():
        return None
    w = {}
    for zeile in Path(pfad).read_text(encoding="utf-8").splitlines():
        if "=" in zeile and not zeile.lstrip().startswith("#"):
            k, v = zeile.split("=", 1)
            w[k.strip()] = v.strip()
    noetig = ("DIFFHDR_ORDNER", "DIFFHDR_PYTHON", "MODEL_BASE", "DIFFHDR_LORA")
    return w if all(w.get(k) for k in noetig) else None


def lum(x: np.ndarray) -> np.ndarray:
    return 0.2126 * x[..., 0] + 0.7152 * x[..., 1] + 0.0722 * x[..., 2]


def _srgb(x: np.ndarray) -> np.ndarray:
    x = np.clip(x, 0, 1)
    return np.where(x <= 0.0031308, 12.92 * x, 1.055 * np.power(x, 1 / 2.4) - 0.055)


def eingabe(hdr: np.ndarray, clip: np.ndarray) -> tuple[np.ndarray, float, np.ndarray, np.ndarray]:
    """8-bit-Eingabe für DiffHDR: so belichtet, dass die geclippten Stellen gerade weiss sind und der Rest erhalten
    bleibt. Gibt (sRGB uint8 H×W×3 RGB, Belichtung k, verkleinertes HDR, verkleinerte Clip-Maske) zurück."""
    h = cv2.resize(hdr, (BREITE, HOEHE), interpolation=cv2.INTER_AREA)
    c = cv2.resize(clip.astype(np.float32), (BREITE, HOEHE), interpolation=cv2.INTER_AREA) > 0.5
    if c.any():
        k = 1.0 / max(float(np.percentile(lum(h)[c], 5)), 1e-6)
    else:
        k = 1.0 / max(float(np.percentile(lum(h), 99.5)), 1e-6)
    return (_srgb(h * k) * 255 + 0.5).astype(np.uint8), k, h, c


def zusammensetzen(hdr: np.ndarray, clip: np.ndarray, schaetzung_klein: np.ndarray, h_klein: np.ndarray,
                   c_klein: np.ndarray) -> tuple[np.ndarray, np.ndarray, dict]:
    """Schätzung auf die Messung skalieren (über nicht geclippte Mitteltöne), hochskalieren und nur in den geclippten
    Stellen übernehmen, wo sie heller ist als die Messung. Gibt (HDR, erfunden 0/1, Bericht) zurück."""
    mitte = (~c_klein) & (lum(h_klein) >= np.percentile(lum(h_klein), 20)) & (lum(h_klein) <= np.percentile(lum(h_klein), 95))
    s = float(np.median(lum(h_klein)[mitte] / np.maximum(lum(schaetzung_klein)[mitte], 1e-9))) if mitte.any() else 1.0
    gross = cv2.resize(schaetzung_klein * s, (hdr.shape[1], hdr.shape[0]), interpolation=cv2.INTER_LINEAR)
    c = clip > 0.5
    heller = c & (lum(gross) > lum(hdr))
    aus = hdr.copy()
    aus[heller] = gross[heller]
    licht_vorher = float(lum(hdr)[c].sum())
    bericht = {
        "clip_anteil": float(c.mean()),
        "massstab": s,
        "licht_im_clip_faktor": float(lum(aus)[c].sum() / licht_vorher) if licht_vorher > 0 else 1.0,
        "erfunden_anteil": float(heller.mean()),
    }
    return aus.astype(np.float32), heller.astype(np.float32), bericht


def diffhdr(png: Path, aus_ordner: Path, cfg: dict, pruefen, melden=None) -> np.ndarray:
    """Startet DiffHDR als eigenen Prozess; `pruefen()` wirft, sobald die Stage Ada braucht (dann Prozess beenden)."""
    aus_ordner.mkdir(parents=True, exist_ok=True)
    befehl = [
        cfg["DIFFHDR_PYTHON"], "infer_hdri.py",
        "--lora_path", cfg["DIFFHDR_LORA"],
        "--input_path", str(png),
        "--output_dir", str(aus_ordner),
        "--num_inference_steps", str(cfg.get("DIFFHDR_SCHRITTE", "30")),
    ]
    umgebung = {**os.environ, "MODEL_BASE": cfg["MODEL_BASE"], "PYTHONIOENCODING": "utf-8"}
    with open(aus_ordner / "diffhdr.log", "w", encoding="utf-8") as log:
        p = subprocess.Popen(befehl, cwd=cfg["DIFFHDR_ORDNER"], env=umgebung, stdout=log, stderr=subprocess.STDOUT)
        start = time.time()
        try:
            while p.poll() is None:
                time.sleep(5)
                pruefen()
                if melden:
                    melden(f"DiffHDR rechnet ({time.time() - start:.0f} s)")
        except BaseException:
            p.kill()
            p.wait()
            raise
    if p.returncode != 0:
        ende = (aus_ordner / "diffhdr.log").read_text(encoding="utf-8", errors="replace")[-600:]
        raise KiAbbruch(f"DiffHDR beendet mit {p.returncode}: {ende}")
    rgb, _ = exr.lesen(aus_ordner / "predicted.exr")
    return cv2.resize(rgb, (BREITE, HOEHE), interpolation=cv2.INTER_LINEAR)


def lichter(gemessen: Path, ziel: Path, cfg: dict, arbeit: Path, pruefen, melden=None) -> dict | None:
    """Schreibt `ziel` (`_ki.exr`), wenn genug ausgebrannt ist. None, wenn nichts zu tun war."""
    hdr, kanaele = exr.lesen(gemessen)
    clip = kanaele.get("clip.Y", np.zeros(hdr.shape[:2], np.float32))
    if float((clip > 0.5).mean()) < MINDEST_CLIP:
        return None
    start = time.time()
    rgb8, _, h_klein, c_klein = eingabe(hdr, clip)
    arbeit.mkdir(parents=True, exist_ok=True)
    png = arbeit / "eingabe.png"
    cv2.imwrite(str(png), rgb8[..., ::-1])
    schaetzung = diffhdr(png, arbeit / "diffhdr", cfg, pruefen, melden)
    aus, erfunden, bericht = zusammensetzen(hdr, clip, schaetzung, h_klein, c_klein)
    masken = {k.removesuffix(".Y"): v for k, v in kanaele.items() if k.endswith(".Y")}
    masken["erfunden"] = erfunden
    exr.schreiben(ziel, aus, masken, {
        "stage_ingest_stufe": "KI: ausgebrannte Stellen mit DiffHDR geschätzt (Kanal erfunden), sonst gemessen",
        "stage_ingest_ki": "DiffHDR_Pano (Eyeline Labs, Apache 2.0) auf Wan2.1-VACE-14B",
        "stage_ingest_farbraum": "linear, Primärfarben Rec.709/sRGB",
    })
    bericht["sekunden"] = round(time.time() - start)
    return bericht
