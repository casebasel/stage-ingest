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
};
export type DrehKurz = { id: string; name: string; datum: string; projektId: string | null; produktion: string };

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
  const [zugang, setZugang] = useState<Zugang>(() => {
    const z = gemerkt("zugang", { adresse: "", anonKey: "", email: "" });
    return { adresse: z.adresse || VORGABE_ADRESSE, anonKey: z.anonKey || VORGABE_ANON, email: z.email };
  });
  const [konto, setKonto] = useState<{ email: string; ingestRecht: boolean } | null>(null);
  const [verbindung, setVerbindung] = useState<Verbindung>("aus");
  const [meldung, setMeldung] = useState<string | null>(null);
  const [projekte, setProjekte] = useState<Projekt[]>([]);
  const [drehs, setDrehs] = useState<DrehKurz[]>([]);
  const vollstaendig = !!(zugang.adresse.trim() && zugang.anonKey.trim() && zugang.email.trim());

  async function laden(z: Zugang = zugang) {
    try {
      const [pr, dr] = await Promise.all([
        invoke<Projekt[]>("plate_projekte", { zugang: z }),
        invoke<DrehKurz[]>("plate_drehs", { zugang: z }),
      ]);
      setProjekte(pr);
      setDrehs(dr);
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
    merken("zugang", zugang);
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

  const kurznameVorschlag = async (name: string) => (await invoke<string | null>("kurzname_vorschlag", { name })) ?? "";

  return {
    zugang,
    setZugang,
    vollstaendig,
    konto,
    verbindung,
    meldung,
    projekte,
    drehs,
    laden: () => laden(),
    anmelden,
    projektAnlegen,
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
