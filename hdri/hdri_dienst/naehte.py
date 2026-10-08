"""Bessere Nähte für Aufnahmen aus der Hand (Parallaxe): Schnitte um Objekte herum und lokales Angleichen.

VERSUCH (08.10.2026), nicht im Dienst: Am Wohnzimmer brachten Graph-Cut und Fluss allein wenig und verschlimmerten
grosse Versätze (Fenster). Hauptursache sind unverbundene Positionen und das Aufnahmemuster (docs/HDRI-KI-TEST.md).

Aus der Hand wandert das Objektiv zwischen den Positionen um einige Zentimeter. Eine gemeinsame Drehung kann nahe und
ferne Dinge dann nie zugleich decken; an den Nähten springen Kanten (Schrank, Lampe). Zwei Schritte ohne KI, die nur
gemessene Pixel verschieben und nichts erfinden:

1. **Nähte legen** (Graph-Cut, OpenCV `detail.GraphCutSeamFinder`): In den Überlappungen läuft der Schnitt dort, wo
   sich die Bilder am ähnlichsten sind (ruhige Wand, Boden), statt an fester Stelle mitten durch ein Möbel.
2. **Lokal angleichen** (optischer Fluss, OpenCV DIS): Entlang jeder Naht wird gemessen, wie weit die beiden Bilder
   gegeneinander verschoben sind; jedes Bild wird nahe der Naht um die Hälfte davon verschoben, abklingend über ein
   schmales Band. An der Naht treffen sich beide in der Mitte, im Inneren bleibt jedes Bild unverändert.

Danach mischt `projektion.panorama_multiband` wie bisher (Details aus einem Bild, grobe Übergänge weich, im Log).
"""

from __future__ import annotations

import cv2
import numpy as np

from .projektion import Position, abbildung, mittigkeit, richtungen

#: Auflösung (Höhe) für Nahtsuche und Fluss; das Ergebnis wird auf die volle Höhe übertragen.
ARBEITSHOEHE = 512
#: Breite des Bandes, in dem das Angleichen abklingt, in Grad.
BAND_GRAD = 6.0
#: Grösste erlaubte Verschiebung in Grad (darüber ist der Fluss unzuverlässig; Parallaxe aus der Hand bleibt darunter).
MAX_VERSCHIEBUNG_GRAD = 4.0


def _grau(bild: np.ndarray) -> np.ndarray:
    """Lineares HDR → 8-bit-Log-Helligkeit (gleiche Abbildung für alle Bilder, damit der Fluss vergleichbar ist)."""
    lum = 0.2126 * bild[..., 0] + 0.7152 * bild[..., 1] + 0.0722 * bild[..., 2]
    log = np.log2(np.maximum(lum, 1e-5))
    return np.clip((log + 12) / 16 * 255, 0, 255).astype(np.uint8)


def _projizieren(positionen: list[Position], hoehe: int):
    """Je Bild: (Karten mx, my, gültig, Log-Grau im Panorama, Mittigkeit)."""
    welt = richtungen(hoehe)
    aus = []
    for p in positionen:
        mx, my, w = abbildung(p.kamera, p.lage, welt)
        gueltig = (cv2.remap((p.bild.max(axis=2) > 0).astype(np.float32), mx, my, interpolation=cv2.INTER_LINEAR,
                             borderMode=cv2.BORDER_CONSTANT) > 0.99) & (w > 0)
        aus.append((mx, my, gueltig, mittigkeit(p.kamera, mx, my)))
    return aus


def besitzer_mittig(proj, groesse) -> np.ndarray:
    """Bisherige Regel: jede Stelle gehört dem Bild, das sie am mittigsten sieht."""
    beste = np.full(groesse, -1.0, np.float32)
    besitzer = np.full(groesse, -1, np.int16)
    for k, (_, _, g, m) in enumerate(proj):
        punkte = np.where(g, m + 1e-6, -1.0)
        neu = punkte > beste
        beste[neu], besitzer[neu] = punkte[neu], k
    return besitzer


