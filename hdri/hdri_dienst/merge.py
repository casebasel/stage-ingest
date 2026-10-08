"""Belichtungsreihe einer Position zu einem linearen HDR-Bild zusammenführen.

Die Rohdaten (Bayer-RAW, DNG) sind linear: Ein Pixelwert ist Licht mal Belichtung. Darum braucht es keine
Kennlinie wie beim klassischen Debevec-Verfahren mit JPEGs, sondern nur die Gewichtung nach Debevec und
Malik (1997): Jedes Bild schätzt die Strahldichte als Wert / Belichtung; gemittelt wird mit einem Gewicht, das
Rauschen (sehr dunkel) und Sättigung (sehr hell) ausschliesst. Massgeblich sind die tatsächlich verwendeten
Belichtungszeiten und ISO-Werte aus den Aufnahme-Metadaten, nie die verlangten.

Clip-Maske: Pixel, die selbst in der kürzesten Belichtung gesättigt sind. Dort ist der Wert nicht gemessen,
sondern nur eine Untergrenze; die KI-Stufe (später) ergänzt nur innerhalb dieser Maske.
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np

#: Ab hier gilt ein Wert (auf 0..1 normiert, 1 = Weisspunkt des Sensors) als gesättigt.
GESAETTIGT = 0.95
#: Darunter dominiert das Rauschen.
RAUSCHEN = 0.002


@dataclass
class Belichtung:
    """Ein Bild der Reihe: linear, float32 H×W×3, 0..1 (1 = Weisspunkt), mit tatsächlicher Belichtung."""

    bild: np.ndarray
    zeit_s: float
    iso: float

    @property
    def faktor(self) -> float:
        """Belichtung relativ zu ISO 100 und 1 s: Wert / faktor = Strahldichte (relativ)."""
        return self.zeit_s * self.iso / 100.0


def gewicht(wert: np.ndarray) -> np.ndarray:
    """Hutfunktion auf 0..1: null unter RAUSCHEN und über GESAETTIGT, dazwischen ein Dreieck."""
    mitte = (RAUSCHEN + GESAETTIGT) / 2
    breite = (GESAETTIGT - RAUSCHEN) / 2
    w = 1.0 - np.abs(wert - mitte) / breite
    return np.clip(w, 0.0, 1.0).astype(np.float32)


def zusammenfuehren(reihe: list[Belichtung]) -> tuple[np.ndarray, np.ndarray]:
    """Gibt (Strahldichte H×W×3 float32, Clip-Maske H×W bool) zurück."""
    if not reihe:
        raise ValueError("leere Belichtungsreihe")
    formen = {b.bild.shape for b in reihe}
    if len(formen) != 1:
        raise ValueError(f"Bilder der Reihe haben verschiedene Grössen: {formen}")
    kuerzeste = min(reihe, key=lambda b: b.faktor)

    summe = np.zeros_like(reihe[0].bild, dtype=np.float64)
    gewichte = np.zeros(reihe[0].bild.shape[:2], dtype=np.float64)
    for b in reihe:
        # Ein Gewicht pro Pixel aus dem hellsten Kanal: ist ein Kanal gesättigt, ist das ganze Pixel unzuverlässig.
        w = gewicht(b.bild.max(axis=2))
        summe += (b.bild / b.faktor) * w[..., None]
        gewichte += w

    # Wo kein Bild brauchbar war: zu hell (gesättigt auch kurz) oder zu dunkel (Rauschen auch lang).
    kurz = kuerzeste.bild.max(axis=2) >= GESAETTIGT
    leer = gewichte <= 1e-6
    ergebnis = np.where(gewichte[..., None] > 1e-6, summe / np.maximum(gewichte, 1e-6)[..., None], 0.0)
    # Gesättigt: Untergrenze aus der kürzesten Belichtung (die KI-Stufe ersetzt das später innerhalb der Maske).
    ergebnis = np.where((leer & kurz)[..., None], kuerzeste.bild / kuerzeste.faktor, ergebnis)
    # Zu dunkel: der Wert der längsten Belichtung, auch wenn verrauscht (besser als schwarz).
    laengste = max(reihe, key=lambda b: b.faktor)
    ergebnis = np.where((leer & ~kurz)[..., None], laengste.bild / laengste.faktor, ergebnis)
    return ergebnis.astype(np.float32), kurz
