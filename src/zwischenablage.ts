// Werte aus Tabellen in die Zwischenablage (Marlon, 03.11.2026: „Die GPS-Daten lassen sich nicht kopieren … auf den
// Wert doppelklicken“; „Werte kopieren sollte man bei allen können“). Ein Doppelklick auf eine Zelle mit Text kopiert
// ihren Wert, in jeder Tabelle. Eine Zelle, die einen Knopf, ein Bild oder nur „–“ trägt, bleibt beim bisherigen
// Verhalten (z. B. Take abspielen). `data-kopie` an der Zelle liefert einen genaueren Wert als die Anzeige (GPS).

export async function kopieren(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // Rückfall unten.
  }
  try {
    const t = document.createElement("textarea");
    t.value = text;
    t.setAttribute("readonly", "");
    t.style.position = "fixed";
    t.style.opacity = "0";
    document.body.appendChild(t);
    t.select();
    const ok = document.execCommand("copy");
    t.remove();
    return ok;
  } catch {
    return false;
  }
}

/** Was ein Doppelklick auf diese Zelle kopieren würde; `undefined`, wenn nichts. */
export function zellenWert(ziel: EventTarget | null): string | undefined {
  if (!(ziel instanceof Element)) return undefined;
  const zelle = ziel.closest("td");
  if (!zelle || !zelle.closest("table")) return undefined;
  // Dateiliste im Protokoll: Doppelklick öffnet wie im Finder; der Pfad ist dort über Rechtsklick kopierbar.
  if (zelle.closest("table.finder")) return undefined;
  if (ziel.closest("button, a, input, select, textarea, img, figure, [role=button]")) return undefined;
  const genau = zelle.dataset.kopie;
  if (genau) return genau;
  const text = zelle.innerText.replace(/\s+/g, " ").trim();
  if (!text || text === "–" || text === "-") return undefined;
  return text;
}

/**
 * Hängt den Doppelklick zum Kopieren an das Dokument (vor den Handlern der Zeilen). `melden` zeigt den kopierten Wert.
 * Gibt die Abmeldefunktion zurück.
 */
export function zellenKopierenEinrichten(melden: (text: string) => void): () => void {
  const h = (ev: MouseEvent) => {
    const wert = zellenWert(ev.target);
    if (wert === undefined) return;
    ev.stopPropagation();
    ev.preventDefault();
    void kopieren(wert).then((ok) => melden(ok ? wert : ""));
  };
  document.addEventListener("dblclick", h, true);
  return () => document.removeEventListener("dblclick", h, true);
}
