import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderInput, HardDrive, Moon, Plus, Sun, SunMoon, X } from "lucide-react";
import { anwenden, gemerkteWahl, type Wahl } from "./thema";
import {
  abbrechen,
  aufFortschritt,
  bytesText,
  karteEinlesen,
  kartenziele,
  sollVonStage,
  vorabPruefen,
  zielNachpruefen,
  type Befund,
  type Nachpruefung,
  type SollClip,
  type Dreh,
  type Fortschritt,
  type KartenErgebnis,
  type Abweichung,
} from "./kern";
import { Aktualisierung, useAktualisierung } from "./Aktualisierung";
import { Verlauf } from "./Verlauf";

type Phase = "bereit" | "kopieren" | "pruefen" | "nachlesen" | "nachpruefen" | "fertig" | "fehler";

type Stand = {
  dateien: number;
  bytes: number;
  gelesen: number;
  datei: string;
  pruefZiel: number;
  pruefPfad: string;
  beginn: number;
  ausfaelle: string[];
};

const LEERER_STAND: Stand = { dateien: 0, bytes: 0, gelesen: 0, datei: "", pruefZiel: 0, pruefPfad: "", beginn: 0, ausfaelle: [] };

// Ziele und Einstellungen pro Rechner merken. Nur Bequemlichkeit: fehlt der Speicher, gilt der Standard.
function gemerkt<T>(schluessel: string, standard: T): T {
  try {
    const wert = localStorage.getItem(`ingest.${schluessel}`);
    return wert === null ? standard : (JSON.parse(wert) as T);
  } catch {
    return standard;
  }
}
function merken(schluessel: string, wert: unknown) {
  try {
    localStorage.setItem(`ingest.${schluessel}`, JSON.stringify(wert));
  } catch {
    // privat oder gesperrt: dann eben nicht
  }
}

// Vorschau des Projekt-Kurznamens wie im Kern (struktur::kurzname): A–Z, 0–9, _.
const kurz = (p: string) =>
  p
    .trim()
    .replace(/[äÄ]/g, "AE")
    .replace(/[öÖ]/g, "OE")
    .replace(/[üÜ]/g, "UE")
    .replace(/ß/g, "SS")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "") || "OHNE_PROJEKT";

const trenner = (pfad: string) => (pfad.includes("\\") ? "\\" : "/");
const name = (pfad: string) => pfad.split(/[\\/]/).filter(Boolean).pop() ?? pfad;

