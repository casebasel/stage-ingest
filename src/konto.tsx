// Plate Assistant: Konto, Projekte und Drehorte aus der gemeinsamen Supabase (Vertrag plate-assistant docs/ABGLEICH.md).
// Anmeldung mit dem persönlichen Konto wie im iPhone. E-Mail bleibt auf diesem Rechner, das Passwort im Schlüsselbund.
// Löschen (HDRI-Rohdaten) und Meldungen nur mit dem Kennzeichen „ingest“ am Konto (setzt Marlon).
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SollClip } from "./kern";

export type Zugang = { adresse: string; anonKey: string; email: string };
export type Projekt = {
  id: string;
  name: string;
  kurzname: string;
  aktiv: boolean;
  /** Standard-Kameraeinstellungen (freiwillig): Abweichungen der Clips nur als Warnung. */
  fps?: number | null;
  codec?: string | null;
  aufloesungPx?: string | null;
  sensorFps?: number | null;
  sensorModus?: string | null;
  aufloesung?: string | null;
  /** Projekt-Einstellungen (Zahnrad, Migration 0016). */
  art?: string | null;
  firma?: string | null;
  regie?: string | null;
  dop?: string | null;
  /** Unabhängige Kopien vor der Freigabe (0025, 1..9; leer = 2). */
  kopien?: number | null;
  /** Farbe (0027): Vorgabe für Aufnahme-Gamma und Look, leer = keine. */
  aufnahmeGamma?: string | null;
  look?: string | null;
};

/** Arten laut Systemkarte; der Server prüft die Liste. */
export const ARTEN: [string, string][] = [
  ["", "–"],
  ["spielfilm", "Spielfilm"],
  ["serie", "Serie"],
  ["werbung", "Werbung"],
  ["musikvideo", "Musikvideo"],
  ["dokumentarfilm", "Dokumentarfilm"],
  ["test", "Studiotest / R&D"],
  ["sonstiges", "Sonstiges"],
];
export type DrehKurz = {
  id: string;
  name: string;
  datum: string;
  projektId: string | null;
  produktion: string;
  /** Fester Kurzname des Drehorts (ab Migration 0017): Ordnername. */
  kurzname?: string | null;
};

export const KURZNAME = /^[A-Z0-9]+(_[A-Z0-9]+)*$/;

function gemerkt<T>(k: string, s: T): T {
  try {
    const w = localStorage.getItem(`ingest.plate.${k}`);
    return w === null ? s : (JSON.parse(w) as T);
  } catch {
    return s;
  }
}
function merken(k: string, w: unknown) {
  try {
    localStorage.setItem(`ingest.plate.${k}`, JSON.stringify(w));
  } catch {
    // ohne Speicher: nur für diese Sitzung
  }
}

// Beim Bauen eingesetzt (GitHub-Variablen SUPABASE_ADRESSE, SUPABASE_ANON_KEY), nie im Repo. Fehlen sie,
// fragt die Einrichtung danach.
export const VORGABE_ADRESSE = (import.meta.env.VITE_SUPABASE_ADRESSE as string | undefined) ?? "";
export const VORGABE_ANON = (import.meta.env.VITE_SUPABASE_ANON_KEY as string | undefined) ?? "";

export type Verbindung = "aus" | "verbindet" | "verbunden" | "fehler";

