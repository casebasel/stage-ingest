// Tag/Nacht: Nacht ist die dunkle Welt der Stage, Tag ist für Tageslicht. Gewählt wird pro Rechner;
// beim ersten Start folgt die App dem System. Gesetzt als data-thema auf <html>, vor dem ersten Bild.
export type Thema = "tag" | "nacht";

const SCHLUESSEL = "ingest.thema";

export function gemerktesThema(): Thema {
  try {
    const t = localStorage.getItem(SCHLUESSEL);
    if (t === "tag" || t === "nacht") return t;
  } catch {
    // ohne Speicher: System
  }
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "tag" : "nacht";
}

export function themaSetzen(t: Thema) {
  document.documentElement.dataset.thema = t;
  try {
    localStorage.setItem(SCHLUESSEL, t);
  } catch {
    // dann eben nur für diese Sitzung
  }
}