export function App() {
  const [quelle, setQuelle] = useState<string | null>(null);
  const [zielOrdner, setZielOrdner] = useState<string[]>(() => gemerkt("ziele", []));
  const [mitMd5, setMitMd5] = useState<boolean>(() => gemerkt("mitMd5", false));
  const [mindestKopien, setMindestKopien] = useState<number>(() => gemerkt("mindestKopien", 2));
  useEffect(() => merken("ziele", zielOrdner), [zielOrdner]);
  useEffect(() => merken("mitMd5", mitMd5), [mitMd5]);
  useEffect(() => merken("mindestKopien", mindestKopien), [mindestKopien]);
  const [zweimalLesen, setZweimalLesen] = useState<boolean>(() => gemerkt("zweimalLesen", false));
  useEffect(() => merken("zweimalLesen", zweimalLesen), [zweimalLesen]);
  const [nachpruefung, setNachpruefung] = useState<Nachpruefung | null>(null);
  // Soll-Liste: Adresse des Stage-Servers pro Rechner (nie im Repo). Leer = ohne Soll-Liste.
  const [stageAdresse, setStageAdresse] = useState<string>(() => gemerkt("stageAdresse", ""));
  // Pfad zu ARRI ART CMD (lokal). Leer = keine Bewegungsdaten pro Clip.
  const [artCmd, setArtCmd] = useState<string>(() => gemerkt("artCmd", ""));
  useEffect(() => merken("artCmd", artCmd), [artCmd]);
  useEffect(() => merken("stageAdresse", stageAdresse), [stageAdresse]);
  const [soll, setSoll] = useState<{ liste: SollClip[]; fehler: string | null; zeit: number } | null>(null);
  async function sollLaden(): Promise<SollClip[]> {
    if (!stageAdresse.trim()) {
      setSoll(null);
      return [];
    }
    try {
      const liste = await sollVonStage(stageAdresse.trim());
      setSoll({ liste, fehler: null, zeit: Date.now() });
      return liste;
    } catch (e) {
      setSoll({ liste: [], fehler: String(e), zeit: Date.now() });
      return [];
    }
  }
  const [phase, setPhase] = useState<Phase>("bereit");
  const [stand, setStand] = useState<Stand>(LEERER_STAND);
  const [ergebnis, setErgebnis] = useState<KartenErgebnis | null>(null);
  const [fehler, setFehler] = useState<string | null>(null);
  const [abbruchFragen, setAbbruchFragen] = useState(false);
  const abbruchZeit = useRef(0);
  const { update, pflicht, version } = useAktualisierung();
  const [befunde, setBefunde] = useState<Befund[]>([]);

  useEffect(() => {
    const weg = aufFortschritt((f: Fortschritt) =>
      setStand((s) => {
        if (f.phase === "pruefen") {
          setPhase("pruefen");
          return { ...s, pruefZiel: f.ziel, pruefPfad: f.pfad };
        }
        if (f.phase === "nachlesen" || f.phase === "nachpruefen") {
          setPhase(f.phase);
          return { ...s, pruefPfad: f.pfad };
        }
        const m = f.meldung;
        switch (m.art) {
          case "begonnen":
            return { ...s, dateien: m.dateien, bytes: m.bytes };
          case "datei":
            return { ...s, datei: m.pfad };
          case "bytes":
            return { ...s, gelesen: m.gelesen };
          case "zielAusgefallen":
            return { ...s, ausfaelle: [...s.ausfaelle, `${m.ordner}: ${m.fehler}`] };
        }
      }),
    );
    return () => {
      weg.then((f) => f());
    };
  }, []);

  const laeuft = phase === "kopieren" || phase === "pruefen" || phase === "nachlesen" || phase === "nachpruefen";
  // Drehstruktur: mit Projekt und Dreh kommt jede Karte nach <Ziel>/<Projekt>/<Datum>_<Dreh>/01_KAMERA/<Karte>.
  // Projekt ist vorerst Text; es wird zur Auswahl aus der gemeinsamen Supabase (Paket casebasel/stage-projekt).
  const [projekt, setProjekt] = useState<string>(() => gemerkt("projekt", ""));
  const [drehName, setDrehName] = useState<string>(() => gemerkt("drehName", ""));
  const [drehDatum, setDrehDatum] = useState<string>(() => new Date().toLocaleDateString("sv-SE"));
  useEffect(() => merken("projekt", projekt), [projekt]);
  useEffect(() => merken("drehName", drehName), [drehName]);
  const dreh: Dreh | null =
    projekt.trim() && drehName.trim() ? { projekt: projekt.trim(), datum: drehDatum, name: drehName.trim() } : null;
  const drehSchluessel = dreh ? `${dreh.projekt}|${dreh.datum}|${dreh.name}` : "";

  // Die Kartenziele berechnet der Kern (Kartenname bei Laufwerkswurzel, taugliche Ordnernamen).
  const [ziele, setZiele] = useState<string[]>([]);
  const basisSchluessel = zielOrdner.join("|");
  useEffect(() => {
    if (!quelle || zielOrdner.length === 0) {
      setZiele([]);
      return;
    }
    kartenziele(quelle, zielOrdner, dreh)
      .then(setZiele)
      .catch(() => setZiele(zielOrdner.map((z) => z.replace(/[\\/]+$/, "") + trenner(z) + name(quelle))));
  }, [quelle, basisSchluessel, drehSchluessel]);

  // Vorab-Prüfung bei jeder Änderung von Karte, Zielen oder Einstellungen.
  const zieleSchluessel = ziele.join("|");
  useEffect(() => {
    if (!quelle || ziele.length === 0 || laeuft) {
      setBefunde([]);
      return;
    }
    let aktuell = true;
    vorabPruefen({ quelle, ziele, mitMd5, mindestKopien, dreh })
      .then((b) => aktuell && setBefunde(b))
      .catch((e) => aktuell && setBefunde([{ stufe: "fehler", text: String(e) }]));
    return () => {
      aktuell = false;
    };
  }, [quelle, zieleSchluessel, mitMd5, mindestKopien, laeuft]);
  const sperrt = befunde.some((b) => b.stufe === "fehler");

  async function karteWaehlen() {
    const pfad = await open({ directory: true, title: "Karte oder Reel-Ordner wählen" });
    if (typeof pfad === "string") {
      setQuelle(pfad);
      setErgebnis(null);
      setPhase("bereit");
    }
  }

  async function zielHinzufuegen() {
    const pfad = await open({ directory: true, title: "Zielordner wählen" });
    if (typeof pfad === "string" && !zielOrdner.includes(pfad)) setZielOrdner([...zielOrdner, pfad]);
  }

  async function einlesen() {
    if (!quelle || ziele.length === 0) return;
    setFehler(null);
    setErgebnis(null);
    setStand({ ...LEERER_STAND, beginn: Date.now() });
    setPhase("kopieren");
    try {
      setNachpruefung(null);
      // Soll-Liste frisch holen; ist die Stage nicht erreichbar, wird trotzdem kopiert (Hinweis links).
      const sollListe = await sollLaden();
      setErgebnis(await karteEinlesen({
          quelle,
          ziele,
          mitMd5,
          mindestKopien,
          zweimalLesen,
          soll: sollListe,
          dreh,
          artCmd: artCmd.trim() || null,
        }));
      setPhase("fertig");
    } catch (e) {
      setFehler(String(e));
      setPhase("fehler");
    }
    setAbbruchFragen(false);
  }

  async function nachpruefenKlick() {
    const ordner = await open({ directory: true, title: "Kopie mit ascmhl-Ordner wählen (z. B. A001R132)" });
    if (typeof ordner !== "string") return;
    setFehler(null);
    setErgebnis(null);
    setNachpruefung(null);
    setStand({ ...LEERER_STAND, beginn: Date.now() });
    setPhase("nachpruefen");
    try {
      setNachpruefung(await zielNachpruefen(ordner));
      setPhase("bereit");
    } catch (e) {
      setFehler(String(e));
      setPhase("fehler");
    }
    setAbbruchFragen(false);
  }

  // Abbrechen braucht einen zweiten Klick nach frühestens 0,5 s und innerhalb von 4 s (wie Überschreiben in der Stage).
  function abbrechenKlick() {
    const jetzt = Date.now();
    if (abbruchFragen && jetzt - abbruchZeit.current >= 500 && jetzt - abbruchZeit.current <= 4000) {
      abbrechen();
      setAbbruchFragen(false);
      return;
    }
    abbruchZeit.current = jetzt;
    setAbbruchFragen(true);
    setTimeout(() => setAbbruchFragen(false), 4000);
  }

  return (
    <div className={`ingest ${update ? "ingest-update" : ""}`}>
      <Aktualisierung update={update} pflicht={pflicht} laeuft={laeuft} />
      <header className="i-kopf">
        <div className="i-marke">
          Stage Ingest <span className="mono">{version}</span>
        </div>
        <div className="i-kopf-rechts">
          <Kopflampe phase={phase} ergebnis={ergebnis} />
          <ThemaSchalter />
        </div>
      </header>

      <div className="i-rumpf">
        <aside className="k-spalte">
          <section className="k-gruppe">
            <h2>Karte</h2>
            {quelle ? (
              <span className="i-pfad mono" title={quelle}>
                {quelle}
              </span>
            ) : (
              <span className="k-leise">Noch keine Karte gewählt</span>
            )}
            <div className="i-knopfreihe">
              <button className="k-taste" onClick={karteWaehlen} disabled={laeuft}>
                <FolderInput size={16} strokeWidth={1.75} /> Karte wählen
              </button>
            </div>
          </section>

          <section className="k-gruppe">
            <h2>Dreh</h2>
            <input
              className="i-eingabe"
              placeholder="Projekt"
              value={projekt}
              disabled={laeuft}
              onChange={(e) => setProjekt(e.target.value)}
            />
            <div className="i-zweier">
              <input
                className="i-eingabe mono"
                type="date"
                value={drehDatum}
                disabled={laeuft}
                onChange={(e) => setDrehDatum(e.target.value)}
              />
              <input
                className="i-eingabe"
                placeholder="Dreh (Ort)"
                value={drehName}
                disabled={laeuft}
                onChange={(e) => setDrehName(e.target.value)}
              />
            </div>
            <span className="k-leise k-klein">
              {dreh ? (
                <span className="mono">
                  {kurz(dreh.projekt)}/{dreh.datum}_{dreh.name}/01_KAMERA/…
                </span>
              ) : (
                "Ohne Projekt und Dreh kommt die Karte direkt in den Zielordner."
              )}
            </span>
          </section>

          <section className="k-gruppe">
            <h2>Ziele</h2>
            {zielOrdner.length === 0 && <span className="k-leise">Mindestens zwei Ziele auf verschiedenen Platten</span>}
            {zielOrdner.map((z) => (
              <div className="i-ziel" key={z}>
                <span className="i-pfad mono" title={z}>
                  {z}
                </span>
                <button
                  className="k-symbol"
                  aria-label="Ziel entfernen"
                  disabled={laeuft}
                  onClick={() => setZielOrdner(zielOrdner.filter((x) => x !== z))}
                >
                  <X size={16} strokeWidth={1.75} />
                </button>
              </div>
            ))}
            <div className="i-knopfreihe">
              <button className="k-taste" onClick={zielHinzufuegen} disabled={laeuft}>
                <Plus size={16} strokeWidth={1.75} /> Ziel hinzufügen
              </button>
            </div>
          </section>

          <section className="k-gruppe">
            <h2>Prüfung</h2>
            <label className="i-schalter">
              Unabhängige Kopien für die Freigabe
              <input
                className="i-zahl"
                type="number"
                min={1}
                max={9}
                value={mindestKopien}
                disabled={laeuft}
                onChange={(e) => setMindestKopien(Math.max(1, Number(e.target.value) || 1))}
              />
            </label>
            <label className="i-schalter">
              Zusätzlich MD5
              <input type="checkbox" checked={mitMd5} disabled={laeuft} onChange={(e) => setMitMd5(e.target.checked)} />
            </label>
            <label className="i-schalter" title="Erkennt einen Kartenleser, der unzuverlässig liefert. Dauert länger.">
              Karte zweimal lesen
              <input
                type="checkbox"
                checked={zweimalLesen}
                disabled={laeuft}
                onChange={(e) => setZweimalLesen(e.target.checked)}
              />
            </label>
            <div className="k-zeile">
              <span className="k-zeile-name">Prüfsumme</span>
              <span className="k-zeile-wert mono">XXH3-128{mitMd5 ? " + MD5" : ""}</span>
            </div>
            <div className="k-zeile">
              <span className="k-zeile-name">Zurücklesen</span>
              <span className="k-zeile-wert">jedes Ziel, ohne Cache</span>
            </div>
          </section>

          <section className="k-gruppe">
            <h2>Soll-Liste</h2>
            <span className="k-leise k-klein">Gedrehte Takes von der Stage; fehlende Clips werden vor dem Formatieren gemeldet.</span>
            <input
              className="i-eingabe mono"
              placeholder="http://stage-server:4400"
              value={stageAdresse}
              disabled={laeuft}
              onChange={(e) => setStageAdresse(e.target.value)}
              spellCheck={false}
            />
            <div className="i-knopfreihe">
              <button className="k-taste k-taste-klein" onClick={sollLaden} disabled={laeuft || !stageAdresse.trim()}>
                Laden
              </button>
              {soll && !soll.fehler && <span className="k-lampe k-lampe-ok"><i /> {soll.liste.length} Takes</span>}
              {soll?.fehler && <span className="k-lampe k-lampe-warn"><i /> nicht erreichbar</span>}
            </div>
          </section>

          <section className="k-gruppe">
            <h2>Bewegungsdaten</h2>
            <span className="k-leise k-klein">Neigung, Rollen und Brennweite pro Bild mit ARRI ART CMD (optional).</span>
            <input
              className="i-eingabe mono"
              placeholder="Pfad zu art-cmd"
              value={artCmd}
              disabled={laeuft}
              onChange={(e) => setArtCmd(e.target.value)}
              spellCheck={false}
            />
            <div className="i-knopfreihe">
              <button
                className="k-taste k-taste-klein"
                disabled={laeuft}
                onClick={async () => {
                  const p = await open({ directory: false, title: "ART CMD wählen" });
                  if (typeof p === "string") setArtCmd(p);
                }}
              >
                Wählen …
              </button>
            </div>
          </section>

          <section className="k-gruppe">
            <h2>Bestehende Kopie</h2>
            <span className="k-leise k-klein">Vollständig gegen ihr ASC MHL prüfen, jederzeit.</span>
            <div className="i-knopfreihe">
              <button className="k-taste" onClick={nachpruefenKlick} disabled={laeuft}>
                Ziel nachprüfen …
              </button>
            </div>
          </section>
        </aside>

        <main className="i-flaeche">
          <div className="i-raster">
            <Zustand
              phase={phase}
              stand={stand}
              ergebnis={ergebnis}
              fehler={fehler}
              ziele={ziele}
              bereit={!!quelle && ziele.length > 0 && !pflicht && !sperrt}
              befunde={befunde}
              einlesen={einlesen}
              abbrechen={abbrechenKlick}
              abbruchFragen={abbruchFragen}
            />
            {nachpruefung && <NachpruefungAnzeige n={nachpruefung} />}
            {ergebnis && <Ergebnis ergebnis={ergebnis} />}
            {!laeuft && <Verlauf neu={ergebnis?.kopie.beginn ?? ""} />}
          </div>
        </main>
      </div>
    </div>
  );
}

