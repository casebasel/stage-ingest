// Kleine Bausteine, die alle Seiten teilen. Zustand nie nur über Farbe: immer Zeichen und Wort.
import type { ReactNode } from "react";
import { CircleAlert, CircleCheck, CircleDashed, CircleX, LoaderCircle } from "lucide-react";

export type Ton = "ok" | "warn" | "fehler" | "leise" | "laeuft";

const ZEICHEN = { ok: CircleCheck, warn: CircleAlert, fehler: CircleX, leise: CircleDashed, laeuft: LoaderCircle };

/** Zustand mit Zeichen und Wort, z. B. in Tabellen und im Kopf. */
export function Status({ ton, children, title }: { ton: Ton; children: ReactNode; title?: string }) {
  const Z = ZEICHEN[ton];
  return (
    <span className={`status status-${ton}`} title={title}>
      <Z size={14} strokeWidth={2} aria-hidden />
      <span>{children}</span>
    </span>
  );
}

/** Pfad, der von links gekürzt wird: das Ende (Karte, Ordner) bleibt sichtbar. */
export function Pfad({ pfad, className = "" }: { pfad: string; className?: string }) {
  return (
    <span className={`pfad ${className}`} title={pfad}>
      {pfad}
    </span>
  );
}

/** Eine Zeile der Einrichtung: Bezeichnung und Erklärung links, Bedienelement rechts. */
export function Feld({ name, hilfe, children }: { name: ReactNode; hilfe?: ReactNode; children: ReactNode }) {
  return (
    <div className="feld">
      <div className="feld-text">
        <span className="feld-name">{name}</span>
        {hilfe && <span className="feld-hilfe">{hilfe}</span>}
      </div>
      <div className="feld-wert">{children}</div>
    </div>
  );
}

export const name = (pfad: string) => pfad.split(/[\\/]/).filter(Boolean).pop() ?? pfad;

export const zahl = (n: number, stellen = 1) => n.toFixed(stellen).replace(".", ",");

export function dauerText(sekunden: number) {
  if (!isFinite(sekunden) || sekunden <= 0) return "–";
  const s = Math.round(sekunden);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${String(s % 60).padStart(2, "0")} s`;
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, "0")} min`;
}

export const datumZeit = (iso: string) =>
  new Date(iso).toLocaleString("de-CH", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
