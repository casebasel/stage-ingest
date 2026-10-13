// Projekt-Einstellungen (Zahnrad), gleiches Verhalten in allen drei Apps (Systemkarte SCHNITTSTELLEN.md):
// Name änderbar, Kurzname fest, Art, Produktionsfirma, Regie, DoP und die Kamera-Vorgaben. Alles freiwillig.
// Vorläufig eigenes Formular; sobald das gemeinsame Paket stage-projekt da ist, kommt das Formular von dort.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ARTEN, drehsVon, useKonto, type Projekt } from "./konto";
import { Feld, Status } from "./teile";

type Werte = Record<
  | "name"
  | "art"
  | "firma"
  | "regie"
  | "dop"
  | "fps"
  | "sensor_fps"
  | "sensor_modus"
  | "codec"
  | "aufloesung"
  | "aufloesung_px"
  | "kopien"
  | "aufnahme_gamma"
  | "look",
  string
>;

/** „3840 × 2160“ → „3840x2160“ (Schreibweise der Datenbank). */
const pixel = (t: string) => t.trim().toLowerCase().replace(/[×*]/g, "x").replace(/\s+/g, "");

const zahlText = (n: number | null | undefined) => (n == null ? "" : String(n).replace(".", ","));

function ausProjekt(p: Projekt): Werte {
  return {
    name: p.name,
    art: p.art ?? "",
    firma: p.firma ?? "",
    regie: p.regie ?? "",
    dop: p.dop ?? "",
    fps: zahlText(p.fps),
    sensor_fps: zahlText(p.sensorFps),
    sensor_modus: p.sensorModus ?? "",
    codec: p.codec ?? "",
    aufloesung: p.aufloesung ?? "",
    aufloesung_px: p.aufloesungPx ?? "",
    kopien: p.kopien == null ? "" : String(p.kopien),
    aufnahme_gamma: p.aufnahmeGamma ?? "",
    look: p.look ?? "",
  };
}