const WAHLEN: { wahl: Wahl; wort: string; Zeichen: typeof Sun }[] = [
  { wahl: "auto", wort: "Automatisch", Zeichen: SunMoon },
  { wahl: "tag", wort: "Tag", Zeichen: Sun },
  { wahl: "nacht", wort: "Nacht", Zeichen: Moon },
];

/** Drei Stufen wie im Plate Assistant; ein Klick schaltet weiter: Automatisch → Tag → Nacht. */
function ThemaSchalter() {
  const [wahl, setWahl] = useState<Wahl>(gemerkteWahl);
  useEffect(() => anwenden(wahl), [wahl]);
  const jetzt = WAHLEN.find((w) => w.wahl === wahl)!;
  const weiter = WAHLEN[(WAHLEN.indexOf(jetzt) + 1) % WAHLEN.length];
  return (
    <button
      className="k-taste k-taste-klein k-taste-leise"
      onClick={() => setWahl(weiter.wahl)}
      title={`Darstellung: ${jetzt.wort}. Klick: ${weiter.wort}`}
    >
      <jetzt.Zeichen size={16} strokeWidth={1.75} /> {jetzt.wort}
    </button>
  );
}

function Kopflampe({ phase, ergebnis }: { phase: Phase; ergebnis: KartenErgebnis | null }) {
  if (phase === "kopieren") return <span className="k-lampe k-lampe-leise"><i /> Kopiert</span>;
  if (phase === "pruefen" || phase === "nachlesen" || phase === "nachpruefen")
    return <span className="k-lampe k-lampe-leise"><i /> Prüft</span>;
  if (phase === "fehler") return <span className="k-lampe k-lampe-warn"><i /> Fehler</span>;
  if (ergebnis?.freigabe.sicher) return <span className="k-lampe k-lampe-ok"><i /> Sicher zum Formatieren</span>;
  if (ergebnis) return <span className="k-lampe k-lampe-warn"><i /> Nicht freigegeben</span>;
  return <span className="k-lampe k-lampe-leise"><i /> Bereit</span>;
}

