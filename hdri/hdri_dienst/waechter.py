"""nDisplay-Wächter: Ada ist der nDisplay-Node der Stage. Der Dienst rechnet nur, wenn dort nichts von Unreal läuft;
startet es, wird ein laufender Job sofort abgebrochen und später neu begonnen (KONZEPT Kapitel 8). Keine
Schnittstelle zur Stage, nur die lokale Prozessliste.

Dazu das Einmessen von Edge Blending (Warp & Blend, Absprache 13.10.2026): Während Testbilder auf den Projektoren
stehen oder eine Prüfung läuft, pausiert der Dienst ebenfalls. Erkannt an der Befehlszeile von python(w).exe. Der
Dauerbetrieb von Edge Blending (`ada.py betrieb`, Sekunden) zählt nicht.
"""

from __future__ import annotations

import subprocess
import sys

#: Prozesse, bei denen Ada für die Stage arbeitet (Unreal mit nDisplay, Switchboard-Listener im Einsatz).
STAGE_PROZESSE = ("unrealeditor", "unrealeditor-cmd", "unrealgame", "displaycluster")


#: Befehlszeilen von Edge Blending im Messbetrieb (Testbild-Ausgabe, Masken- und Eckenwächter), klein, mit `/`.
#: Gestartet wird oft mit relativem Pfad (`pythonw.exe werkzeuge\anzeige.py`), darum ohne führenden Schrägstrich.
MESS_MERKMALE = ("werkzeuge/anzeige.py", "ada.py live", "ada.py nachstellen")


class StageAktiv(Exception):
    """nDisplay/Unreal läuft: sofort aufhören."""


def laufende_prozesse() -> list[str]:
    if sys.platform == "win32":
        aus = subprocess.run(["tasklist", "/fo", "csv", "/nh"], capture_output=True, text=True, timeout=30).stdout
        return [z.split('","')[0].strip('"').lower() for z in aus.splitlines() if z]
    aus = subprocess.run(["ps", "-eo", "comm="], capture_output=True, text=True, timeout=30).stdout
    return [z.strip().lower() for z in aus.splitlines()]


def stage_aktiv(prozesse: list[str] | None = None) -> str | None:
    """Name des Stage-Prozesses, wenn einer läuft, sonst None."""
    for p in prozesse if prozesse is not None else laufende_prozesse():
        name = p.removesuffix(".exe")
        if any(name.startswith(s) for s in STAGE_PROZESSE):
            return p
    return None


def python_befehlszeilen() -> list[str]:
    """Befehlszeilen aller laufenden python.exe/pythonw.exe (Windows) bzw. aller Prozesse (sonst)."""
    if sys.platform == "win32":
        befehl = (
            "Get-CimInstance Win32_Process -Filter \"Name='python.exe' or Name='pythonw.exe'\" "
            "| ForEach-Object { $_.CommandLine }"
        )
        aus = subprocess.run(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", befehl],
            capture_output=True,
            text=True,
            timeout=30,
        ).stdout
    else:
        aus = subprocess.run(["ps", "-eo", "args="], capture_output=True, text=True, timeout=30).stdout
    return [z.strip() for z in aus.splitlines() if z.strip()]


def messung_aktiv(befehlszeilen: list[str] | None = None) -> str | None:
    """Befehlszeile, wenn Edge Blending gerade einmisst oder prüft, sonst None."""
    for z in befehlszeilen if befehlszeilen is not None else python_befehlszeilen():
        klein = " ".join(z.lower().replace("\\", "/").split())
        if any(m in klein for m in MESS_MERKMALE):
            return z
    return None


def pruefen() -> None:
    """Wirft StageAktiv, wenn die Stage Ada braucht. Zwischen allen Arbeitsschritten aufrufen."""
    p = stage_aktiv()
    if p:
        raise StageAktiv(f"{p} läuft (nDisplay/Stage); der HDRI-Job pausiert")
    m = messung_aktiv()
    if m:
        raise StageAktiv("Edge Blending misst ein (Testbilder auf den Projektoren); der HDRI-Job pausiert")
