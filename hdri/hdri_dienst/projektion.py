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


def mittigkeit(kamera: Kamera, mx: np.ndarray, my: np.ndarray) -> np.ndarray:
    """1 in der Bildmitte, fallend zum Rand (elliptisch): wo ein Bild am wenigsten verzerrt und am genauesten ist."""
    dx = (mx - kamera.breite / 2) / (kamera.breite / 2)
    dy = (my - kamera.hoehe / 2) / (kamera.hoehe / 2)
    return np.clip(1.0 - np.sqrt(dx * dx + dy * dy) / np.sqrt(2), 0.0, 1.0).astype(np.float32)


@dataclass
class Position:
    """Ein zusammengeführtes Bild (linear) mit Lage und Kamera; optional seine Clip-Maske."""

    bild: np.ndarray
    lage: np.ndarray
    kamera: Kamera
    clip: np.ndarray | None = None


#: Schärfe der Übergänge: Gewicht hoch diese Zahl. 1 = weiches Mitteln (Geisterbilder bei Parallaxe),
#: höher = jede Stelle kommt fast nur aus dem Bild, das sie am mittigsten sieht (schmale Nähte statt Geister).
SCHAERFE = 8


def panorama(positionen: list[Position], hoehe: int) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Gibt (Panorama H×2H×3 float32, Clip-Maske H×2H float32 0..1, Abdeckung H×2H float32) zurück.

    Abdeckung = Zahl der Aufnahmen, die eine Richtung sehen (weich); 0 heisst Loch (typisch am Nadir).
    Gemischt wird mit Gewicht = Randabfall × Mittigkeit^SCHAERFE: wo sich Bilder überlappen, dominiert das, das die
    Stelle am mittigsten sieht. Bei Parallaxe (aus der Hand) gibt das schmale Nähte statt durchscheinender Geister.
    """
    welt = richtungen(hoehe)
    summe = np.zeros((hoehe, 2 * hoehe, 3), dtype=np.float64)
    clip = np.zeros((hoehe, 2 * hoehe), dtype=np.float64)
    gewichte = np.zeros((hoehe, 2 * hoehe), dtype=np.float64)
    abdeckung = np.zeros((hoehe, 2 * hoehe), dtype=np.float64)
    for p in positionen:
        mx, my, w = abbildung(p.kamera, p.lage, welt)
        if not w.any():
            continue
        # Nach der Entzerrung schwarze Ränder: dort sieht das Bild nichts.
        gueltig = cv2.remap((p.bild.max(axis=2) > 0).astype(np.float32), mx, my, interpolation=cv2.INTER_LINEAR,
                            borderMode=cv2.BORDER_CONSTANT) > 0.99
        w = np.where(gueltig, w, 0.0)
        abdeckung += w
        scharf = (w * mittigkeit(p.kamera, mx, my) ** SCHAERFE + 1e-12 * (w > 0)).astype(np.float64)
        werte = cv2.remap(p.bild, mx, my, interpolation=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)
        summe += werte * scharf[..., None]
        gewichte += scharf
        if p.clip is not None:
            c = cv2.remap(p.clip.astype(np.float32), mx, my, interpolation=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)
            clip += c * scharf
    bedeckt = gewichte > 0
    bild = np.where(bedeckt[..., None], summe / np.where(bedeckt, gewichte, 1.0)[..., None], 0.0)
    maske = np.where(bedeckt, clip / np.where(bedeckt, gewichte, 1.0), 0.0)
    return bild.astype(np.float32), maske.astype(np.float32), abdeckung.astype(np.float32)
