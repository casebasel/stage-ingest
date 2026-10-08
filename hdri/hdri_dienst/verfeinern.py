"""Lage verfeinern: Die Lage aus dem iPhone (CoreMotion) ist nur auf ±1–2° genau. Gemeinsame Merkmale in
überlappenden Bildern geben die genaue relative Drehung; optimiert werden alle Lagen zusammen und ein gemeinsamer
Massstab der Brennweite (Apples Sichtfeld gilt für das entzerrte Video, nicht genau für das entzerrte RAW).

Verfahren (wie jjc256/photosphere, Briefing Kapitel 6): IMU-Lage als Startwert → SIFT-Merkmale je Position →
Paare nur zwischen Positionen, die sich laut IMU überlappen → Treffer, deren Strahlen nach der IMU-Lage höchstens
einige Grad auseinanderliegen → Bündelausgleich der Drehungen (kleine Korrekturen δ je Position, robust) mit
schwacher Bindung an die IMU-Lage, damit Horizont (Schwerkraft) und Yaw 0 erhalten bleiben.
"""

from __future__ import annotations

from dataclasses import replace

import cv2
import numpy as np
from scipy.optimize import least_squares
from scipy.spatial.transform import Rotation

from .projektion import Kamera, Position

#: Kantenlänge der Bilder für die Merkmalssuche (schnell und genug für 0,1° bei ~100° Sichtfeld).
SUCHGROESSE = 1000


def _grau(bild: np.ndarray) -> np.ndarray:
    """Lineares HDR → 8 bit Grau mit log-Tonung (Merkmale in hellen und dunklen Bereichen)."""
    lum = 0.2126 * bild[..., 0] + 0.7152 * bild[..., 1] + 0.0722 * bild[..., 2]
    log = np.log1p(np.maximum(lum, 0) / max(float(np.percentile(lum, 50)), 1e-6) * 4)
    log = log / max(float(np.percentile(log, 99.5)), 1e-6)
    return (np.clip(log, 0, 1) * 255).astype(np.uint8)


def _strahlen(punkte: np.ndarray, kamera: Kamera, massstab: float) -> np.ndarray:
    """Bildpunkte (x, y in Pixeln des Originals) → Einheitsstrahlen in Kamerakoordinaten (x rechts, y oben, Blick −z)."""
    x = (punkte[:, 0] + 0.5 - kamera.breite / 2) / (kamera.fx * massstab)
    y = -(punkte[:, 1] + 0.5 - kamera.hoehe / 2) / (kamera.fy * massstab)
    v = np.stack([x, y, -np.ones_like(x)], axis=1)
    return v / np.linalg.norm(v, axis=1, keepdims=True)


def merkmale(positionen: list[Position]) -> list[tuple[np.ndarray, np.ndarray]]:
    sift = cv2.SIFT_create(nfeatures=4000)
    aus = []
    for p in positionen:
        g = _grau(p.bild)
        f = SUCHGROESSE / max(g.shape)
        klein = cv2.resize(g, None, fx=f, fy=f, interpolation=cv2.INTER_AREA)
        # Ränder der Entzerrung (schwarz) ausblenden
        maske = (cv2.resize((p.bild.max(axis=2) > 0).astype(np.uint8), (klein.shape[1], klein.shape[0])) > 0).astype(np.uint8)
        maske = cv2.erode(maske, np.ones((15, 15), np.uint8))
        kp, desk = sift.detectAndCompute(klein, maske)
        pts = np.array([k.pt for k in kp], dtype=np.float64) / f if kp else np.zeros((0, 2))
        aus.append((pts, desk))
    return aus