export function ProjektEinstellungen({ projekt, schliessen }: { projekt: Projekt; schliessen: (p?: Projekt) => void }) {
  const konto = useKonto();
  const [w, setW] = useState<Werte>(() => ausProjekt(projekt));
  const [stand, setStand] = useState<{ ton: "fehler" | "laeuft"; text: string } | null>(null);
  const alt = ausProjekt(projekt);
  const geaendert = (Object.keys(w) as (keyof Werte)[]).filter((k) => w[k].trim() !== alt[k].trim());

  function pruefen(): string | null {
    if (!w.name.trim()) return "Der Name darf nicht leer sein.";
    for (const k of ["fps", "sensor_fps"] as const) {
      const t = w[k].trim();
      if (t && !(Number(t.replace(",", ".")) > 0)) return `${k === "fps" ? "Framerate" : "Sensor-Framerate"}: eine Zahl wie 25 oder 23,976.`;
    }
    // Feldgrenzen der Datenbank (Migration 0016, Systemkarte ee5bc62): Texte höchstens 200 Zeichen.
    const lang = (Object.keys(w) as (keyof Werte)[]).find((k) => w[k].trim().length > 200);
    if (lang) return "Höchstens 200 Zeichen pro Feld.";
    const k = w.kopien.trim();
    if (k && !/^[2-9]$/.test(k)) return "Kopien vor der Freigabe: eine Zahl von 2 bis 9 (leer = 2). Eine einzige Kopie nur als Testschwelle in der Einrichtung.";
    const px = pixel(w.aufloesung_px);
    if (px && !/^[1-9][0-9]{0,5}x[1-9][0-9]{0,5}$/.test(px)) return "Auflösung in Pixeln als Breite x Höhe, z. B. 3840x2160.";
    return null;
  }

  async function speichern() {
    const f = pruefen();
    if (f) return setStand({ ton: "fehler", text: f });
    const felder: Record<string, string | number | null> = {};
    for (const k of geaendert) {
      const t = w[k].trim();
      if (k === "fps" || k === "sensor_fps") felder[k] = t ? Number(t.replace(",", ".")) : null;
      // Als JSON-Zahl (0025 lehnt Text ab); leer = Standard 2.
      else if (k === "kopien") felder[k] = t ? Number(t) : null;
      else if (k === "aufloesung_px") felder[k] = pixel(t) || null;
      else felder[k] = t || null;
    }
    setStand({ ton: "laeuft", text: "Speichert …" });
    try {
      await konto.projektAendern(projekt.id, felder);
      setStand(null);
      schliessen({ ...projekt, name: w.name.trim() });
    } catch (e) {
      setStand({ ton: "fehler", text: String(e) });
    }
  }

  const eingabe = (k: keyof Werte, label: string, platzhalter = "") => (
    <input
      aria-label={label}
      className={k === "fps" || k === "sensor_fps" || k === "aufloesung_px" || k === "kopien" ? "zahl" : ""}
      value={w[k]}
      placeholder={platzhalter}
      onChange={(e) => setW({ ...w, [k]: e.target.value })}
    />
  );

  return (
    <section className="block projekt-einstellungen" aria-labelledby="t-pe">
      <div className="block-kopf">
        <h2 id="t-pe">Projekt-Einstellungen</h2>
        <span className="leise">Gilt in allen drei Apps</span>
      </div>
      <Feld name="Name">{eingabe("name", "Name")}</Feld>
      <Feld name="Kurzname" hilfe="Ordnername auf allen Platten; nach dem Anlegen fest.">
        <span className="zahl">{projekt.kurzname}</span>
      </Feld>
      <Feld name="Art">
        <select aria-label="Art" value={w.art} onChange={(e) => setW({ ...w, art: e.target.value })}>
          {ARTEN.map(([id, text]) => (
            <option key={id} value={id}>
              {text}
            </option>
          ))}
        </select>
      </Feld>
      <Feld name="Produktionsfirma" hilfe="Steht im Bericht und geht mit dem Slate an die Kamera.">
        {eingabe("firma", "Produktionsfirma")}
      </Feld>
      <Feld name="Regie">{eingabe("regie", "Regie")}</Feld>
      <Feld name="DoP">{eingabe("dop", "DoP")}</Feld>
      <Feld name="Framerate" hilfe="Basis des Timecodes. Weichen Clips ab, warnt der Ingest (sperrt nie).">
        {eingabe("fps", "Framerate", "z. B. 25")}
        <span className="leise">fps</span>
      </Feld>
      <Feld name="Sensor-Framerate" hilfe="Leer = gleich der Framerate.">
        {eingabe("sensor_fps", "Sensor-Framerate", "leer = gleich")}
        <span className="leise">fps</span>
      </Feld>
      <Feld name="Sensormodus" hilfe="So, wie die Kamera ihn nennt.">{eingabe("sensor_modus", "Sensormodus", "z. B. 3.4K 16:9")}</Feld>
      <Feld name="Codec" hilfe="So, wie die Kamera ihn nennt.">{eingabe("codec", "Codec", "z. B. ProRes 422 HQ")}</Feld>
      <Feld name="Auflösung" hilfe="Name wie in der Kamera.">{eingabe("aufloesung", "Auflösung", "z. B. 4K UHD")}</Feld>
      <Feld name="Auflösung in Pixeln" hilfe="Breite x Höhe; damit prüft der Ingest die Clips.">
        {eingabe("aufloesung_px", "Auflösung in Pixeln", "z. B. 3840x2160")}
      </Feld>
      <Feld name="Aufnahme-Gamma" hilfe="Wie aufgenommen wird, z. B. Log C. Weicht ein Clip ab, warnt der Ingest (sperrt nie).">
        {eingabe("aufnahme_gamma", "Aufnahme-Gamma", "z. B. Log C")}
      </Feld>
      <Feld name="Look" hilfe="Look-Name in der Kamera, z. B. ARRI 709. Weicht ein Clip ab, warnt der Ingest (sperrt nie).">
        {eingabe("look", "Look", "z. B. ARRI 709")}
      </Feld>
      <Feld
        name="Kopien vor der Freigabe"
        hilfe="Unabhängige, geprüfte Kopien auf verschiedenen Platten, bevor eine Karte als sicher gilt. Leer = 2; Netflix verlangt 3."
      >
        {eingabe("kopien", "Kopien vor der Freigabe", "2")}
      </Feld>
      <div className="knopfreihe einstellungen-knoepfe">
        {stand && <Status ton={stand.ton}>{stand.text}</Status>}
        <button className="knopf" onClick={() => schliessen()}>
          Abbrechen
        </button>
        <button className="knopf knopf-haupt" disabled={geaendert.length === 0 || stand?.ton === "laeuft"} onClick={speichern}>
          Speichern
        </button>
      </div>
    </section>
  );
}

