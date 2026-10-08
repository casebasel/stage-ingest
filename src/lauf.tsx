// Der Auftrag (Karte, Dreh) und der laufende Vorgang. Liegt über den Seiten, damit ein Seitenwechsel nichts verliert
// und der Kopf überall zeigt, was gerade läuft.
import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  abbrechen as kernAbbrechen,
  aufFortschritt,
  karteEinlesen,
  kartenziele,
  sollVonStage,
  vorabPruefen,
  zielNachpruefen,
  einlesenVorschau,
  type EinlesenVorschau,
  type Befund,
  type Dreh,
  type Fortschritt,
  type KartenErgebnis,
  type Nachpruefung,
  type SollClip,
} from "./kern";
import { gemerkt, merken, useEinstellungen } from "./einstellungen";
import { drehsVon, plateSoll, useKonto, type DrehKurz, type Projekt } from "./konto";

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
  /** Dateien in der Reihenfolge des Kopierens; `geprueft` = Zahl der Ziele, die sie schon zurückgelesen haben
   *  (die Ziele werden nacheinander geprüft). */
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

  // Dreh: Projekt (aus dem Plate Assistant gewählt oder getippt), Datum, Drehort.
  const heute = new Date().toLocaleDateString("sv-SE");
  const [projektText, setProjektText] = useState<string>(() => gemerkt("projekt", ""));
  const [paProjekt, setPaProjekt] = useState<Projekt | null>(null);
  const [paDreh, setPaDreh] = useState<DrehKurz | null>(null);
  // Das gewählte Projekt gilt für alle Seiten und überlebt Seitenwechsel und Neustart (gemerkt wird die ID).
  useEffect(() => {
    if (paProjekt) merken("paProjekt", paProjekt.id);
  }, [paProjekt?.id]);
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

  // Vorab-Prüfung bei jeder Änderung von Karte, Zielen oder Einstellungen.
  const [befunde, setBefunde] = useState<Befund[]>([]);
  const zieleSchluessel = ziele.join("|");
  useEffect(() => {
    if (!quelle || ziele.length === 0 || laeuft) {
      setBefunde([]);
      return;
    }
    let aktuell = true;
    vorabPruefen({ quelle: quelle.pfad, ziele, mitMd5: e.mitMd5, mindestKopien: e.mindestKopien, dreh })
      .then((b) => aktuell && setBefunde(b))
      .catch((err) => aktuell && setBefunde([{ stufe: "fehler", text: String(err) }]));
    return () => {
      aktuell = false;
    };
  }, [quelle?.pfad, zieleSchluessel, e.mitMd5, e.mindestKopien, laeuft]);

  useEffect(() => {
    // Phase ausserhalb des Zustands-Updaters setzen: React darf Updater mehrfach und spät ausführen, ein setPhase
    // darin könnte „prüft“ nach dem fertigen Ergebnis wieder setzen.
    const weg = aufFortschritt((f: Fortschritt) => {
      if (f.phase !== "kopieren") setPhase(f.phase);
      setStand((s) => {
        if (f.phase === "pruefen")
          return {
            ...s,
            liste: s.liste.map((d) => (d.pfad === f.pfad ? { ...d, geprueft: Math.max(d.geprueft, f.ziel + 1) } : d)),
            pruefNummer: f.ziel === s.pruefZiel ? s.pruefNummer + 1 : 1,
            pruefBeginn: s.pruefNummer === 0 || f.ziel !== s.pruefZiel ? Date.now() : s.pruefBeginn,
            pruefZiel: f.ziel,
            pruefPfad: f.pfad,
          };
        if (f.phase === "nachlesen" || f.phase === "nachpruefen") return { ...s, pruefPfad: f.pfad, pruefNummer: s.pruefNummer + 1 };
        const m = f.meldung;
        switch (m.art) {
          case "begonnen":
            return { ...s, dateien: m.dateien, bytes: m.bytes };
          case "datei":
            return {
              ...s,
              datei: m.pfad,
              dateiNummer: m.nummer + 1,
              liste: [...s.liste, { pfad: m.pfad, groesse: m.groesse ?? 0, geprueft: 0 }],
            };
          case "bytes":
            return { ...s, gelesen: m.gelesen };
          case "zielAusgefallen":
            return { ...s, ausfaelle: [...s.ausfaelle, { ordner: m.ordner, fehler: m.fehler }] };
        }
      });
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

  async function einlesen() {
    if (!quelle || ziele.length === 0) return;
    setFehler(null);
    setErgebnis(null);
    setNachpruefung(null);
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
      const r = await karteEinlesen({
        quelle: quelle.pfad,
        ziele,
        mitMd5: e.mitMd5,
        mindestKopien: e.mindestKopien,
        zweimalLesen: e.zweimalLesen,
        soll: [...sollStage, ...sollPlate],
        dreh,
        artCmd: e.artCmd.trim() || null,
        stageAdresse: e.stageAdresse.trim() || null,
        plateZugang: paDreh || paProjekt ? konto.zugang : null,
        plateDreh: automatisch ? (haupt?.id ?? null) : (paDreh?.id ?? null),
        // Fest gewähltes Projekt: Karte und Clips gehen in die gemeinsame Datenbank (Tabellen karte/clip).
        plateProjekt: paProjekt ? { id: paProjekt.id, kurzname: paProjekt.kurzname } : null,
        kamera:
          paProjekt && (paProjekt.fps || paProjekt.codec || paProjekt.aufloesungPx)
            ? { fps: paProjekt.fps ?? null, codec: paProjekt.codec ?? null, aufloesungPx: paProjekt.aufloesungPx ?? null }
            : null,
        projektAngaben: paProjekt
          ? { firma: paProjekt.firma ?? null, regie: paProjekt.regie ?? null, dop: paProjekt.dop ?? null }
          : null,
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
    setStand({ ...LEER, beginn: Date.now() });
    setPhase("nachpruefen");
    try {
      setNachpruefung(await zielNachpruefen(ordner));
    } catch (err) {
      setNachpruefFehler({ ordner, text: String(err) });
    }
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
