"""nDisplay-Wächter: Ada ist der nDisplay-Node der Stage. Der Dienst rechnet nur, wenn dort nichts von Unreal läuft;
startet es, wird ein laufender Job sofort abgebrochen und später neu begonnen (KONZEPT Kapitel 8). Keine
Schnittstelle zur Stage, nur die lokale Prozessliste.
"""

from __future__ import annotations

import subprocess
import sys

#: Prozesse, bei denen Ada für die Stage arbeitet (Unreal mit nDisplay, Switchboard-Listener im Einsatz).
STAGE_PROZESSE = ("unrealeditor", "unrealeditor-cmd", "unrealgame", "displaycluster")


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


def pruefen() -> None:
    """Wirft StageAktiv, wenn die Stage Ada braucht. Zwischen allen Arbeitsschritten aufrufen."""
    p = stage_aktiv()
    if p:
        raise StageAktiv(f"{p} läuft (nDisplay/Stage); der HDRI-Job pausiert")