function Zustand(p: {
  phase: Phase;
  stand: Stand;
  ergebnis: KartenErgebnis | null;
  fehler: string | null;
  ziele: string[];
  bereit: boolean;
  befunde: Befund[];
  einlesen: () => void;
  abbrechen: () => void;
  abbruchFragen: boolean;
}) {
  const { phase, stand, ergebnis } = p;

  if (phase === "kopieren" || phase === "pruefen" || phase === "nachlesen" || phase === "nachpruefen") {
    const anteil = stand.bytes > 0 ? stand.gelesen / stand.bytes : 0;
    const sekunden = Math.max(1, (Date.now() - stand.beginn) / 1000);
    const tempo = stand.gelesen / sekunden;
    return (
      <section className="i-zustand">
        <span className="i-zustand-titel">
          {phase === "kopieren"
            ? "Kopiert an alle Ziele"
            : phase === "pruefen"
              ? `Liest Ziel ${stand.pruefZiel + 1} von ${p.ziele.length} zurück`
              : phase === "nachlesen"
                ? "Liest die Karte ein zweites Mal"
                : "Prüft die Kopie gegen ihr ASC MHL"}
        </span>
        {phase === "kopieren" && (
          <div className="i-balken" aria-label="Fortschritt">
            <div style={{ width: `${(anteil * 100).toFixed(1)}%` }} />
          </div>
        )}
        <div className="i-werte">
          {phase === "kopieren" ? (
            <>
              <span>
                <span className="mono">{bytesText(stand.gelesen)}</span> von <span className="mono">{bytesText(stand.bytes)}</span>
              </span>
              <span>
                <span className="mono">{bytesText(tempo)}/s</span>
              </span>
              <span>
                Datei <span className="mono">{stand.datei}</span>
              </span>
            </>
          ) : (
            <span>
              Datei <span className="mono">{stand.pruefPfad}</span>
            </span>
          )}
        </div>
        {stand.ausfaelle.map((a) => (
          <span key={a} className="k-lampe k-lampe-warn">
            <i /> Ziel ausgefallen: {a}
          </span>
        ))}
        <div>
          <button className={`k-taste ${p.abbruchFragen ? "" : "k-taste-leise"}`} onClick={p.abbrechen}>
            {p.abbruchFragen ? "Wirklich abbrechen?" : "Abbrechen"}
          </button>
        </div>
      </section>
    );
  }

  return (
    <section className="i-zustand">
      {ergebnis ? (
        <>
          <span className={`i-zustand-titel k-lampe ${ergebnis.freigabe.sicher ? "k-lampe-ok" : "k-lampe-warn"}`}>
            <i /> {ergebnis.freigabe.sicher ? "Sicher zum Formatieren" : "Nicht freigegeben"}
          </span>
          <span className="i-zustand-grund">{ergebnis.freigabe.grund}</span>
          {ergebnis.freigabe.hinweise.map((h) => (
            <span key={h} className="k-lampe k-lampe-warn">
              <i /> {h}
            </span>
          ))}
        </>
      ) : phase === "fehler" ? (
        <>
          <span className="i-zustand-titel k-lampe k-lampe-warn">
            <i /> Nicht kopiert
          </span>
          <span className="i-zustand-grund">{p.fehler}</span>
        </>
      ) : (
        <span className="i-leer">
          Karte und Ziele wählen. Die Karte wird einmal gelesen, gleichzeitig an alle Ziele geschrieben und jedes Ziel danach
          vollständig zurückgelesen.
        </span>
      )}
      {p.befunde.map((b) => (
        <span key={b.text} className={`k-lampe ${b.stufe === "fehler" ? "k-lampe-kritisch" : "k-lampe-warn"}`}>
          <i /> {b.text}
        </span>
      ))}
      <div>
        <button className={`k-taste ${p.bereit ? "k-taste-amber" : ""}`} disabled={!p.bereit} onClick={p.einlesen}>
          <HardDrive size={16} strokeWidth={1.75} /> {ergebnis || phase === "fehler" ? "Neue Karte einlesen" : "Einlesen"}
        </button>
      </div>
    </section>
  );
}

