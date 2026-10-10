// Der Auftrag (Karte, Dreh) und der laufende Vorgang. Liegt über den Seiten, damit ein Seitenwechsel nichts verliert
// und der Kopf überall zeigt, was gerade läuft.
import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { message } from "@tauri-apps/plugin-dialog";
import {
  abbrechen as kernAbbrechen,
  aufFortschritt,
  karteEinlesen,
  kartenziele,
  sollVonStage,
  vorabPruefen,
  zielNachpruefen,
  einlesenVorschau,
  zieleStand,
  kopieAusKopie,
  type KaskadenErgebnis,
  type EinlesenVorschau,
  type ZielStand,
  type Befund,
  type Dreh,
  type Fortschritt,
  type KartenErgebnis,
  type Nachpruefung,
  type SollClip,
} from "./kern";
import { gemerkt, merken, useEinstellungen } from "./einstellungen";
import { drehsVon, plateSoll, studioSoll, useKonto, type DrehKurz, type Projekt } from "./konto";

export type Phase = "bereit" | "kopieren" | "pruefen" | "nachlesen" | "nachpruefen" | "fertig" | "fehler";

export type Stand = {
  dateien: number;
  bytes: number;
  gelesen: number;
  datei: string;
  dateiNummer: number;
  pruefZiel: number;
  pruefPfad: string;
  pruefNummer: number;
  /** Beginn des Zurücklesens des aktuellen Ziels (für Tempo und Restzeit pro Ziel). */
  pruefBeginn: number;
  beginn: number;
  ausfaelle: { ordner: string; fehler: string }[];
  /** Zahl der Ziele beim Start (die Auswahl kann sich danach ändern). */
  zielZahl: number;
  /** Zurücklesen pro Ziel (Ziele auf verschiedenen Platten laufen gleichzeitig): fertige Dateien, Bytes, Beginn. */
  pruefJeZiel: Record<number, { nummer: number; bytes: number; beginn: number }>;
  /** Dateien in der Reihenfolge des Kopierens; `geprueft` = Bitmaske der Ziele, die sie schon zurückgelesen haben
   *  (Bit i = Ziel i). */
  liste: { pfad: string; groesse: number; geprueft: number }[];
};

const LEER: Stand = {
  dateien: 0,
  bytes: 0,
  gelesen: 0,
  datei: "",
  dateiNummer: 0,
  pruefZiel: 0,
  pruefPfad: "",
  pruefNummer: 0,
  pruefBeginn: 0,
  beginn: 0,
  ausfaelle: [],
  zielZahl: 0,
  pruefJeZiel: {},
  liste: [],
};

/** Quelle: Pfad, und wenn aus der Liste der eingesteckten Laufwerke gewählt, dessen Angaben (für Auswerfen). */
export type Quelle = { pfad: string; name: string; laufwerk: boolean; kamera?: string; clips?: number; bytes?: number };

export const LAUFEND: Phase[] = ["kopieren", "pruefen", "nachlesen", "nachpruefen"];

