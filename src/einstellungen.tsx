// Einstellungen pro Rechner (Seite Einrichtung und Ziele beim Einlesen). Nur Bequemlichkeit: fehlt der Speicher,
// gilt der Standard. Die Freigabe-Schwelle wird nie unter 2 gemerkt: eine abgesenkte Schwelle gilt nur bis zum
// Neustart und nur nach Bestätigung.
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

export function gemerkt<T>(schluessel: string, standard: T): T {
  try {
    const wert = localStorage.getItem(`ingest.${schluessel}`);
    return wert === null ? standard : (JSON.parse(wert) as T);
  } catch {
    return standard;
  }
}

export function merken(schluessel: string, wert: unknown) {
  try {
    localStorage.setItem(`ingest.${schluessel}`, JSON.stringify(wert));
  } catch {
    // privat oder gesperrt: dann eben nicht
  }
}

function useGemerkt<T>(schluessel: string, standard: T) {
  const [wert, setWert] = useState<T>(() => gemerkt(schluessel, standard));
  useEffect(() => merken(schluessel, wert), [wert]);
  return [wert, setWert] as const;
}

export type Einstellungen = ReturnType<typeof useEinstellungenHalten>;

function useEinstellungenHalten() {
  const [ziele, setZiele] = useGemerkt<string[]>("ziele", []);
  const [mitMd5, setMitMd5] = useGemerkt("mitMd5", false);
  const [zweimalLesen, setZweimalLesen] = useGemerkt("zweimalLesen", false);
  // Kopien für die Freigabe: pro Projekt (Marlon, 08.10.2026: Standard 2, pro Projekt einstellbar, z. B. 3 für
  // Netflix). Ohne Projekt gilt der allgemeine Wert. Auch die Testschwelle 1 bleibt gemerkt; die Kopfleiste warnt.
  const [kopienAllgemein, setKopienAllgemein] = useState(() => Math.min(9, Math.max(1, gemerkt("mindestKopien", 2))));
  useEffect(() => merken("mindestKopien", kopienAllgemein), [kopienAllgemein]);
  const [kopienJeProjekt, setKopienJeProjekt] = useGemerkt<Record<string, number>>("kopienJeProjekt", {});
  const [aktivesProjekt, setAktivesProjekt] = useState<string | null>(null);
  const mindestKopien = (aktivesProjekt && kopienJeProjekt[aktivesProjekt]) || kopienAllgemein;
  const setMindestKopien = (n: number) => {
    const wert = Math.min(9, Math.max(1, n));
    if (aktivesProjekt) setKopienJeProjekt({ ...kopienJeProjekt, [aktivesProjekt]: wert });
    else setKopienAllgemein(wert);
  };
  const [stageAdresse, setStageAdresse] = useGemerkt("stageAdresse", "");
  const [artCmd, setArtCmd] = useGemerkt("artCmd", "");
  return {
    ziele,
    setZiele,
    mitMd5,
    setMitMd5,
    zweimalLesen,
    setZweimalLesen,
    mindestKopien,
    setMindestKopien,
    /** Projekt, für das `mindestKopien` gilt (setzt `lauf` beim Wählen oben links). */
    aktivesProjekt,
    setAktivesProjekt,
    stageAdresse,
    setStageAdresse,
    artCmd,
    setArtCmd,
  };
}

const Kontext = createContext<Einstellungen | null>(null);

export function EinstellungenGeben({ children }: { children: ReactNode }) {
  const e = useEinstellungenHalten();
  return <Kontext.Provider value={e}>{children}</Kontext.Provider>;
}

export function useEinstellungen() {
  const e = useContext(Kontext);
  if (!e) throw new Error("Einstellungen fehlen");
  return e;
}
