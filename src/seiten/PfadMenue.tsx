/** Rechtsklick auf eine Take-Zeile: alle Orte des Clips (NAS, Platten); Klick auf einen Pfad kopiert ihn
 *  (Marlon, 14.10.2026). Esc oder Klick daneben schliesst. */
import { useEffect, useState } from "react";
import { Copy, Film } from "lucide-react";
import { Status } from "../teile";

export type PfadMenueStand = { x: number; y: number; titel: string; dateien: string[] };

async function kopieren(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Rückfall ohne Clipboard-API
    const f = document.createElement("textarea");
    f.value = text;
    document.body.appendChild(f);
    f.select();
    document.execCommand("copy");
    f.remove();
  }
}

export function PfadMenue({ m, zu, exportieren }: { m: PfadMenueStand; zu: () => void; exportieren?: () => void }) {
  const [kopiert, setKopiert] = useState<string | null>(null);
  useEffect(() => {
    const taste = (ev: KeyboardEvent) => ev.key === "Escape" && zu();
    window.addEventListener("keydown", taste);
    window.addEventListener("blur", zu);
    return () => {
      window.removeEventListener("keydown", taste);
      window.removeEventListener("blur", zu);
    };
  }, []);
  // Im Fenster halten.
  const breite = 660;
  const left = Math.max(8, Math.min(m.x, window.innerWidth - breite - 8));
  const top = Math.max(8, Math.min(m.y, window.innerHeight - 60 - m.dateien.length * 34));
  return (
    <div className="pfad-menue-hinter" onMouseDown={zu} onContextMenu={(ev) => (ev.preventDefault(), zu())}>
      <div className="pfad-menue" role="menu" style={{ left, top, width: breite }} onMouseDown={(ev) => ev.stopPropagation()}>
        <div className="pfad-menue-titel">
          {m.titel} · Pfad kopieren{m.dateien.length > 1 ? ` (${m.dateien.length} Kopien)` : ""}
        </div>
        {m.dateien.length === 0 ? (
          <div className="pfad-menue-leer leise">Keine Kopie auf einer angeschlossenen Platte</div>
        ) : (
          m.dateien.map((d) => (
            <button
              key={d}
              role="menuitem"
              className="pfad-menue-eintrag"
              title="Klicken kopiert den Pfad"
              onClick={async () => {
                await kopieren(d);
                setKopiert(d);
                setTimeout(zu, 600);
              }}
            >
              <Copy size={13} aria-hidden />
              <span className="zahl">{d}</span>
              {kopiert === d && <Status ton="ok">Kopiert</Status>}
            </button>
          ))
        )}
        {exportieren && m.dateien.length > 0 && (
          <>
            <div className="pfad-menue-trenner" />
            <button
              role="menuitem"
              className="pfad-menue-eintrag"
              onClick={() => {
                zu();
                exportieren();
              }}
            >
              <Film size={13} aria-hidden />
              <span>Exportieren … (ProRes, MP4, EXR)</span>
            </button>
          </>
        )}
      </div>
    </div>
  );
}