function useLaufHalten() {
  const e = useEinstellungen();
  const konto = useKonto();

  const [quelle, setQuelleRoh] = useState<Quelle | null>(null);
  const [phase, setPhase] = useState<Phase>("bereit");
  const [stand, setStand] = useState<Stand>(LEER);
  const [ergebnis, setErgebnis] = useState<KartenErgebnis | null>(null);
  /** Karte des letzten Ergebnisses: zum Auswerfen nach „Sicher zum Formatieren“. */
  const [letzteQuelle, setLetzteQuelle] = useState<Quelle | null>(null);
  const [fehler, setFehler] = useState<string | null>(null);
  const [nachpruefung, setNachpruefung] = useState<Nachpruefung | null>(null);
  const [nachpruefFehler, setNachpruefFehler] = useState<{ ordner: string; text: string } | null>(null);
  const laeuft = LAUFEND.includes(phase);
  const puffer = useRef<Fortschritt[]>([]);
  const zeitgeber = useRef<ReturnType<typeof setTimeout> | null>(null);
  const kaskadeAktiv = useRef(false);

  // Dreh: Projekt (aus dem Plate Assistant gewählt oder getippt), Datum, Drehort.
  const heute = new Date().toLocaleDateString("sv-SE");
  const [projektText, setProjektText] = useState<string>(() => gemerkt("projekt", ""));
  const [paProjekt, setPaProjekt] = useState<Projekt | null>(null);
  const [paDreh, setPaDreh] = useState<DrehKurz | null>(null);
  // Das gewählte Projekt gilt für alle Seiten und überlebt Seitenwechsel und Neustart (gemerkt wird die ID).
  useEffect(() => {
    if (paProjekt) merken("paProjekt", paProjekt.id);
    e.setAktivesProjekt(paProjekt?.id ?? null, paProjekt?.kopien ?? null);
  }, [paProjekt?.id, paProjekt?.kopien]);
  // Eine geänderte Kopienzahl gilt für das Projekt in allen drei Apps (projekt.kopien, Migration 0025).
  useEffect(() => {
    const p = paProjekt;
    e.kopienSpeichern.current =
      p && konto.verbindung === "verbunden"
        ? (n: number) =>
            konto.projektAendern(p.id, { kopien: n }).catch((err) =>
              message(`Kopienzahl nicht im Projekt gespeichert (gilt nur auf diesem Rechner):\n${String(err)}`, {
                title: "Kopien vor der Freigabe",
                kind: "warning",
              }).catch(() => {}),
            )
        : null;
  }, [paProjekt?.id, konto.verbindung]);
  useEffect(() => {
    const id = paProjekt?.id ?? gemerkt("paProjekt", "");
    const p = id ? konto.projekte.find((x) => x.id === id) : undefined;
    // Nach dem Laden der Liste (oder dem Speichern der Einstellungen) den aktuellen Stand übernehmen.
    if (p && p !== paProjekt) {
      setPaProjekt(p);
      if (!paProjekt) setProjektText(p.name);
    }
  }, [konto.projekte]);
  /** Projekt wählen (oben links): setzt auch den Projektnamen der Ablage und vergisst den Drehort. */
  function projektWaehlen(p: Projekt | null) {
    setPaProjekt(p);
    setProjektText(p?.name ?? "");
    setPaDreh(null);
    if (!p) merken("paProjekt", "");
  }
  // Der Drehort wird nur am selben Tag übernommen: sonst landet die Karte von heute still im Ordner von gestern.
  const [drehName, setDrehName] = useState<string>(() => (gemerkt("drehTag", "") === heute ? gemerkt("drehName", "") : ""));
  const [drehDatum, setDrehDatum] = useState(heute);
  useEffect(() => merken("projekt", projektText), [projektText]);
  useEffect(() => {
    merken("drehName", drehName);
    merken("drehTag", heute);
  }, [drehName, heute]);

  // Mit Projekt aus dem Plate Assistant: kein Drehort von Hand. Vor dem Kopieren ordnet die App die Clips der Karte
  // den Drehorten zu; die Karte kommt zum Drehort mit den meisten Clips, ohne Treffer nach <Datum>_OHNE_DREHORT.
  const [vorschau, setVorschau] = useState<EinlesenVorschau | null>(null);
  const [vorschauLaedt, setVorschauLaedt] = useState(false);
  const automatisch = konto.verbindung === "verbunden" && !!paProjekt;
  async function vorschauLaden() {
    if (!quelle || !paProjekt || konto.verbindung !== "verbunden") {
      setVorschau(null);
      return;
    }
    setVorschauLaedt(true);
    try {
      await konto.laden(); // neue Drehorte und Takes vom iPhone
      setVorschau(await einlesenVorschau(konto.zugang, paProjekt, quelle.pfad));
    } catch {
      setVorschau(null);
    }
    setVorschauLaedt(false);
  }
  useEffect(() => {
    vorschauLaden();
  }, [quelle?.pfad, paProjekt?.id, konto.verbindung]);
  const haupt = vorschau?.drehorte[0] ?? null;

  const dreh: Dreh | null = automatisch
    ? vorschau
      ? {
          projekt: paProjekt!.name,
          kurzname: paProjekt!.kurzname,
          datum: haupt?.datum || vorschau.aufnahmetag || heute,
          name: haupt?.name ?? "Ohne Drehort",
          ortKurzname: vorschau.ordner,
        }
      : null
    : projektText.trim() && drehName.trim()
      ? {
          projekt: projektText.trim(),
          kurzname: paProjekt?.kurzname ?? null,
          datum: drehDatum,
          name: drehName.trim(),
          ortKurzname: paDreh?.kurzname ?? null,
        }
      : null;
  const drehSchluessel = dreh ? `${dreh.projekt}|${dreh.kurzname}|${dreh.datum}|${dreh.name}|${dreh.ortKurzname}` : "";

  // Soll-Liste der Stage (im Studio).
  const [soll, setSoll] = useState<{ liste: SollClip[]; fehler: string | null; zeit: number } | null>(null);
  async function sollLaden(): Promise<SollClip[]> {
    const adresse = e.stageAdresse.trim();
    if (!adresse) {
      setSoll(null);
      return [];
    }
    try {
      const liste = await sollVonStage(adresse);
      setSoll({ liste, fehler: null, zeit: Date.now() });
      // Im Studio: aktives Filmprojekt der Stage übernehmen, wenn noch keines eingetragen ist.
      if (!projektText.trim()) {
        const p = await invoke<{ id: string; name: string; kurzname: string } | null>("stage_projekt", { adresse }).catch(
          () => null,
        );
        if (p) {
          setProjektText(p.name);
          setPaProjekt({ id: p.id, name: p.name, kurzname: p.kurzname, aktiv: true });
        }
      }
      return liste;
    } catch (err) {
      setSoll({ liste: [], fehler: String(err), zeit: Date.now() });
      return [];
    }
  }

  // Kartenziele berechnet der Kern (Kartenname bei Laufwerkswurzel, taugliche Ordnernamen).
  const [ziele, setZiele] = useState<string[]>([]);
  const basisSchluessel = e.ziele.join("|");
  useEffect(() => {
    if (!quelle || e.ziele.length === 0) {
      setZiele([]);
      return;
    }
    let aktuell = true;
    kartenziele(quelle.pfad, e.ziele, dreh)
      .then((z) => aktuell && setZiele(z))
      .catch(() => aktuell && setZiele(e.ziele.map((z) => z.replace(/[\\/]+$/, "") + (z.includes("\\") ? "\\" : "/") + quelle.name)));
    return () => {
      aktuell = false;
    };
  }, [quelle?.pfad, basisSchluessel, drehSchluessel]);

  // Zustand jedes Kartenziels: neu, früher vollständig kopiert (nur nachprüfen) oder abweichend (zur Seite legen).
  const [zielStaende, setZielStaende] = useState<ZielStand[]>([]);
  const [zurSeite, setZurSeite] = useState<string[]>([]);
  const [fortsetzen, setFortsetzen] = useState<string[]>([]);
  const [standZaehler, setStandZaehler] = useState(0);
  useEffect(() => {
    setZurSeite([]);
    setFortsetzen([]);
  }, [quelle?.pfad]);
  useEffect(() => {
    if (!quelle || ziele.length === 0 || laeuft) {
      setZielStaende([]);
      return;
    }
    let aktuell = true;
    zieleStand(quelle.pfad, ziele)
      .then((st) => aktuell && setZielStaende(st))
      .catch(() => aktuell && setZielStaende([]));
    return () => {
      aktuell = false;
    };
  }, [quelle?.pfad, ziele.join("|"), laeuft, standZaehler]);
  // Zur Seite legen und Fortsetzen schliessen sich aus: wer das eine wählt, nimmt das andere zurück.
  function zurSeiteUmschalten(ziel: string) {
    setFortsetzen((f) => f.filter((x) => x !== ziel));
    setZurSeite((z) => (z.includes(ziel) ? z.filter((x) => x !== ziel) : [...z, ziel]));
  }
  function fortsetzenUmschalten(ziel: string) {
    setZurSeite((z) => z.filter((x) => x !== ziel));
    setFortsetzen((f) => (f.includes(ziel) ? f.filter((x) => x !== ziel) : [...f, ziel]));
  }

  // Vorab-Prüfung bei jeder Änderung von Karte, Zielen oder Einstellungen.
  const [befunde, setBefunde] = useState<Befund[]>([]);
  const zieleSchluessel = ziele.join("|");
  useEffect(() => {
    if (!quelle || ziele.length === 0 || laeuft) {
      setBefunde([]);
      return;
    }
    let aktuell = true;
    vorabPruefen({ quelle: quelle.pfad, ziele, mitMd5: e.mitMd5, mindestKopien: e.mindestKopien, dreh, zurSeite, fortsetzen })
      .then((b) => aktuell && setBefunde(b))
      .catch((err) => aktuell && setBefunde([{ stufe: "fehler", text: String(err) }]));
    return () => {
      aktuell = false;
    };
  }, [quelle?.pfad, zieleSchluessel, e.mitMd5, e.mindestKopien, laeuft, zurSeite.join("|"), fortsetzen.join("|"), standZaehler]);

  useEffect(() => {
    // Phase ausserhalb des Zustands-Updaters setzen: React darf Updater mehrfach und spät ausführen, ein setPhase
    // darin könnte „prüft“ nach dem fertigen Ergebnis wieder setzen.
    const weg = aufFortschritt((f: Fortschritt) => {
      // Bei „Kopie aus Kopie“ bleibt die Phase „nachpruefen“ (Kopf und Einlesen-Seite zeigen keinen Kartenlauf).
      if (f.phase !== "kopieren" && !kaskadeAktiv.current) setPhase(f.phase);
      // Gesammelt und höchstens alle 100 ms angewendet: eine Karte mit Zehntausenden Dateien schickt sonst für jede
      // Datei ein Ereignis, und jedes kopierte die ganze Liste (Code-Prüfung 09.10.2026).
      puffer.current.push(f);
      if (!zeitgeber.current)
        zeitgeber.current = setTimeout(() => {
          zeitgeber.current = null;
          const ereignisse = puffer.current;
          puffer.current = [];
          setStand((s) => fortschrittAnwenden(s, ereignisse));
        }, 100);
    });
    return () => {
      weg.then((f) => f());
    };
  }, []);

  function quelleWaehlen(q: Quelle | null) {
    setQuelleRoh(q);
    if (q) {
      setErgebnis(null);
      setFehler(null);
      setPhase("bereit");
    }
  }

  /** `wenigerBestaetigt`: der Benutzer hat bestätigt, mit weniger Zielen als verlangten Kopien zu starten. */
  async function einlesen(wenigerBestaetigt = false) {
    if (!quelle || ziele.length === 0) return;
    setFehler(null);
    setErgebnis(null);
    setNachpruefung(null);
    puffer.current = [];
    setStand({ ...LEER, beginn: Date.now(), zielZahl: ziele.length });
    setPhase("kopieren");
    try {
      // Soll-Liste frisch holen; ist die Stage nicht erreichbar, wird trotzdem kopiert.
      const sollStage = await sollLaden();
      // Draussen: Takes des gewählten Drehorts aus dem Plate Assistant (fehlt der Zugang, nur ein Hinweis).
      let sollPlate: SollClip[] = [];
      // Takes aller Drehorte des Projekts (eine Karte kann mehrere Drehorte haben); doppelte Takes nur einmal.
      const drehIds = automatisch
        ? [...new Set([...(vorschau?.drehorte.map((d) => d.id) ?? []), ...drehsVon(konto.drehs, paProjekt).map((d) => d.id)])]
        : paDreh
          ? [paDreh.id]
          : [];
      for (const id of drehIds) {
        try {
          for (const s of await plateSoll(konto.zugang, id)) if (!sollPlate.some((x) => x.takeId === s.takeId)) sollPlate.push(s);
        } catch (err) {
          setSoll({ liste: sollStage, fehler: `Plate Assistant: ${err}`, zeit: Date.now() });
        }
      }
      // Studio: Takes aus `studio_take` (Entscheid „Datenfluss“). Gibt es sie, ersetzen sie die CSV der Stage, die
      // dieselben Takes ohne ID trägt; sonst bleibt die CSV der Rückfall.
      let sollStudio: SollClip[] = [];
      if (paProjekt) {
        try {
          sollStudio = await studioSoll(konto.zugang, paProjekt.id);
        } catch {
          sollStudio = [];
        }
      }
      const r = await karteEinlesen({
        quelle: quelle.pfad,
        ziele,
        mitMd5: e.mitMd5,
        mindestKopien: e.mindestKopien,
        zweimalLesen: e.zweimalLesen,
        soll: [...(sollStudio.length ? sollStudio : sollStage), ...sollPlate],
        dreh,
        artCmd: e.artCmd.trim() || null,
        stageAdresse: e.stageAdresse.trim() || null,
        plateZugang: paDreh || paProjekt ? konto.zugang : null,
        plateDreh: automatisch ? (haupt?.id ?? null) : (paDreh?.id ?? null),
        // Fest gewähltes Projekt: Karte und Clips gehen in die gemeinsame Datenbank (Tabellen karte/clip).
        plateProjekt: paProjekt ? { id: paProjekt.id, kurzname: paProjekt.kurzname } : null,
        kamera:
          paProjekt &&
          (paProjekt.fps || paProjekt.codec || paProjekt.aufloesungPx || paProjekt.aufnahmeGamma || paProjekt.look)
            ? {
                fps: paProjekt.fps ?? null,
                codec: paProjekt.codec ?? null,
                aufloesungPx: paProjekt.aufloesungPx ?? null,
                gamma: paProjekt.aufnahmeGamma || null,
                look: paProjekt.look || null,
              }
            : null,
        projektAngaben: paProjekt
          ? { firma: paProjekt.firma ?? null, regie: paProjekt.regie ?? null, dop: paProjekt.dop ?? null }
          : null,
        zurSeite: zurSeite.filter((z) => ziele.includes(z)),
        fortsetzen: fortsetzen.filter((z) => ziele.includes(z)),
        wenigerKopienBestaetigt: wenigerBestaetigt,
      });
      setErgebnis(r);
      setLetzteQuelle(quelle);
      setQuelleRoh(null); // nächste Karte: nie aus Versehen dieselbe nochmals
      setPhase("fertig");
    } catch (err) {
      setFehler(String(err));
      setPhase("fehler");
    }
  }

  async function nachpruefen(ordner: string) {
    // Eine Karte, die schon gewählt oder fertig ist, bleibt dabei unberührt: nur Phase und Stand werden geliehen.
    const vorher = phase;
    setNachpruefFehler(null);
    setNachpruefung(null);
    puffer.current = [];
    setStand({ ...LEER, beginn: Date.now() });
    setPhase("nachpruefen");
    try {
      setNachpruefung(await zielNachpruefen(ordner));
    } catch (err) {
      setNachpruefFehler({ ordner, text: String(err) });
    }
    setPhase(vorher === "fertig" || vorher === "fehler" ? vorher : "bereit");
  }

  // Kopie aus Kopie (Karte nicht mehr da): wie das Nachprüfen leiht es nur Phase und Stand.
  const [kaskade, setKaskade] = useState<KaskadenErgebnis | null>(null);
  const [kaskadeLaeuft, setKaskadeLaeuft] = useState(false);
  const [kaskadeFehler, setKaskadeFehler] = useState<string | null>(null);
  async function kopieAusKopieStarten(quelleKopie: string, zielBasis: string) {
    const vorher = phase;
    setKaskade(null);
    setKaskadeFehler(null);
    puffer.current = [];
    setStand({ ...LEER, beginn: Date.now() });
    setPhase("nachpruefen");
    kaskadeAktiv.current = true;
    setKaskadeLaeuft(true);
    try {
      setKaskade(
        await kopieAusKopie({
          quelle: quelleKopie,
          zielBasis,
          mindestKopien: e.mindestKopien,
          zugang: konto.verbindung === "verbunden" && paProjekt ? konto.zugang : null,
          projekt: paProjekt ? { id: paProjekt.id, kurzname: paProjekt.kurzname } : null,
        }),
      );
    } catch (err) {
      setKaskadeFehler(String(err));
    }
    kaskadeAktiv.current = false;
    setKaskadeLaeuft(false);
    setPhase(vorher === "fertig" || vorher === "fehler" ? vorher : "bereit");
  }

  // Abbrechen braucht einen zweiten Klick nach frühestens 0,5 s und innerhalb von 4 s (wie Überschreiben in der Stage).
  const [abbruchFragen, setAbbruchFragen] = useState(false);
  const abbruchZeit = useRef(0);
  function abbrechen() {
    const jetzt = Date.now();
    if (abbruchFragen && jetzt - abbruchZeit.current >= 500 && jetzt - abbruchZeit.current <= 4000) {
      kernAbbrechen();
      setAbbruchFragen(false);
      return;
    }
    abbruchZeit.current = jetzt;
    setAbbruchFragen(true);
    setTimeout(() => setAbbruchFragen(false), 4000);
  }

  return {
    quelle,
    quelleWaehlen,
    letzteQuelle,
    phase,
    stand,
    ergebnis,
    fehler,
    nachpruefung,
    nachpruefFehler,
    laeuft,
    ziele,
    befunde,
    sperrt: befunde.some((b) => b.stufe === "fehler"),
    dreh,
    projektText,
    setProjektText,
    paProjekt,
    setPaProjekt,
    projektWaehlen,
    paDreh,
    setPaDreh,
    automatisch,
    zielStaende,
    zurSeite,
    zurSeiteUmschalten,
    fortsetzen,
    fortsetzenUmschalten,
    staendeNeuLaden: () => setStandZaehler((n) => n + 1),
    vorschau,
    vorschauLaedt,
    vorschauLaden,
    drehName,
    setDrehName,
    drehDatum,
    setDrehDatum,
    soll,
    sollLaden,
    einlesen,
    nachpruefen,
    kaskade,
    kaskadeFehler,
    kaskadeLaeuft,
    kopieAusKopieStarten,
    abbrechen,
    abbruchFragen,
  };
}