/** Neues Projekt anlegen: Name, Kurzname wird vorgeschlagen und ist danach fest. */
export function NeuesProjekt({ fertig }: { fertig: (p?: Projekt) => void }) {
  const konto = useKonto();
  const [name, setName] = useState("");
  const [kurzname, setKurzname] = useState("");
  const [vonHand, setVonHand] = useState(false);
  const [stand, setStand] = useState<{ ton: "fehler" | "laeuft"; text: string } | null>(null);
  const vorhanden = konto.projekte.find((p) => p.kurzname === kurzname);

  async function anlegen() {
    if (!name.trim()) return setStand({ ton: "fehler", text: "Der Name darf nicht leer sein." });
    if (!/^[A-Z0-9]+(_[A-Z0-9]+)*$/.test(kurzname) || kurzname.length < 2 || kurzname.length > 24)
      return setStand({ ton: "fehler", text: "Kurzname: 2–24 Zeichen, nur A–Z, 0–9 und _ (nicht vorne oder hinten)." });
    if (vorhanden) return setStand({ ton: "fehler", text: `Den Kurznamen hat schon „${vorhanden.name}“.` });
    setStand({ ton: "laeuft", text: "Legt an …" });
    try {
      fertig(await konto.projektAnlegen(name.trim(), kurzname));
    } catch (e) {
      setStand({ ton: "fehler", text: String(e) });
    }
  }

  return (
    <section className="block projekt-einstellungen" aria-labelledby="t-np">
      <div className="block-kopf">
        <h2 id="t-np">Neues Projekt</h2>
        <span className="leise">Erscheint danach in allen drei Apps</span>
      </div>
      <Feld name="Name" hilfe="Änderbar, auch später.">
        <input
          aria-label="Name"
          autoFocus
          placeholder="z. B. Happy End"
          value={name}
          onChange={async (e) => {
            const n = e.target.value;
            setName(n);
            setStand(null);
            if (!vonHand) setKurzname(await konto.kurznameVorschlag(n));
          }}
        />
      </Feld>
      <Feld name="Kurzname" hilfe="Ordnername auf allen Platten. Nach dem Anlegen fest.">
        <input
          aria-label="Kurzname"
          className="zahl"
          placeholder="HAPPY_END"
          value={kurzname}
          onChange={(e) => {
            setVonHand(true);
            setKurzname(e.target.value.toUpperCase());
            setStand(null);
          }}
        />
      </Feld>
      <div className="knopfreihe einstellungen-knoepfe">
        {stand && <Status ton={stand.ton}>{stand.text}</Status>}
        <button className="knopf" onClick={() => fertig()}>
          Abbrechen
        </button>
        <button className="knopf knopf-haupt" disabled={!name.trim() || !kurzname || stand?.ton === "laeuft"} onClick={anlegen}>
          Anlegen
        </button>
      </div>
    </section>
  );
}

/** Nächster freier Kurzname: RHEINUFER, RHEINUFER_2, … (höchstens 12 Zeichen, der Stamm wird nötigenfalls gekürzt). */
export function freierKurzname(wunsch: string, vergeben: string[]): string {
  if (!vergeben.includes(wunsch)) return wunsch;
  for (let n = 2; n < 100; n++) {
    const endung = `_${n}`;
    const k = wunsch.slice(0, 12 - endung.length).replace(/_+$/, "") + endung;
    if (!vergeben.includes(k)) return k;
  }
  return wunsch;
}

/** Drehort anlegen (Systemkarte 4f197ed): Kurzname vorgeschlagen und änderbar, fest nach dem Anlegen, Ordnername;
 *  schon vergeben → Warnung und nächster freier. Datum bis Stufe C Pflicht. */