def besitzer_graphcut(bilder_klein: list[np.ndarray], gueltig_klein: list[np.ndarray], start: np.ndarray) -> np.ndarray:
    """Nähte per Graph-Cut. Startet von der mittigen Zuordnung, verschiebt Schnitte nur innerhalb der Überlappungen."""
    n = len(bilder_klein)
    hoehe, breite = start.shape
    bilder = [cv2.UMat(b.astype(np.float32)) for b in bilder_klein]
    # Erlaubt: eigenes Gebiet plus Überlappung; Überlappung auf ein Band um die bisherige Naht begrenzt (sonst springt
    # der Schnitt zu weit weg, wo das Bild schon stark verzerrt ist).
    band = int(max(4, BAND_GRAD / 180 * hoehe))
    kern = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * band + 1, 2 * band + 1))
    masken = []
    for k in range(n):
        eigen = (start == k).astype(np.uint8)
        erlaubt = cv2.dilate(eigen, kern) & gueltig_klein[k].astype(np.uint8)
        masken.append(cv2.UMat((erlaubt * 255).astype(np.uint8)))
    ecken = [(0, 0)] * n
    finder = cv2.detail_GraphCutSeamFinder("COST_COLOR_GRAD")
    finder.find(bilder, ecken, masken)
    aus = np.full((hoehe, breite), -1, np.int16)
    # Wo mehrere Masken übrig bleiben, entscheidet die mittige Zuordnung; wo keine, ebenfalls.
    for k in range(n):
        m = masken[k].get() > 0
        frei = m & ((aus < 0) | (start == k))
        aus[frei] = k
    rest = aus < 0
    aus[rest] = start[rest]
    return aus


def verschiebungen(grau: list[np.ndarray], gueltig: list[np.ndarray], besitzer: np.ndarray) -> list[np.ndarray]:
    """Je Bild ein Verschiebungsfeld (H×W×2, Pixel der Arbeitsauflösung): halber Fluss zum Nachbarn an jeder Naht,
    abklingend ins eigene Gebiet."""
    hoehe, breite = besitzer.shape
    band = max(4.0, BAND_GRAD / 180 * hoehe)
    grenze = MAX_VERSCHIEBUNG_GRAD / 180 * hoehe
    dis = cv2.DISOpticalFlow_create(cv2.DISOPTICAL_FLOW_PRESET_MEDIUM)
    felder = [np.zeros((hoehe, breite, 2), np.float32) for _ in grau]
    gewicht = [np.zeros((hoehe, breite), np.float32) for _ in grau]
    n = len(grau)
    for a in range(n):
        eigen_a = besitzer == a
        if not eigen_a.any():
            continue
        # Abstand jedes Pixels zum Gebiet von b (in a's Gebiet): wie nah an der Naht a|b
        for b in range(n):
            if a == b:
                continue
            eigen_b = (besitzer == b).astype(np.uint8)
            if not eigen_b.any():
                continue
            nah = cv2.dilate(eigen_b, np.ones((3, 3), np.uint8)).astype(bool) & eigen_a
            if nah.sum() < 20:
                continue  # keine gemeinsame Naht
            abstand = cv2.distanceTransform((1 - eigen_b).astype(np.uint8), cv2.DIST_L2, 5)
            w = np.clip(1.0 - abstand / band, 0, 1) * eigen_a * gueltig[b]
            if not w.any():
                continue
            # Fluss b → a: b(x) ≈ a(x + F); a wird um die Hälfte in Richtung b gezogen: a'(x) = a(x + F/2)... für a
            # braucht es den Fluss a → b gespiegelt: a'(x) = a(x - F_ab/2) mit a(x) ≈ b(x + F_ab).
            beide = (gueltig[a] & gueltig[b]).astype(np.uint8) * 255
            ga = np.where(beide > 0, grau[a], 0).astype(np.uint8)
            gb = np.where(beide > 0, grau[b], 0).astype(np.uint8)
            f_ab = dis.calc(ga, gb, None)
            betrag = np.linalg.norm(f_ab, axis=2)
            f_ab = np.where((betrag <= grenze)[..., None], f_ab, 0)
            felder[a] += (-0.5 * f_ab) * w[..., None]
            gewicht[a] += w
    for a in range(n):
        g = gewicht[a]
        # Mehrere Nähte überlagert: gewichtetes Mittel, aber nie mehr als volle Stärke
        felder[a] = felder[a] / np.maximum(g, 1.0)[..., None]
    return felder