export type Lauf = ReturnType<typeof useLaufHalten>;
const Kontext = createContext<Lauf | null>(null);

export function LaufGeben({ children }: { children: ReactNode }) {
  const l = useLaufHalten();
  return <Kontext.Provider value={l}>{children}</Kontext.Provider>;
}

export function useLauf() {
  const l = useContext(Kontext);
  if (!l) throw new Error("Lauf fehlt");
  return l;
}

/** Wendet gesammelte Fortschritts-Ereignisse in einem Durchgang an (Liste einmal kopiert, Suche über einen Index). */
function fortschrittAnwenden(alt: Stand, ereignisse: Fortschritt[]): Stand {
  const s: Stand = { ...alt, liste: alt.liste.slice(), pruefJeZiel: { ...alt.pruefJeZiel }, ausfaelle: alt.ausfaelle.slice() };
  const index = new Map<string, number>();
  s.liste.forEach((d, i) => index.set(d.pfad, i));
  for (const f of ereignisse) {
    if (f.phase === "pruefen" && !f.pfad) {
      // Leerer Pfad: dieses Ziel beginnt mit dem Zurücklesen (Zeit für Tempo und Restzeit).
      s.pruefJeZiel[f.ziel] = { nummer: 0, bytes: 0, beginn: Date.now() };
      s.pruefBeginn = s.pruefBeginn || Date.now();
      s.pruefZiel = f.ziel;
    } else if (f.phase === "pruefen") {
      // Gemeldet wird nach dem Prüfen einer Datei; Ziele verschiedener Platten melden durcheinander.
      const i = index.get(f.pfad);
      if (i !== undefined) s.liste[i] = { ...s.liste[i], geprueft: s.liste[i].geprueft | (1 << f.ziel) };
      const vorher = s.pruefJeZiel[f.ziel];
      s.pruefJeZiel[f.ziel] = {
        nummer: (vorher?.nummer ?? 0) + 1,
        bytes: (vorher?.bytes ?? 0) + (i !== undefined ? s.liste[i].groesse : 0),
        beginn: vorher?.beginn ?? Date.now(),
      };
      s.pruefNummer += 1;
      s.pruefBeginn = s.pruefBeginn || Date.now();
      s.pruefZiel = f.ziel;
      s.pruefPfad = f.pfad;
    } else if (f.phase === "nachlesen" || f.phase === "nachpruefen") {
      s.pruefPfad = f.pfad;
      s.pruefNummer += 1;
    } else {
      const m = f.meldung;
      switch (m.art) {
        case "begonnen":
          s.dateien = m.dateien;
          s.bytes = m.bytes;
          break;
        case "datei":
          s.datei = m.pfad;
          s.dateiNummer = m.nummer + 1;
          index.set(m.pfad, s.liste.length);
          s.liste.push({ pfad: m.pfad, groesse: m.groesse ?? 0, geprueft: 0 });
          break;
        case "bytes":
          s.gelesen = m.gelesen;
          break;
        case "zielAusgefallen":
          s.ausfaelle.push({ ordner: m.ordner, fehler: m.fehler });
          break;
      }
    }
  }
  return s;
}
