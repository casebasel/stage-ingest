"""Bilder mit bekannter Lage in ein equirectangulares Panorama (2:1) legen.

Koordinaten wie im Plate Assistant (Systemkarte, `hdri_frame.lage_quaternion` [w, x, y, z], Kamera → Welt,
v_welt = q · v_kamera):
- Kamera: x = rechts, y = oben, Blick = −z.
- Welt: x = rechts von Yaw 0, y = Yaw 0 waagrecht, z = oben.

Panorama: Mitte (u = B/2) = Yaw 0, nach rechts zunehmender Azimut (im Uhrzeigersinn von oben gesehen, also zu
+x), oben = Zenit. Breite = 2 × Höhe.

Die IMU-Lage ist nur ein Startwert (±1–2°); die Verfeinerung über Merkmale kommt in einem zweiten Schritt.
Dieses Modul rechnet die Abbildung exakt aus der gegebenen Lage, damit die Verfeinerung nur noch die Lage
korrigieren muss.
"""

from __future__ import annotations

from dataclasses import dataclass

import cv2
import numpy as np


def quaternion_zu_matrix(q: tuple[float, float, float, float]) -> np.ndarray:
    """[w, x, y, z] → 3×3-Drehmatrix (normiert)."""
    w, x, y, z = np.asarray(q, dtype=np.float64) / np.linalg.norm(q)
    return np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ]
    )


def richtungen(hoehe: int) -> np.ndarray:
    """Weltrichtung jedes Panoramapixels: H×2H×3 (Einheitsvektoren, Pixelmitten)."""
    breite = 2 * hoehe
    u = (np.arange(breite) + 0.5) / breite  # 0..1
    v = (np.arange(hoehe) + 0.5) / hoehe
    azimut = (u - 0.5) * 2 * np.pi  # 0 in der Mitte, + nach rechts
    hoehenwinkel = (0.5 - v) * np.pi  # +π/2 oben
    az, hw = np.meshgrid(azimut, hoehenwinkel)
    return np.stack([np.sin(az) * np.cos(hw), np.cos(az) * np.cos(hw), np.sin(hw)], axis=-1)


@dataclass
class Kamera:
    """Lochkamera (Verzeichnung schon entfernt) mit horizontalem und vertikalem Sichtfeld in Grad."""

    breite: int
    hoehe: int
    hfov_grad: float
    vfov_grad: float

    @property
    def fx(self) -> float:
        return (self.breite / 2) / np.tan(np.radians(self.hfov_grad) / 2)

    @property
    def fy(self) -> float:
        return (self.hoehe / 2) / np.tan(np.radians(self.vfov_grad) / 2)


def abbildung(kamera: Kamera, lage: np.ndarray, welt: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Für jede Panoramarichtung: Bildkoordinaten (map_x, map_y, float32) und Gewicht (0 ausserhalb).

    Gewicht: weich zum Bildrand hin (cos-Abfall über die äusseren 15 %), damit Übergänge nicht sichtbar sind.
    """
    # Welt → Kamera: v_kamera = Rᵀ · v_welt (R = Kamera → Welt).
    v = welt @ lage  # entspricht (Rᵀ · vᵀ)ᵀ
    x, y, z = v[..., 0], v[..., 1], v[..., 2]
    vorne = -z
    gueltig = vorne > 1e-6
    vorne_sicher = np.where(gueltig, vorne, 1.0)
    map_x = kamera.fx * (x / vorne_sicher) + kamera.breite / 2 - 0.5
    map_y = kamera.hoehe / 2 - kamera.fy * (y / vorne_sicher) - 0.5
    innen = gueltig & (map_x >= 0) & (map_x <= kamera.breite - 1) & (map_y >= 0) & (map_y <= kamera.hoehe - 1)
    # Abstand zum Rand relativ (0 am Rand, 1 in der Mitte), weicher Übergang in den äusseren 15 %.
    rand_x = np.minimum(map_x, kamera.breite - 1 - map_x) / (kamera.breite / 2)
    rand_y = np.minimum(map_y, kamera.hoehe - 1 - map_y) / (kamera.hoehe / 2)
    rand = np.clip(np.minimum(rand_x, rand_y) / 0.15, 0.0, 1.0)
    gewicht = np.where(innen, 0.5 - 0.5 * np.cos(np.pi * rand), 0.0)
    return map_x.astype(np.float32), map_y.astype(np.float32), gewicht.astype(np.float32)


@dataclass
class Position:
    """Ein zusammengeführtes Bild (linear) mit Lage und Kamera; optional seine Clip-Maske."""

    bild: np.ndarray
    lage: np.ndarray
    kamera: Kamera
    clip: np.ndarray | None = None


def panorama(positionen: list[Position], hoehe: int) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Gibt (Panorama H×2H×3 float32, Clip-Maske H×2H float32 0..1, Abdeckung H×2H float32) zurück.

    Abdeckung = Summe der Gewichte; 0 heisst „keine Aufnahme sieht diese Richtung“ (Loch, typisch am Nadir).
    """
    welt = richtungen(hoehe)
    summe = np.zeros((hoehe, 2 * hoehe, 3), dtype=np.float64)
    clip = np.zeros((hoehe, 2 * hoehe), dtype=np.float64)
    gewichte = np.zeros((hoehe, 2 * hoehe), dtype=np.float64)
    for p in positionen:
        mx, my, w = abbildung(p.kamera, p.lage, welt)
        if not w.any():
            continue
        werte = cv2.remap(p.bild, mx, my, interpolation=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)
        summe += werte * w[..., None]
        gewichte += w
        if p.clip is not None:
            c = cv2.remap(p.clip.astype(np.float32), mx, my, interpolation=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)
            clip += c * w
    bedeckt = gewichte > 1e-6
    bild = np.where(bedeckt[..., None], summe / np.maximum(gewichte, 1e-6)[..., None], 0.0)
    maske = np.where(bedeckt, clip / np.maximum(gewichte, 1e-6), 0.0)
    return bild.astype(np.float32), maske.astype(np.float32), gewichte.astype(np.float32)