function NachpruefungAnzeige({ n }: { n: Nachpruefung }) {
  return (
    <section className="i-zustand">
      <span className={`i-zustand-titel k-lampe ${n.abweichungen.length === 0 ? "k-lampe-ok" : "k-lampe-warn"}`}>
        <i /> {n.abweichungen.length === 0 ? "Kopie unverändert" : "Kopie weicht ab"}
      </span>
      <span className="i-pfad mono" title={n.ordner}>
        {n.ordner}
      </span>
      <span className="i-zustand-grund">
        {n.geprueft} Dateien gegen <span className="mono">{n.generation}</span> geprüft
      </span>
      {n.abweichungen.map((a) => (
        <span key={abweichungText(a)} className="i-abweichung">
          {abweichungText(a)}
        </span>
      ))}
    </section>
  );
}

function abweichungText(a: Abweichung) {
  switch (a.art) {
    case "fehlt":
      return `fehlt: ${a.pfad}`;
    case "groesse":
      return `falsche Grösse: ${a.pfad} (${a.ist} statt ${a.soll} Bytes)`;
    case "pruefsumme":
      return `Prüfsumme weicht ab: ${a.pfad}`;
    case "unlesbar":
      return `nicht lesbar: ${a.pfad} (${a.fehler})`;
    case "zusaetzlich":
      return `nicht von der Karte: ${a.pfad}`;
  }
}

