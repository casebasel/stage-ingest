"""Flächiges Ausrichten für Nachbarbilder ohne gemeinsame Merkmale (weisse Wand, Decke).

VERSUCH (08.10.2026), nicht im Dienst: verbindet im Wohnzimmer 5–10 zusätzliche Paare; Prüfung am echten Raum mit
einem HDRI im neuen Aufnahmemuster steht aus.

Merkmale (SIFT) brauchen Ecken und Muster; eine glatte Wand hat keine. Sie hat aber Helligkeitsverläufe, Kanten zur
Decke, Türrahmen und Schatten. Hier werden zwei überlappende Bilder in eine gemeinsame Zentralprojektion um die Mitte
ihrer Blickrichtungen gelegt und mit dem ECC-Verfahren (OpenCV `findTransformECC`, Helligkeit Pixel für Pixel)
gegeneinander ausgerichtet. Das Ergebnis wird als Punktpaare ausgegeben, wie sie auch die Merkmalssuche liefert; der
Bündelausgleich behandelt beides gleich.
"""

from __future__ import annotations

import cv2
import numpy as np

from .projektion import Position

#: Kantenlänge und Bildwinkel der gemeinsamen Zentralprojektion.
GROESSE = 512
WINKEL_GRAD = 70.0
#: Mindestgüte der Ausrichtung (ECC-Korrelation) und grösste plausible Korrektur.
MIN_GUETE = 0.75
MAX_KORREKTUR_GRAD = 12.0
#: Punktpaare je ausgerichtetem Paar (Gitter in der Überlappung).
PUNKTE = 60


def _achse(p: Position) -> np.ndarray:
    return p.lage @ np.array([0.0, 0.0, -1.0])


def _basis(mitte: np.ndarray) -> np.ndarray:
    """Orthonormale Basis (rechts, oben, Blick) um eine Weltrichtung; oben möglichst zum Zenit."""
    blick = mitte / np.linalg.norm(mitte)
    hilfe = np.array([0.0, 0.0, 1.0]) if abs(blick[2]) < 0.95 else np.array([0.0, 1.0, 0.0])
    rechts = np.cross(blick, hilfe)
    rechts /= np.linalg.norm(rechts)
    oben = np.cross(rechts, blick)
    return np.stack([rechts, oben, blick])


def _strahlen_ebene(basis: np.ndarray) -> np.ndarray:
    """Weltstrahlen der Pixel der Zentralprojektion (G×G×3)."""
    t = np.tan(np.radians(WINKEL_GRAD) / 2)
    u = (np.arange(GROESSE) + 0.5) / GROESSE * 2 - 1
    x, y = np.meshgrid(u * t, -u * t)
    v = x[..., None] * basis[0] + y[..., None] * basis[1] + basis[2]
    return v / np.linalg.norm(v, axis=2, keepdims=True)