def panorama_genaeht(positionen: list[Position], hoehe: int, angleichen: bool = True, graphcut: bool = True,
                     stufen: int | None = None):
    """Wie `projektion.panorama_multiband`, mit Graph-Cut-Nähten und lokalem Angleichen.
    Gibt (Bild, Clip, Abdeckung, Bericht) zurück; der Bericht enthält den Nahtfehler vorher/nachher."""
    from . import projektion

    klein = min(ARBEITSHOEHE, hoehe)
    proj_k = _projizieren(positionen, klein)
    bilder_k, grau_k = [], []
    for p, (mx, my, g, _) in zip(positionen, proj_k):
        b = cv2.remap(p.bild, mx, my, interpolation=cv2.INTER_LINEAR,
                      borderMode=cv2.BORDER_CONSTANT)
        bilder_k.append(np.log2(np.maximum(b, 1e-5)) * g[..., None])
        grau_k.append(_grau(b))
    gueltig_k = [g for (_, _, g, _) in proj_k]
    start = besitzer_mittig(proj_k, (klein, 2 * klein))
    besitzer_k = besitzer_graphcut(bilder_k, gueltig_k, start) if graphcut else start
    felder_k = verschiebungen(grau_k, gueltig_k, besitzer_k) if angleichen else None
    bericht = {"nahtfehler_vorher": nahtfehler(grau_k, start), "nahtfehler_nachher": None}
    if felder_k is not None:
        grau_neu = [_verschieben(g, f) for g, f in zip(grau_k, felder_k)]
        bericht["nahtfehler_nachher"] = nahtfehler(grau_neu, besitzer_k)
    else:
        bericht["nahtfehler_nachher"] = nahtfehler(grau_k, besitzer_k)

    # Auf volle Auflösung: Besitzer (nächster Nachbar) und Verschiebungen (skaliert) übertragen, Karten verschieben.
    groesse = (hoehe, 2 * hoehe)
    besitzer = cv2.resize(besitzer_k, (groesse[1], groesse[0]), interpolation=cv2.INTER_NEAREST)
    faktor = hoehe / klein
    neue = []
    welt = richtungen(hoehe)
    for k, p in enumerate(positionen):
        mx, my, w = abbildung(p.kamera, p.lage, welt)
        if felder_k is not None:
            f = cv2.resize(felder_k[k], (groesse[1], groesse[0]), interpolation=cv2.INTER_LINEAR) * faktor
            gx, gy = np.meshgrid(np.arange(groesse[1], dtype=np.float32), np.arange(groesse[0], dtype=np.float32))
            sx, sy = gx + f[..., 0], gy + f[..., 1]
            mx = cv2.remap(mx, sx, sy, interpolation=cv2.INTER_LINEAR, borderMode=cv2.BORDER_WRAP)
            my = cv2.remap(my, sx, sy, interpolation=cv2.INTER_LINEAR, borderMode=cv2.BORDER_WRAP)
        neue.append((mx, my, w))
    bild, clip, abdeckung = projektion.panorama_multiband(positionen, hoehe, stufen, karten=neue, besitzer=besitzer)
    return bild, clip, abdeckung, bericht


def _verschieben(bild: np.ndarray, feld: np.ndarray) -> np.ndarray:
    h, w = bild.shape[:2]
    gx, gy = np.meshgrid(np.arange(w, dtype=np.float32), np.arange(h, dtype=np.float32))
    return cv2.remap(bild, gx + feld[..., 0], gy + feld[..., 1], interpolation=cv2.INTER_LINEAR, borderMode=cv2.BORDER_WRAP)


def nahtfehler(grau: list[np.ndarray], besitzer: np.ndarray) -> float:
    """Mittlerer Helligkeitsunterschied (8-bit-Log-Stufen) zwischen den zwei Bildern direkt an den Nähten.
    Misst, wie sichtbar Sprünge an den Nähten sind (0 = nahtlos)."""
    g = np.stack(grau).astype(np.float32)
    werte = []
    for a, b in [(besitzer[:, :-1], besitzer[:, 1:]), (besitzer[:-1, :], besitzer[1:, :])]:
        naht = (a != b) & (a >= 0) & (b >= 0)
        ys, xs = np.nonzero(naht)
        if len(ys):
            werte.append(np.abs(g[a[ys, xs], ys, xs] - g[b[ys, xs], ys, xs]))
    return float(np.concatenate(werte).mean()) if werte else 0.0
