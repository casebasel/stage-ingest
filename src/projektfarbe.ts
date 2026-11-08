// Projektfarben (Systemkarte DESIGN-GEMEINSAM, Marlon 08.11.2026): zwölf Farben aus Le Corbusiers Polychromie
// architecturale, Index = FNV-1a 32 Bit über die UTF-8-Bytes von `projekt.id` mod 12. Nur zum Wiedererkennen eines
// Projekts, in allen Apps dieselbe Farbe; als 3-px-Kante vorne an Zeile oder Pfad, immer mit dem Namen daneben,
// nie als Zustandsfarbe.

export const PROJEKTFARBEN: { code: string; name: string; hex: string }[] = [
  { code: "32020", name: "bleu outremer 31", hex: "#4E6498" },
  { code: "32030", name: "bleu céruléen 31", hex: "#366C8E" },
  { code: "32031", name: "céruléen vif", hex: "#5A8A93" },
  { code: "32040", name: "vert anglais", hex: "#2C695A" },
  { code: "4320G", name: "vert 59", hex: "#559778" },
  { code: "4320F", name: "vert olive vif", hex: "#A4AA35" },
  { code: "32141", name: "ombre naturelle moyenne", hex: "#877C6A" },
  { code: "32080", name: "orange", hex: "#DE6433" },
  { code: "32121", name: "terre sienne brique", hex: "#A86A5D" },
  { code: "4320C", name: "rose vif", hex: "#DE8DA0" },
  { code: "32101", name: "rouge rubia", hex: "#901442" },
  { code: "4320H", name: "gris 59", hex: "#616F74" },
];

/** FNV-1a, 32 Bit, über die UTF-8-Bytes. */
export function fnv1a32(text: string): number {
  let h = 0x811c9dc5;
  for (const b of new TextEncoder().encode(text)) {
    h ^= b;
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

export function projektfarbeIndex(projektId: string): number {
  return fnv1a32(projektId) % PROJEKTFARBEN.length;
}

/** Hex-Farbe eines Projekts; leer für Projekte ohne ID (nur Text, offline). */
export function projektfarbe(projektId: string | null | undefined): string {
  if (!projektId) return "";
  return PROJEKTFARBEN[projektfarbeIndex(projektId)].hex;
}

/** Inline-Stil für die 3-px-Kante (Klasse `projektkante`). */
export function projektkante(projektId: string | null | undefined): React.CSSProperties | undefined {
  const f = projektfarbe(projektId);
  return f ? ({ "--projektfarbe": f } as React.CSSProperties) : undefined;
}
