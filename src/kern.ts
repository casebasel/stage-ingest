// Typen und Aufrufe des Rust-Kerns (kern/, src-tauri/src/lib.rs). Felder wie dort serialisiert.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Pruefsumme = { xxh128: string; md5: string | null };

export type Datei = { pfad: string; groesse: number; geaendert: string; pruefsumme: Pruefsumme };
export type Ziel = { ordner: string; fehler: string | null; vorhanden?: boolean };
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
  clips: { pfad: string; angaben: { startTc: string | null; endTc: string | null; fps: number | null; bilder: number | null } | null; fehler: string | null }[];
  ale: (string | null)[];
  bewegung: [string, Bewegung][];
  stage: { Ok: Record<string, unknown> } | { Err: string } | null;
  /** Gemeinsame Datenbank (karte/clip): ID der Karte oder Fehler, null ohne Projekt aus dem Plate Assistant. */
  datenbank: { Ok: string } | { Err: string } | null;
  plates: { plates: number; fotosNeu: number; fehler: string[] } | null;
  abgleich: Abgleich | null;
  berichte: ({ Ok: string } | { Err: string })[];
  freigabe: Freigabe;
};

export type Meldung =
  | { art: "begonnen"; dateien: number; bytes: number }
  | { art: "datei"; nummer: number; pfad: string; groesse?: number }
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
  soll?: SollClip[];
  dreh?: Dreh | null;
  artCmd?: string | null;
  stageAdresse?: string | null;
  plateZugang?: { adresse: string; anonKey: string; email: string } | null;
  plateDreh?: string | null;
  plateProjekt?: { id: string; kurzname: string } | null;
  kamera?: { fps: number | null; codec: string | null; aufloesungPx: string | null } | null;
  projektAngaben?: { firma: string | null; regie: string | null; dop: string | null } | null;
  /** Bestehende, abweichende Zielordner zur Seite legen (umbenennen, nie löschen). */
  zurSeite?: string[];
  /** Bewusst mit weniger Zielen als verlangten Kopien gestartet. */
  wenigerKopienBestaetigt?: boolean;
};

export type SollClip = {
  clip: string;
  szene: string;
  take: string;
  startTc: string;
  endTc: string;
  bewertung: string;
  quelle: string;
  takeId?: string;
  startZeit?: string;
  fensterBis?: string;
  drehtag?: string;
};
export type Abgleich = {
  gefunden: [SollClip, string][];
  fehlt: SollClip[];
  ueberTimecode: [SollClip, string][];
  ueberZeitfenster: [SollClip, string][];
  mehrdeutig: [SollClip, string[]][];
  unerwartet: string[];
};
export const sollVonStage = (adresse: string) => invoke<SollClip[]>("soll_von_stage", { adresse });

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
  if (n < 1e12) return `${(n / 1e9).toFixed(n < 1e11 ? 2 : 1).replace(".", ",")} GB`;
  return `${(n / 1e12).toFixed(2).replace(".", ",")} TB`;
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

export type Dreh = { projekt: string; kurzname?: string | null; datum: string; name: string; ortKurzname?: string | null };
export const kartenziele = (quelle: string, basis: string[], dreh: Dreh | null) =>
  invoke<string[]>("kartenziele", { quelle, basis, dreh });

export type Werte = { mittel: number; min: number; max: number };
export type Bewegung = { bilder: number; tilt: Werte | null; roll: Werte | null; brennweiteMm: number | null };

export type Laufwerk = {
  pfad: string;
  name: string;
  gesamt: number | null;
  frei: number | null;
  netz: boolean;
  karte: { kamera: string; clips: number; bytes: number } | null;
};
export const laufwerke = () => invoke<Laufwerk[]>("laufwerke");
export const auswerfen = (pfad: string) => invoke<void>("auswerfen", { pfad });

export type ZielGeraet = {
  pfad: string;
  kennung: Kennung | null;
  gesamt: number | null;
  frei: number | null;
  fehler: string | null;
};
export const zielGeraete = (basis: string[]) => invoke<ZielGeraet[]>("ziel_geraete", { basis });

/** Vor dem Kopieren: Clips der Karte den Drehorten des Projekts zuordnen (Ordner = Drehort mit den meisten Clips). */
export type EinlesenVorschau = {
  drehorte: { id: string; name: string; datum: string; kurzname: string | null; clips: string[] }[];
  ohne: string[];
  gesamt: number;
  aufnahmetag: string | null;
  uhrFalsch: boolean;
  ordner: string | null;
};
export const einlesenVorschau = (zugang: unknown, projekt: unknown, quelle: string) =>
  invoke<EinlesenVorschau>("einlesen_vorschau", { zugang, projekt, quelle });

/** Zustand eines Zielordners vor dem Einlesen (kern/zielstand.rs). */
export type ZielStand = { art: "neu" } | { art: "vorhanden"; dateien: number } | { art: "abweichend"; grund: string };
export const zieleStand = (quelle: string, ziele: string[]) => invoke<ZielStand[]>("ziele_stand", { quelle, ziele });

/** Kopie aus Kopie (Kaskade): fehlende Kopie aus einer geprüften Kopie, geprüft gegen die Prüfsummen der Karte. */
export type KaskadenErgebnis = {
  quelle: string;
  ziel: string;
  urteile: Urteil[];
  kennungen: Kennung[];
  freigabe: Freigabe;
  bericht: string | null;
};
export const kopieAusKopie = (a: {
  quelle: string;
  zielBasis: string;
  mindestKopien: number;
  zugang: unknown;
  projekt: { id: string; kurzname: string } | null;
}) => invoke<KaskadenErgebnis>("kopie_aus_kopie", a);
