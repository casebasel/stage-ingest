"""DNG-Korrekturen aus `OpcodeList3`, die LibRaw nicht anwendet: Entzerrung und Randabdunklung.

Das iPhone schreibt ins Bayer-RAW (DNG) die Korrekturen des Objektivs nur als Anweisung (DNG-Spezifikation 1.6,
Kapitel „Opcode List Processing“); angewendet werden sie erst beim Entwickeln. Ohne sie passen die Bilder an den
Rändern nicht aufeinander (Verzeichnung des Ultraweitwinkels) und die Lichtwerte fallen zum Rand hin ab
(Randabdunklung bis etwa ×6 in den Ecken). Gemessen am iPhone 13 Pro (`iPhone14,4 back ultra wide camera`):
WarpRectilinear2 (ID 14) und FixVignetteRadial (ID 3).

Angewendet im Bild nach dem Entwickeln, in Sensor-Ausrichtung (vor dem Drehen nach dem Orientation-Tag), auf der
Fläche der ActiveArea (= Ausgabe von LibRaw). Koordinaten normiert wie in der Spezifikation: Mittelpunkt (cx, cy)
relativ zu Breite und Höhe, Radius relativ zum grössten Abstand vom Mittelpunkt zu einer Ecke.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass

import cv2
import numpy as np


@dataclass
class Verzerrung:
    """WarpRectilinear (1) bzw. WarpRectilinear2 (14), eine Ebene für alle Farben."""

    kr: list[float]  # radiale Koeffizienten zu r^0, r^1, … (bei ID 1 nur gerade Potenzen, hier ausgeschrieben)
    kt: tuple[float, float]
    cx: float
    cy: float
    kehrwert: bool  # ReciprocalRadial: Faktor = 1 / Polynom


@dataclass
class Vignette:
    """FixVignetteRadial (3): Verstärkung g = 1 + k0 r² + k1 r⁴ + … + k4 r¹⁰."""

    k: list[float]
    cx: float
    cy: float


def lesen(daten: bytes) -> tuple[Verzerrung | None, Vignette | None]:
    """Liest eine OpcodeList (big-endian). Unbekannte Opcodes werden übersprungen."""
    verz, vign = None, None
    if not daten:
        return None, None
    anzahl = struct.unpack(">I", daten[:4])[0]
    o = 4
    for _ in range(anzahl):
        kennung, _version, _flags, groesse = struct.unpack(">IIII", daten[o : o + 16])
        d = daten[o + 16 : o + 16 + groesse]
        if kennung == 14:  # WarpRectilinear2
            ebenen = struct.unpack(">I", d[:4])[0]
            # je Ebene 19 Werte: kr0..kr14, kt0, kt1, kleinster und grösster gültiger Radius; danach cx, cy, Kehrwert
            w = struct.unpack(">19d", d[4 : 4 + 19 * 8])  # erste Ebene gilt für alle Farben
            ende = 4 + 19 * 8 * ebenen
            cx, cy = struct.unpack(">2d", d[ende : ende + 16])
            kehrwert = struct.unpack(">I", d[ende + 16 : ende + 20])[0] == 1
            verz = Verzerrung(list(w[:15]), (w[15], w[16]), cx, cy, kehrwert)
        elif kennung == 1:  # WarpRectilinear (alt): kr0..kr3 für r^0, r^2, r^4, r^6
            ebenen = struct.unpack(">I", d[:4])[0]
            w = struct.unpack(">6d", d[4:52])
            cx, cy = struct.unpack(">2d", d[4 + 48 * ebenen : 4 + 48 * ebenen + 16])
            kr = [w[0], 0.0, w[1], 0.0, w[2], 0.0, w[3]]
            verz = Verzerrung(kr, (w[4], w[5]), cx, cy, False)
        elif kennung == 3:  # FixVignetteRadial
            w = struct.unpack(">7d", d[:56])
            vign = Vignette(list(w[:5]), w[5], w[6])
        o += 16 + groesse
    return verz, vign


def _radius(breite: int, hoehe: int, cx: float, cy: float) -> tuple[np.ndarray, np.ndarray, float]:
    """Abstand jedes Pixels vom Mittelpunkt (dx, dy in Pixeln) und der Normierungsradius (grösste Ecke)."""
    mx, my = cx * (breite - 1), cy * (hoehe - 1)
    ecken = [(0, 0), (breite - 1, 0), (0, hoehe - 1), (breite - 1, hoehe - 1)]
    m = max(np.hypot(x - mx, y - my) for x, y in ecken)
    x, y = np.meshgrid(np.arange(breite, dtype=np.float32), np.arange(hoehe, dtype=np.float32))
    return x - mx, y - my, float(m)


def vignette_anwenden(bild: np.ndarray, v: Vignette) -> np.ndarray:
    h, b = bild.shape[:2]
    dx, dy, m = _radius(b, h, v.cx, v.cy)
    r2 = (dx * dx + dy * dy) / (m * m)
    g = np.ones_like(r2)
    p = r2.copy()
    for k in v.k:
        g += k * p
        p *= r2
    # Gesättigte Pixel bleiben gesättigt (Clip-Erkennung sieht sie weiter); alles andere wird linear aufgehellt.
    return (bild * g[..., None]).astype(np.float32)


def verzerrung_anwenden(bild: np.ndarray, v: Verzerrung) -> np.ndarray:
    """Ziel → Quelle: x_quelle = Mitte + Faktor(r) · (x_ziel − Mitte); r im Ziel gemessen."""
    h, b = bild.shape[:2]
    dx, dy, m = _radius(b, h, v.cx, v.cy)
    r = np.sqrt(dx * dx + dy * dy) / m
    poly = np.zeros_like(r)
    p = np.ones_like(r)
    for k in v.kr:
        poly += k * p
        p *= r
    faktor = 1.0 / poly if v.kehrwert else poly
    kt0, kt1 = v.kt
    rn2 = r * r
    xn, yn = dx / m, dy / m
    # Tangentiale Anteile (beim iPhone null), Formel nach der DNG-Spezifikation.
    tx = 2 * kt0 * xn * yn + kt1 * (rn2 + 2 * xn * xn)
    ty = kt0 * (rn2 + 2 * yn * yn) + 2 * kt1 * xn * yn
    mx, my = v.cx * (b - 1), v.cy * (h - 1)
    karte_x = (mx + (xn * faktor + tx) * m).astype(np.float32)
    karte_y = (my + (yn * faktor + ty) * m).astype(np.float32)
    return cv2.remap(bild, karte_x, karte_y, interpolation=cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT)