def welt_zu_pixel(p: Position, welt: np.ndarray, massstab: float = 1.0) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Weltstrahlen → Pixel (x, y) im Bild von `p`, dazu gültig (vor der Kamera und im Bild)."""
    c = welt @ p.lage  # Zeilenvektoren: Kamera = Weltᵀ · R
    z = -c[..., 2]
    k = p.kamera
    vor = z > 1e-6
    zs = np.where(vor, z, 1.0)
    x = k.fx * massstab * c[..., 0] / zs + k.breite / 2 - 0.5
    y = -k.fy * massstab * c[..., 1] / zs + k.hoehe / 2 - 0.5
    drin = vor & (x >= 0) & (x <= k.breite - 1) & (y >= 0) & (y <= k.hoehe - 1)
    return x.astype(np.float32), y.astype(np.float32), drin


def _ansicht(p: Position, welt: np.ndarray, massstab: float) -> tuple[np.ndarray, np.ndarray]:
    x, y, drin = welt_zu_pixel(p, welt, massstab)
    lum = 0.2126 * p.bild[..., 0] + 0.7152 * p.bild[..., 1] + 0.0722 * p.bild[..., 2]
    log = np.log2(np.maximum(lum, 1e-5)).astype(np.float32)
    bild = cv2.remap(log, x, y, interpolation=cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT)
    gueltig = drin & (cv2.remap((p.bild.max(axis=2) > 0).astype(np.float32), x, y, interpolation=cv2.INTER_LINEAR) > 0.99)
    return bild, gueltig


def ausrichten_paar(a: Position, b: Position, massstab: float = 1.0):
    """Richtet b gegen a in der gemeinsamen Ansicht aus. Gibt (Punkte in a, Punkte in b, Güte) oder None zurück."""
    basis = _basis(_achse(a) + _achse(b))
    welt = _strahlen_ebene(basis)
    ia, ga = _ansicht(a, welt, massstab)
    ib, gb = _ansicht(b, welt, massstab)
    beide = ga & gb
    if beide.mean() < 0.08:
        return None
    # Gleiche Helligkeit erzwingen (Belichtung/Randabfall), dann leicht glätten; ECC auf den Log-Werten.
    for img in (ia, ib):
        img[~beide] = 0
    ia = (ia - ia[beide].mean()) / max(ia[beide].std(), 1e-6)
    ib = (ib - ib[beide].mean()) / max(ib[beide].std(), 1e-6)
    ia = cv2.GaussianBlur(np.where(beide, ia, 0).astype(np.float32), (0, 0), 1.5)
    ib = cv2.GaussianBlur(np.where(beide, ib, 0).astype(np.float32), (0, 0), 1.5)
    maske = cv2.erode(beide.astype(np.uint8), np.ones((9, 9), np.uint8))
    warp = np.eye(3, dtype=np.float32)
    try:
        guete, warp = cv2.findTransformECC(ia, ib, warp, cv2.MOTION_HOMOGRAPHY,
                                           (cv2.TERM_CRITERIA_EPS | cv2.TERM_CRITERIA_COUNT, 200, 1e-6), maske, 5)
    except cv2.error:
        return None
    if guete < MIN_GUETE:
        return None
    # Gitterpunkte in der Überlappung mit Struktur: Pixel q in a's Ansicht entspricht warp·q in b's Ansicht.
    gy, gx = np.nonzero(maske)
    if len(gx) < PUNKTE:
        return None
    struktur = np.hypot(*np.gradient(ia))[gy, gx]
    wahl = np.argsort(-struktur)[: PUNKTE * 4][:: 4]
    qa = np.stack([gx[wahl], gy[wahl], np.ones(len(wahl))], axis=1).astype(np.float64)
    qb = qa @ warp.T.astype(np.float64)
    qb = qb[:, :2] / qb[:, 2:3]
    t = np.tan(np.radians(WINKEL_GRAD) / 2)

    def ebene_zu_welt(q):
        x = ((q[:, 0] + 0.5) / GROESSE * 2 - 1) * t
        y = -((q[:, 1] + 0.5) / GROESSE * 2 - 1) * t
        v = x[:, None] * basis[0] + y[:, None] * basis[1] + basis[2]
        return v / np.linalg.norm(v, axis=1, keepdims=True)

    wa, wb = ebene_zu_welt(qa), ebene_zu_welt(qb)
    # Plausibilität: Die Korrektur (Winkel zwischen wa und wb) bleibt klein.
    if np.degrees(np.median(np.arccos(np.clip(np.sum(wa * wb, 1), -1, 1)))) > MAX_KORREKTUR_GRAD:
        return None
    xa, ya, da = welt_zu_pixel(a, wa, massstab)
    xb, yb, db = welt_zu_pixel(b, wb, massstab)
    ok = da & db
    if ok.sum() < PUNKTE // 2:
        return None
    return np.stack([xa[ok], ya[ok]], 1).astype(np.float64), np.stack([xb[ok], yb[ok]], 1).astype(np.float64), float(guete)


def ergaenzen(positionen: list[Position], paare: list, massstab: float = 1.0, melden=None) -> list:
    """Ergänzt Paare für benachbarte Bilder, die über Merkmale nicht verbunden sind (Nachbar = Blickachsen höchstens
    so weit auseinander, dass sich die Bilder deutlich überlappen)."""
    vorhanden = {(i, j) for i, j, *_ in paare} | {(j, i) for i, j, *_ in paare}
    achsen = [_achse(p) for p in positionen]
    neu = []
    for i in range(len(positionen)):
        for j in range(i + 1, len(positionen)):
            if (i, j) in vorhanden:
                continue
            k = positionen[i].kamera
            winkel = np.degrees(np.arccos(np.clip(achsen[i] @ achsen[j], -1, 1)))
            if winkel > 0.75 * min(k.hfov_grad, k.vfov_grad) + 10:
                continue
            r = ausrichten_paar(positionen[i], positionen[j], massstab)
            if r is not None:
                neu.append((i, j, r[0], r[1]))
                if melden:
                    melden(f"flächig verbunden: {i}–{j} (Güte {r[2]:.2f})")
    return paare + neu