function useKontoHalten() {
  // Adresse und Schlüssel: der eingebaute Satz gilt, ausser jemand hat unter „Andere Adresse …“ bewusst einen
  // eigenen gespeichert (eigene: true). Ältere Einträge ohne dieses Zeichen (z. B. von Hand in 0.1.5) werden
  // ignoriert, sonst würde ein alter, falscher Schlüssel den richtigen eingebauten für immer verdecken.
  const [zugang, setZugangRoh] = useState<Zugang>(() => {
    const z = gemerkt<Zugang & { eigene?: boolean }>("zugang", { adresse: "", anonKey: "", email: "" });
    const eigene = z.eigene === true || !VORGABE_ADRESSE || !VORGABE_ANON;
    return {
      adresse: (eigene && z.adresse) || VORGABE_ADRESSE,
      anonKey: (eigene && z.anonKey) || VORGABE_ANON,
      email: z.email,
    };
  });
  const [eigenerZugang, setEigenerZugang] = useState(
    () => gemerkt<{ eigene?: boolean }>("zugang", {}).eigene === true && !!VORGABE_ADRESSE && !!VORGABE_ANON,
  );
  /** Adresse/Schlüssel von Hand geändert: ab jetzt gilt der eigene Satz (bis „Eingebaute Adresse verwenden“). */
  function setZugang(z: Zugang) {
    if (z.adresse !== zugang.adresse || z.anonKey !== zugang.anonKey) setEigenerZugang(true);
    setZugangRoh(z);
  }
  function eingebautVerwenden() {
    setEigenerZugang(false);
    setZugangRoh({ adresse: VORGABE_ADRESSE, anonKey: VORGABE_ANON, email: zugang.email });
    merken("zugang", { email: zugang.email });
  }
  const [konto, setKonto] = useState<{ email: string; ingestRecht: boolean } | null>(null);
  const [verbindung, setVerbindung] = useState<Verbindung>("aus");
  const [meldung, setMeldung] = useState<string | null>(null);
  // Projekte und Drehorte samt festen Kurznamen werden gemerkt: ohne Netz heissen die Ordner trotzdem wie mit Netz
  // (Systemkarte „Datenfluss“, 10.10.2026; vorher kam der Name aus getipptem Text).
  const [projekte, setProjekte] = useState<Projekt[]>(() => gemerkt<Projekt[]>("vorrat.projekte", []));
  const [drehs, setDrehs] = useState<DrehKurz[]>(() => gemerkt<DrehKurz[]>("vorrat.drehs", []));
  const vollstaendig = !!(zugang.adresse.trim() && zugang.anonKey.trim() && zugang.email.trim());

  async function laden(z: Zugang = zugang) {
    try {
      const [pr, dr] = await Promise.all([
        invoke<Projekt[]>("plate_projekte", { zugang: z }),
        invoke<DrehKurz[]>("plate_drehs", { zugang: z }),
      ]);
      setProjekte(pr);
      setDrehs(dr);
      merken("vorrat.projekte", pr);
      merken("vorrat.drehs", dr);
      setVerbindung("verbunden");
      setMeldung(null);
    } catch (e) {
      setVerbindung("fehler");
      setMeldung(String(e));
    }
  }

  useEffect(() => {
    // Beim Start mit dem gemerkten Passwort aus dem Schlüsselbund anmelden (still, wenn keins da ist).
    if (!vollstaendig) return;
    setVerbindung("verbindet");
    invoke<{ email: string; ingestRecht: boolean }>("plate_anmelden", { zugang, passwort: "" })
      .then((k) => {
        setKonto(k);
        return laden();
      })
      .catch(() => setVerbindung("aus"));
  }, []);

  async function anmelden(passwort: string) {
    // Ohne eigenen Satz nur die E-Mail merken; Adresse und Schlüssel kommen dann immer aus dem Build.
    merken("zugang", eigenerZugang ? { ...zugang, eigene: true } : { email: zugang.email });
    setVerbindung("verbindet");
    setMeldung(null);
    try {
      setKonto(await invoke("plate_anmelden", { zugang, passwort }));
      await laden();
    } catch (e) {
      setVerbindung("fehler");
      setMeldung(String(e));
    }
  }

  async function projektAnlegen(name: string, kurzname: string): Promise<Projekt> {
    const id = await invoke<string>("plate_projekt_anlegen", { zugang, name, kurzname });
    await laden();
    return { id, name, kurzname, aktiv: true };
  }

  /** Projekt-Einstellungen speichern; nur geänderte Felder (Namen wie in der Datenbank). */
  async function projektAendern(id: string, felder: Record<string, string | number | null>) {
    await invoke("plate_projekt_aendern", { zugang, id, felder });
    await laden();
  }

  const kurznameVorschlag = async (name: string, laenge?: number) =>
    (await invoke<string | null>("kurzname_vorschlag", { name, laenge: laenge ?? null })) ?? "";

  async function abmelden() {
    try {
      await invoke("plate_abmelden", { zugang });
    } catch (e) {
      setMeldung(String(e));
      return;
    }
    setKonto(null);
    setProjekte([]);
    setDrehs([]);
    merken("vorrat.projekte", []);
    merken("vorrat.drehs", []);
    setVerbindung("aus");
    setMeldung(null);
  }

  /** Drehort anlegen (ab Migration 0017): gibt die neue ID zurück. */
  async function drehortAnlegen(projekt: Projekt, name: string, kurzname: string, datum: string): Promise<string> {
    const id = await invoke<string>("plate_drehort_anlegen", { zugang, projekt, name, kurzname, datum });
    await laden();
    return id;
  }

  return {
    zugang,
    setZugang,
    eigenerZugang,
    eingebautVerwenden,
    vollstaendig,
    konto,
    verbindung,
    meldung,
    projekte,
    drehs,
    laden: () => laden(),
    anmelden,
    abmelden,
    projektAnlegen,
    projektAendern,
    drehortAnlegen,
    kurznameVorschlag,
  };
}

export type Konto = ReturnType<typeof useKontoHalten>;
const Kontext = createContext<Konto | null>(null);

export function KontoGeben({ children }: { children: ReactNode }) {
  const k = useKontoHalten();
  return <Kontext.Provider value={k}>{children}</Kontext.Provider>;
}

export function useKonto() {
  const k = useContext(Kontext);
  if (!k) throw new Error("Konto fehlt");
  return k;
}

/** Drehorte eines Projekts; alte Drehorte haben nur den Projektnamen als Text. */
export const drehsVon = (drehs: DrehKurz[], projekt: Projekt | null) =>
  projekt ? drehs.filter((d) => d.projektId === projekt.id || (!d.projektId && d.produktion === projekt.name)) : drehs;

export const plateSoll = (zugang: Zugang, drehId: string) => invoke<SollClip[]>("plate_soll", { zugang, drehId });
