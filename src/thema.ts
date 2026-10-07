// Darstellung wie im Plate Assistant: Automatisch (folgt dem System, Standard), Tag, Nacht; gemerkt pro Rechner.
// Nacht ist die dunkle Welt der Stage, Tag der helle Satz für Tageslicht (stil.css). Gesetzt als data-thema auf <html>.
export type Wahl = "auto" | "tag" | "nacht";
export type Thema = "tag" | "nacht";

const SCHLUESSEL = "ingest.thema";
const system = () => window.matchMedia?.("(prefers-color-scheme: light)");

export function gemerkteWahl(): Wahl {
  try {
    const w = localStorage.getItem(SCHLUESSEL);
    if (w === "auto" || w === "tag" || w === "nacht") return w;
  } catch {
    // ohne Speicher: Automatisch
  }
  return "auto";
}

export function thema(wahl: Wahl): Thema {
  if (wahl !== "auto") return wahl;
  return system()?.matches ? "tag" : "nacht";
}

/** Setzt die Darstellung und folgt bei „Automatisch“ dem System. Gibt eine Abmeldefunktion zurück. */
export function anwenden(wahl: Wahl): () => void {
  document.documentElement.dataset.thema = thema(wahl);
  try {
    localStorage.setItem(SCHLUESSEL, wahl);
  } catch {
    // dann eben nur für diese Sitzung
  }
  const m = system();
  if (wahl !== "auto" || !m) return () => {};
  const folgen = () => (document.documentElement.dataset.thema = thema("auto"));
  m.addEventListener("change", folgen);
  return () => m.removeEventListener("change", folgen);
}
