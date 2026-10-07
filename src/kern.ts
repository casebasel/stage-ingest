// Typen und Aufrufe des Rust-Kerns (kern/, src-tauri/src/lib.rs). Felder wie dort serialisiert.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Pruefsumme = { xxh128: string; md5: string | null };

export type Datei = { pfad: string; groesse: number; geaendert: string; pruefsumme: Pruefsumme };
export type Ziel = { ordner: string; fehler: string | null };
export type Kopie = { quelle: string; dateien: Datei[]; ordner: string[]; ziele: Ziel[]; beginn: string; ende: string };

export type Abweichung =
  | { art: "fehlt"; pfad: string }
  | { art: "groesse"; pfad: string; soll: number; ist: number }
  | { art: "pruefsumme"; pfad: string; soll: string; ist: string }
  | { art: "unlesbar"; pfad: string; fehler: string }
  | { art: "zusaetzlich"; pfad: string };

export type Urteil = { ordner: string; geprueft: number; abweichungen: Abweichung[]; kopierfehler: string | null };
export type Kennung = {
  wert: string;
  sicher: boolean;
  art: "platte" | "netz" | "volume";
  seriennummer: string | null;
  beschreibung: string;
};
export type Freigabe = {
  sicher: boolean;
  unabhaengige_kopien: number;
  mindest_kopien: number;
  kennung_unsicher: boolean;
  grund: string;
  hinweise: string[];
};
export type KartenErgebnis = {
  kopie: Kopie;
  urteile: Urteil[];
  kennungen: Kennung[];
  mhl: (string | null)[];
  berichte: ({ Ok: string } | { Err: string })[];
  freigabe: Freigabe;
};

export type Meldung =
  | { art: "begonnen"; dateien: number; bytes: number }
  | { art: "datei"; nummer: number; pfad: string }
  | { art: "bytes"; gelesen: number }
  | { art: "zielAusgefallen"; ordner: string; fehler: string };

export type Fortschritt =
  | { phase: "kopieren"; meldung: Meldung }
  | { phase: "pruefen"; ziel: number; pfad: string }
  | { phase: "nachlesen"; pfad: string }
  | { phase: "nachpruefen"; pfad: string };

export type KartenAuftrag = {
  quelle: string;
  ziele: string[];
  mitMd5: boolean;
  mindestKopien: number;
  zweimalLesen?: boolean;
};

export type Befund = { stufe: "fehler" | "warnung"; text: string };
export const vorabPruefen = (auftrag: KartenAuftrag) => invoke<Befund[]>("vorab_pruefen", { auftrag });
export const karteEinlesen = (auftrag: KartenAuftrag) => invoke<KartenErgebnis>("karte_einlesen", { auftrag });
export const abbrechen = () => invoke<void>("abbrechen");
export const aufFortschritt = (f: (p: Fortschritt) => void): Promise<UnlistenFn> =>
  listen<Fortschritt>("ingest://fortschritt", (e) => f(e.payload));

export const bytesText = (n: number) => {
  if (n < 1e3) return `${n} B`;
  if (n < 1e6) return `${(n / 1e3).toFixed(1).replace(".", ",")} kB`;
  if (n < 1e9) return `${(n / 1e6).toFixed(1).replace(".", ",")} MB`;
  return `${(n / 1e9).toFixed(2).replace(".", ",")} GB`;
};

export type VerlaufEintrag = {
  beginn: string;
  ende: string;
  karte: string;
  quelle: string;
  dateien: number;
  bytes: number;
  sicher: boolean;
  grund: string;
  ziele: { ordner: string; gut: boolean; bericht: string | null }[];
};
export const verlauf = () => invoke<VerlaufEintrag[]>("verlauf");

export type Nachpruefung = { ordner: string; generation: string; geprueft: number; abweichungen: Abweichung[] };
export const zielNachpruefen = (ordner: string) => invoke<Nachpruefung>("ziel_nachpruefen", { ordner });
