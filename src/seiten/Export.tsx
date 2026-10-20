/** Export eines Clips (Marlon, 15.10.2026): ProRes MOV, MP4 H.264/H.265 oder EXR, Rec.709 mit/ohne Look oder Log,
 *  Grösse, Ziel `06_EXPORT` im Drehordner oder frei. Rechnet mit ARRI ART CMD (src-tauri/src/export.rs).
 *  Mehrere Takes auf einmal (Marlon, 20.10.2026: „diese Selektion für mehrere Clips und dann ein Batch Processing“):
 *  dieselben Einstellungen, derselbe Zielordner, einer nach dem anderen; Abbrechen wirkt nach dem laufenden Clip. */
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { X } from "lucide-react";
import { gemerkt, merken, useEinstellungen } from "../einstellungen";
import { Pfad, Status } from "../teile";

type Wahl = { format: string; stufe: string; farbe: string; breite: number | null };
const STANDARD: Wahl = { format: "prores", stufe: "prores422hq", farbe: "rec709look", breite: null };

const FORMATE: [string, string][] = [
  ["prores", "ProRes MOV"],
  ["h264", "MP4 H.264"],
  ["h265", "MP4 H.265"],
  ["exr", "EXR-Sequenz"],
];
const STUFEN: [string, string][] = [
  ["prores422lt", "422 LT"],
  ["prores422", "422"],
  ["prores422hq", "422 HQ"],
  ["prores4444", "4444"],
  ["prores4444xq", "4444 XQ"],
];
const FARBEN: [string, string][] = [
  ["rec709look", "Rec.709 mit Look"],
  ["rec709", "Rec.709"],
  ["log", "Log (wie aufgenommen)"],
];
const GROESSEN: [number | null, string][] = [
  [null, "Original"],
  [3840, "3840"],
  [1920, "1920"],
  [1280, "1280"],
];
const windows = navigator.userAgent.includes("Windows");

export type ExportClip = { datei: string; titel: string };
type Ergebnis = { titel: string; pfad?: string; hinweis?: string | null; fehler?: string };
type Stand =
  | { art: "bereit" }
  | { art: "laeuft"; seit: number; nummer: number; ergebnisse: Ergebnis[] }
  | { art: "fertig"; ergebnisse: Ergebnis[]; abgebrochen: boolean };

