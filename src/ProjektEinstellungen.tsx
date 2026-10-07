// Projekt-Einstellungen (Zahnrad), gleiches Verhalten in allen drei Apps (Systemkarte SCHNITTSTELLEN.md):
// Name änderbar, Kurzname fest, Art, Produktionsfirma, Regie, DoP und die Kamera-Vorgaben. Alles freiwillig.
// Vorläufig eigenes Formular; sobald das gemeinsame Paket stage-projekt da ist, kommt das Formular von dort.
import { useState } from "react";
import { ARTEN, useKonto, type Projekt } from "./konto";
import { Feld, Status } from "./teile";

type Werte = Record<
  "name" | "art" | "firma" | "regie" | "dop" | "fps" | "sensor_fps" | "sensor_modus" | "codec" | "aufloesung" | "aufloesung_px",
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
      className={k === "fps" || k === "sensor_fps" || k === "aufloesung_px" ? "zahl" : ""}
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