export function NeuerDrehort({ projekt, fertig }: { projekt: Projekt; fertig: (id?: string) => void }) {
  const konto = useKonto();
  const [name, setName] = useState("");
  const [kurzname, setKurzname] = useState("");
  const [vonHand, setVonHand] = useState(false);
  /** Vorschlag war vergeben und wurde auf den nächsten freien gesetzt: sichtbar machen. */
  const [ausgewichen, setAusgewichen] = useState<string | null>(null);
  const [datum, setDatum] = useState(() => new Date().toLocaleDateString("sv-SE"));
  const [stand, setStand] = useState<{ ton: "fehler" | "laeuft" | "warn"; text: string } | null>(null);
  // Vergeben sind auch die Kurznamen gelöschter Drehorte (Systemkarte 4ae71dd); vom Server nachgeladen.
  const [vomServer, setVomServer] = useState<string[]>([]);
  useEffect(() => {
    invoke<string[]>("plate_drehort_kurznamen", { zugang: konto.zugang, projektId: projekt.id })
      .then(setVomServer)
      .catch(() => setVomServer([]));
  }, [projekt.id]);
  const vergeben = drehsVon(konto.drehs, projekt)
    .map((d) => d.kurzname)
    .filter((k): k is string => !!k)
    .concat(vomServer, "STUDIO");
  const doppelt = !!kurzname && vergeben.includes(kurzname);

  async function vorschlagen(n: string) {
    const k = await konto.kurznameVorschlag(n, 12);
    const frei = freierKurzname(k, vergeben);
    setKurzname(frei);
    setAusgewichen(frei !== k ? k : null);
  }

  async function anlegen() {
    if (!name.trim()) return setStand({ ton: "fehler", text: "Der Name darf nicht leer sein." });
    if (!/^[A-Z0-9]+(_[A-Z0-9]+)*$/.test(kurzname) || kurzname.length < 2 || kurzname.length > 12)
      return setStand({ ton: "fehler", text: "Kurzname: 2–12 Zeichen, nur A–Z, 0–9 und _ (nicht vorne, hinten oder doppelt)." });
    if (doppelt) return setStand({ ton: "fehler", text: `${kurzname} gibt es im Projekt schon. Frei wäre ${freierKurzname(kurzname, vergeben)}.` });
    setStand({ ton: "laeuft", text: "Legt an …" });
    try {
      fertig(await konto.drehortAnlegen(projekt, name.trim(), kurzname, datum));
    } catch (e) {
      setStand({ ton: "fehler", text: String(e) });
    }
  }

  return (
    <section className="block projekt-einstellungen" aria-labelledby="t-nd">
      <div className="block-kopf">
        <h2 id="t-nd">Neuer Drehort</h2>
        <span className="leise">in {projekt.name}</span>
      </div>
      <Feld name="Name" hilfe="Änderbar, auch später.">
        <input
          aria-label="Name des Drehorts"
          autoFocus
          placeholder="z. B. Rheinufer Kleinbasel"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            setStand(null);
            if (!vonHand) vorschlagen(e.target.value);
          }}
        />
      </Feld>
      <Feld
        name="Kurzname"
        hilfe={
          <>
            Nach dem Anlegen fest. Ordnername auf allen Platten (
            <span className="zahl">
              {projekt.kurzname}/{datum || "Datum"}_{kurzname || "KURZNAME"}
            </span>
            ) und Anfang der Plate-Namen (<span className="zahl">{kurzname || "KURZNAME"}-01</span>).
          </>
        }
      >
        <input
          aria-label="Kurzname des Drehorts"
          className="zahl"
          placeholder="RHEINUFER"
          maxLength={12}
          value={kurzname}
          onChange={(e) => {
            setVonHand(true);
            setAusgewichen(null);
            setKurzname(e.target.value.toUpperCase());
            setStand(null);
          }}
        />
      </Feld>
      {ausgewichen && !doppelt && (
        <p className="feld-meldung">
          <Status ton="warn">
            {ausgewichen} gibt es im Projekt schon, darum {kurzname}. Ist es derselbe Ort, diesen Drehort nicht neu
            anlegen, sondern den vorhandenen wählen.
          </Status>
        </p>
      )}
      {doppelt && (
        <p className="feld-meldung">
          <Status ton="warn">
            {kurzname} ist im Projekt schon vergeben (auch gelöschte Drehorte zählen).{" "}
            <button className="verweis" onClick={() => setKurzname(freierKurzname(kurzname, vergeben))}>
              {freierKurzname(kurzname, vergeben)} verwenden
            </button>
          </Status>
        </p>
      )}
      <Feld name="Datum" hilfe="Vorerst Pflicht; später freiwillig, dann zählt das Aufnahmedatum der Clips.">
        <input aria-label="Datum" type="date" className="zahl" value={datum} onChange={(e) => setDatum(e.target.value)} />
      </Feld>
      <div className="knopfreihe einstellungen-knoepfe">
        {stand && <Status ton={stand.ton}>{stand.text}</Status>}
        <button className="knopf" onClick={() => fertig()}>
          Abbrechen
        </button>
        <button className="knopf knopf-haupt" disabled={!name.trim() || !kurzname || !datum || stand?.ton === "laeuft"} onClick={anlegen}>
          Anlegen
        </button>
      </div>
    </section>
  );
}