export function ExportDialog({ dateien, titel, zu }: { dateien: ExportClip[]; titel: string; zu: () => void }) {
  const e = useEinstellungen();
  const [w, setW] = useState<Wahl>(() => ({ ...STANDARD, ...gemerkt<Partial<Wahl>>("export", {}) }));
  const [ziel, setZiel] = useState<string | null>(null);
  const [vorschlag, setVorschlag] = useState("");
  const [stand, setStand] = useState<Stand>({ art: "bereit" });
  const [jetzt, setJetzt] = useState(Date.now());
  const abbrechen = useRef(false);
  const datei = dateien[0]?.datei ?? "";
  const auftrag = { datei, ...w, ziel };
  useEffect(() => {
    invoke<string>("export_zielordner", { auftrag: { ...auftrag, ziel: null } })
      .then(setVorschlag)
      .catch(() => {});
  }, [datei, w.format, w.stufe, w.farbe, w.breite]);
  useEffect(() => {
    if (stand.art !== "laeuft") return;
    const t = setInterval(() => setJetzt(Date.now()), 1000);
    return () => clearInterval(t);
  }, [stand.art]);
  useEffect(() => {
    const taste = (ev: KeyboardEvent) => ev.key === "Escape" && stand.art !== "laeuft" && zu();
    // (Abbrechen während eines Laufs nur über den Knopf, damit nichts halb liegen bleibt.)
    window.addEventListener("keydown", taste);
    return () => window.removeEventListener("keydown", taste);
  }, [stand.art]);
  const setze = (x: Partial<Wahl>) => {
    const n = { ...w, ...x };
    setW(n);
    merken("export", n);
  };
  const mp4OhneMac = windows && (w.format === "h264" || w.format === "h265");
  const ohneArt = !e.artCmd.trim();

  async function los() {
    abbrechen.current = false;
    const ergebnisse: Ergebnis[] = [];
    let abgebrochen = false;
    for (const [i, c] of dateien.entries()) {
      if (abbrechen.current) {
        abgebrochen = true;
        break;
      }
      setStand({ art: "laeuft", seit: Date.now(), nummer: i + 1, ergebnisse: [...ergebnisse] });
      try {
        const r = await invoke<{ pfad: string; hinweis: string | null }>("clip_exportieren", {
          auftrag: { ...auftrag, datei: c.datei },
          artCmd: e.artCmd.trim() || null,
        });
        ergebnisse.push({ titel: c.titel, pfad: r.pfad, hinweis: r.hinweis });
      } catch (err) {
        ergebnisse.push({ titel: c.titel, fehler: String(err) });
      }
    }
    setStand({ art: "fertig", ergebnisse, abgebrochen });
  }
  const laeuft = stand.art === "laeuft";
  const mehrere = dateien.length > 1;
  const zeigen = (pfad: string) => revealItemInDir(pfad).catch(() => {});

  const segment = <T,>(liste: [T, string][], wert: T, setzen: (v: T) => void, aus = false) => (
    <div className="segment" role="group">
      {liste.map(([id, text]) => (
        <button key={String(id)} aria-pressed={wert === id} disabled={aus || laeuft} onClick={() => setzen(id)}>
          {text}
        </button>
      ))}
    </div>
  );

  return (
    <div className="dialog-hinter" onMouseDown={() => !laeuft && zu()}>
      <div className="dialog export-dialog" role="dialog" aria-modal="true" aria-label="Clip exportieren" onMouseDown={(ev) => ev.stopPropagation()}>
        <div className="dialog-kopf">
          <h2>Exportieren · {mehrere ? `${dateien.length} Takes` : titel}</h2>
          <button className="knopf-symbol" aria-label="Schliessen" onClick={zu} disabled={laeuft}>
            <X size={16} />
          </button>
        </div>
        {mehrere && (
          <p className="leise export-liste">
            {dateien.map((c) => c.titel).join(" · ")}
          </p>
        )}
        <div className="export-felder">
          <span className="export-name">Format</span>
          {segment(FORMATE, w.format, (format) => setze({ format }))}
          {w.format === "prores" && (
            <>
              <span className="export-name">ProRes</span>
              {segment(STUFEN, w.stufe, (stufe) => setze({ stufe }))}
            </>
          )}
          <span className="export-name">Farbe</span>
          {w.format === "exr" ? (
            <span className="leise">Linear, ACES AP0 (für VFX)</span>
          ) : (
            segment(FARBEN, w.farbe, (farbe) => setze({ farbe }))
          )}
          <span className="export-name">Breite</span>
          {segment(GROESSEN, w.breite, (breite) => setze({ breite }))}
          <span className="export-name">Ziel</span>
          <span className="export-ziel">
            <Pfad pfad={ziel ?? vorschlag} />
            <span className="knopfreihe">
              <button
                className="verweis"
                disabled={laeuft}
                onClick={async () => {
                  const p = await open({ directory: true, title: "Zielordner für den Export" });
                  if (typeof p === "string") setZiel(p);
                }}
              >
                Anderer Ordner …
              </button>
              {ziel && (
                <button className="verweis" onClick={() => setZiel(null)}>
                  Standard (06_EXPORT)
                </button>
              )}
            </span>
          </span>
        </div>
        <div className="export-fuss">
          {ohneArt ? (
            <Status ton="warn">Export braucht ARRI ART CMD (Einrichtung → ARRI-Werkzeuge, gratis bei ARRI).</Status>
          ) : mp4OhneMac ? (
            <Status ton="warn">MP4 exportiert die App vorerst nur am Mac. Unter Windows ProRes oder EXR wählen.</Status>
          ) : stand.art === "laeuft" ? (
            <span className="export-fertig">
              <Status ton="laeuft">
                {mehrere ? `${stand.nummer} von ${dateien.length}: ` : ""}
                {dateien[stand.nummer - 1]?.titel} · rechnet mit ART CMD … {Math.round((jetzt - stand.seit) / 1000)} s
              </Status>
              {stand.ergebnisse.some((r) => r.fehler) && (
                <Status ton="warn">{stand.ergebnisse.filter((r) => r.fehler).length} bisher nicht exportiert</Status>
              )}
            </span>
          ) : stand.art === "fertig" ? (
            <span className="export-fertig">
              {(() => {
                const gut = stand.ergebnisse.filter((r) => r.pfad);
                const schlecht = stand.ergebnisse.filter((r) => r.fehler);
                return (
                  <>
                    <Status ton={schlecht.length ? "warn" : "ok"}>
                      {mehrere
                        ? `${gut.length} von ${dateien.length} exportiert${stand.abgebrochen ? " (abgebrochen)" : ""}`
                        : "Fertig"}
                    </Status>
                    {!mehrere && gut[0]?.pfad && <Pfad pfad={gut[0].pfad} />}
                    {gut[0]?.pfad && (
                      <button className="verweis" onClick={() => zeigen(gut[0].pfad!)}>
                        {windows ? "Im Explorer zeigen" : "Im Finder zeigen"}
                      </button>
                    )}
                    {stand.ergebnisse
                      .filter((r) => r.hinweis || r.fehler)
                      .map((r) => (
                        <Status key={r.titel} ton={r.fehler ? "fehler" : "warn"}>
                          {mehrere ? `${r.titel}: ` : ""}
                          {r.fehler ?? r.hinweis}
                        </Status>
                      ))}
                  </>
                );
              })()}
            </span>
          ) : (
            <span className="leise">
              {mehrere ? "Alle mit denselben Einstellungen in denselben Ordner, einer nach dem anderen. " : ""}
              Das Original bleibt unberührt; ein bestehender Export wird nie überschrieben.
            </span>
          )}
          {laeuft && mehrere && (
            <button className="knopf" disabled={abbrechen.current} onClick={() => (abbrechen.current = true)}>
              Nach diesem Take abbrechen
            </button>
          )}
          <button className="knopf knopf-haupt" disabled={ohneArt || mp4OhneMac || laeuft} onClick={los}>
            {stand.art === "fertig" ? "Nochmals exportieren" : mehrere ? `${dateien.length} Takes exportieren` : "Exportieren"}
          </button>
        </div>
      </div>
    </div>
  );
}