function AbgleichAnzeige({ a }: { a: NonNullable<KartenErgebnis["abgleich"]> }) {
  return (
    <section className="i-abschnitt">
      <h2>
        Abgleich mit der Stage{" "}
        <span className="k-leise mono">
          · {a.gefunden.length} gefunden · {a.fehlt.length} fehlen · {a.unerwartet.length} ohne Take
        </span>
      </h2>
      {a.fehlt.map((s) => (
        <span key={s.clip} className="k-lampe k-lampe-warn">
          <i /> Fehlt auf der Karte: <span className="mono">{s.clip}</span> · {s.szene} Take {s.take}
          {s.bewertung && ` · ${s.bewertung}`}
        </span>
      ))}
      {a.unerwartet.map((p) => (
        <span key={p} className="k-lampe k-lampe-leise">
          <i /> Ohne Take (Klärungsliste): <span className="mono">{p}</span>
        </span>
      ))}
      {a.fehlt.length === 0 && a.unerwartet.length === 0 && (
        <span className="k-lampe k-lampe-ok">
          <i /> Alle gedrehten Takes dieser Karte sind da
        </span>
      )}
    </section>
  );
}

function Ergebnis({ ergebnis }: { ergebnis: KartenErgebnis }) {
  const { kopie, urteile, kennungen } = ergebnis;

  const summe = kopie.dateien.reduce((s, d) => s + d.groesse, 0);
  return (
    <>
      {ergebnis.abgleich && <AbgleichAnzeige a={ergebnis.abgleich} />}
      {ergebnis.bewegung.length > 0 && (
        <section className="i-abschnitt">
          <h2>Bewegungsdaten</h2>
          <table className="i-tabelle">
            <thead>
              <tr>
                <th>Clip</th>
                <th className="rechts">Neigung</th>
                <th className="rechts">Rollen</th>
                <th className="rechts">Bereich</th>
                <th className="rechts">Brennweite</th>
              </tr>
            </thead>
            <tbody>
              {ergebnis.bewegung.map(([clip, b]) => (
                <tr key={clip}>
                  <td className="mono">{name(clip)}</td>
                  <td className="mono">{b.tilt ? `${b.tilt.mittel.toFixed(1).replace(".", ",")}°` : "–"}</td>
                  <td className="mono">{b.roll ? `${b.roll.mittel.toFixed(1).replace(".", ",")}°` : "–"}</td>
                  <td className="mono">
                    {b.tilt && b.roll
                      ? `${(b.tilt.max - b.tilt.min).toFixed(1).replace(".", ",")}° / ${(b.roll.max - b.roll.min).toFixed(1).replace(".", ",")}°`
                      : "–"}
                  </td>
                  <td className="mono">{b.brennweiteMm ? `${b.brennweiteMm.toFixed(1).replace(".", ",")} mm` : "–"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
      <section className="i-abschnitt">
        <h2>Ziele</h2>
        {urteile.map((u, i) => {
          const gut = !u.kopierfehler && u.abweichungen.length === 0;
          return (
            <div key={u.ordner} className="i-zustand">
              <span className={`k-lampe ${gut ? "k-lampe-ok" : "k-lampe-warn"}`}>
                <i /> {gut ? `${u.geprueft} Dateien geprüft` : "Ziel fehlerhaft"}
              </span>
              <span className="i-pfad mono" title={u.ordner}>
                {u.ordner}
              </span>
              <span className="k-leise">
                {kennungen[i]?.beschreibung || kennungen[i]?.wert}
                {kennungen[i]?.seriennummer && <span className="mono"> · SN {kennungen[i]?.seriennummer}</span>}
              </span>
              {ergebnis.mhl[i] && <span className="k-leise">ASC MHL: <span className="mono">{name(ergebnis.mhl[i]!)}</span></span>}
              {ergebnis.ale[i] && <span className="k-leise">ALE: <span className="mono">{name(ergebnis.ale[i]!)}</span></span>}
              {"Ok" in ergebnis.berichte[i] ? (
                <span className="k-leise">
                  Bericht: <span className="mono">{name((ergebnis.berichte[i] as { Ok: string }).Ok)}</span>
                </span>
              ) : (
                <span className="i-abweichung">Bericht nicht geschrieben: {(ergebnis.berichte[i] as { Err: string }).Err}</span>
              )}
              {u.kopierfehler && <span className="i-abweichung">Kopieren: {u.kopierfehler}</span>}
              {u.abweichungen.map((a) => (
                <span key={abweichungText(a)} className="i-abweichung">
                  {abweichungText(a)}
                </span>
              ))}
            </div>
          );
        })}
      </section>

      <section className="i-abschnitt">
        <h2>
          Dateien <span className="k-leise mono">· {kopie.dateien.length} · {bytesText(summe)}</span>
        </h2>
        <table className="i-tabelle">
          <thead>
            <tr>
              <th>Datei</th>
              <th className="rechts">Grösse</th>
              <th className="rechts">XXH3-128</th>
            </tr>
          </thead>
          <tbody>
            {kopie.dateien.map((d) => (
              <tr key={d.pfad}>
                <td className="mono">{d.pfad}</td>
                <td className="mono">{bytesText(d.groesse)}</td>
                <td className="mono">{d.pruefsumme.xxh128}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </>
  );
}