def _kabsch(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    """Drehung R mit R · a ≈ b (Zeilenvektoren)."""
    u, _, vt = np.linalg.svd(b.T @ a)
    d = np.sign(np.linalg.det(u @ vt))
    return u @ np.diag([1, 1, d]) @ vt


def _ransac(ri: np.ndarray, rj: np.ndarray, grenze_rad: float, runden: int = 400, zufall=None):
    """Relative Drehung R (Kamera j → Kamera i, ri ≈ R · rj) aus Strahlenpaaren; robust gegen Fehltreffer."""
    zufall = zufall or np.random.default_rng(0)
    bestes, beste_r = np.zeros(len(ri), bool), None
    for _ in range(runden):
        k = zufall.choice(len(ri), 2, replace=False)
        r = _kabsch(rj[k], ri[k])
        innen = np.arccos(np.clip(np.sum(ri * (rj @ r.T), axis=1), -1, 1)) < grenze_rad
        if innen.sum() > bestes.sum():
            bestes, beste_r = innen, r
    if beste_r is None or bestes.sum() < 3:
        return None, bestes
    r = _kabsch(rj[bestes], ri[bestes])  # mit allen Inliern nachrechnen
    innen = np.arccos(np.clip(np.sum(ri * (rj @ r.T), axis=1), -1, 1)) < grenze_rad
    return r, innen


def treffer(
    positionen: list[Position], mm: list[tuple[np.ndarray, np.ndarray]], grenze_grad: float = 1.0, imu_grad: float = 20.0
) -> list[tuple[int, int, np.ndarray, np.ndarray]]:
    """Paare (i, j, Punkte in i, Punkte in j): Verhältnistest, dann RANSAC der relativen Drehung je Paar.

    Akzeptiert nur, wenn die gefundene Drehung höchstens `imu_grad` von der IMU-Lage abweicht (sich wiederholende
    Muster wie Fliesen liefern sonst ein falsches, aber in sich stimmiges Paar)."""
    abgleich = cv2.BFMatcher(cv2.NORM_L2)
    achsen = [p.lage @ np.array([0, 0, -1.0]) for p in positionen]
    paare = []
    for i in range(len(positionen)):
        for j in range(i + 1, len(positionen)):
            k = positionen[i].kamera
            winkel = np.degrees(np.arccos(np.clip(achsen[i] @ achsen[j], -1, 1)))
            if winkel > (k.hfov_grad + k.vfov_grad) / 2:
                continue
            (pi, di), (pj, dj) = mm[i], mm[j]
            if di is None or dj is None or len(pi) < 10 or len(pj) < 10:
                continue
            m = [a for a, b in abgleich.knnMatch(di, dj, k=2) if a.distance < 0.8 * b.distance]
            if len(m) < 12:
                continue
            qi, qj = pi[[x.queryIdx for x in m]], pj[[x.trainIdx for x in m]]
            ri, rj = _strahlen(qi, positionen[i].kamera, 1.0), _strahlen(qj, positionen[j].kamera, 1.0)
            r, innen = _ransac(ri, rj, np.radians(grenze_grad))
            if r is None or innen.sum() < 12:
                continue
            # IMU: ri = Riᵀ Rj rj → relative Drehung laut IMU
            imu = positionen[i].lage.T @ positionen[j].lage
            abw = np.degrees(np.linalg.norm(Rotation.from_matrix(r @ imu.T).as_rotvec()))
            if abw > imu_grad:
                continue
            paare.append((i, j, qi[innen], qj[innen]))
    return paare


#: Bindung an die IMU je Achse der Welt (x, y = Neigung/Rollen aus der Schwerkraft, genau; z = Yaw, driftet).
#: Ohne Bindung an die Schwerkraft verbiegt sich der Horizont, wo Merkmale fehlen (weisse Wände); zu starke Bindung
#: gibt Doppelkanten. 0,3 an zwei echten Aufnahmen (Küche, Wohnzimmer, 08.10.2026) gewählt: Horizont gerade,
#: Restfehler 0,15° bzw. 0,35°.
BINDUNG = np.array([0.3, 0.3, 0.02])
#: Bereich für den Massstab der Brennweite gegenüber Apples Sichtfeld (gemessen am iPhone 13 Pro: ~1,10).
MASSSTAB = (0.95, 1.15)


def ausgleichen(positionen: list[Position], paare, bindung: np.ndarray = BINDUNG) -> tuple[list[Position], dict]:
    """Bündelausgleich: δ (Drehvektor, Welt) je Position und ein Massstab der Brennweite (begrenzt)."""
    n = len(positionen)

    def zerlegen(x):
        return x[: 3 * n].reshape(n, 3), float(np.exp(x[3 * n]))

    def reste(x):
        deltas, s = zerlegen(x)
        lagen = [Rotation.from_rotvec(d).as_matrix() @ p.lage for d, p in zip(deltas, positionen)]
        teile = []
        for i, j, pi, pj in paare:
            wi = _strahlen(pi, positionen[i].kamera, s) @ lagen[i].T
            wj = _strahlen(pj, positionen[j].kamera, s) @ lagen[j].T
            teile.append(np.cross(wi, wj).ravel())  # ≈ Winkel in rad
        # Gewichtet mit der Wurzel der Trefferzahl, damit die Bindung nicht von vielen Treffern überstimmt wird.
        teile.append((bindung[None, :] * deltas).ravel() * np.sqrt(max(1, anzahl) / max(1, n)))
        return np.concatenate(teile)

    anzahl = sum(len(x[2]) for x in paare)
    x0 = np.zeros(3 * n + 1)
    x0[-1] = np.log(1.05)
    unten = np.full(3 * n + 1, -np.inf)
    oben = np.full(3 * n + 1, np.inf)
    unten[-1], oben[-1] = np.log(MASSSTAB[0]), np.log(MASSSTAB[1])
    x_imu = np.zeros(3 * n + 1)
    vorher = np.abs(reste(x_imu)[: -3 * n]).mean() if paare else 0.0
    erg = least_squares(reste, x0, loss="soft_l1", f_scale=np.radians(0.3), max_nfev=300, bounds=(unten, oben))
    deltas, s = zerlegen(erg.x)
    nachher = np.abs(reste(erg.x)[: -3 * n]).mean() if paare else 0.0
    neu = []
    for d, p in zip(deltas, positionen):
        k = p.kamera
        # Massstab der Brennweite als geändertes Sichtfeld weitergeben
        hf = 2 * np.degrees(np.arctan(np.tan(np.radians(k.hfov_grad) / 2) / s))
        vf = 2 * np.degrees(np.arctan(np.tan(np.radians(k.vfov_grad) / 2) / s))
        neu.append(replace(p, lage=Rotation.from_rotvec(d).as_matrix() @ p.lage, kamera=replace(k, hfov_grad=hf, vfov_grad=vf)))
    bericht = {
        "paare": len(paare),
        "treffer": int(sum(len(x[2]) for x in paare)),
        "fehler_vorher_grad": float(np.degrees(vorher)),
        "fehler_nachher_grad": float(np.degrees(nachher)),
        "massstab": s,
        "groesste_korrektur_grad": float(np.degrees(np.linalg.norm(deltas, axis=1).max())) if n else 0.0,
    }
    return neu, bericht


def verfeinern(positionen: list[Position], melden=None) -> tuple[list[Position], dict]:
    mm = merkmale(positionen)
    paare = treffer(positionen, mm)
    if not paare:
        return positionen, {"paare": 0}
    neu, b = ausgleichen(positionen, paare)
    if melden:
        melden(f"Lage verfeinert: {b['paare']} Paare, {b['treffer']} Treffer, Fehler {b['fehler_vorher_grad']:.2f}° → {b['fehler_nachher_grad']:.2f}°, Massstab {b['massstab']:.3f}", 1.0)
    return neu, b
