import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderInput, HardDrive, Plus, X } from "lucide-react";
import {
  abbrechen,
  aufFortschritt,
  bytesText,
  karteEinlesen,
  type Fortschritt,
  type KartenErgebnis,
  type Abweichung,
} from "./kern";
import { Aktualisierung, useAktualisierung } from "./Aktualisierung";

type Phase = "bereit" | "kopieren" | "pruefen" | "fertig" | "fehler";

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

const trenner = (pfad: string) => (pfad.includes("\\") ? "\\" : "/");
const name = (pfad: string) => pfad.split(/[\\/]/).filter(Boolean).pop() ?? pfad;

export function App() {
  const [quelle, setQuelle] = useState<string | null>(null);
  const [zielOrdner, setZielOrdner] = useState<string[]>([]);
  const [mitMd5, setMitMd5] = useState(false);
  const [mindestKopien, setMindestKopien] = useState(2);
  const [phase, setPhase] = useState<Phase>("bereit");
  const [stand, setStand] = useState<Stand>(LEERER_STAND);
  const [ergebnis, setErgebnis] = useState<KartenErgebnis | null>(null);
  const [fehler, setFehler] = useState<string | null>(null);
  const [abbruchFragen, setAbbruchFragen] = useState(false);
  const abbruchZeit = useRef(0);
  const { update, pflicht, version } = useAktualisierung();

  useEffect(() => {
    const weg = aufFortschritt((f: Fortschritt) =>
      setStand((s) => {
        if (f.phase === "pruefen") {
          setPhase("pruefen");
          return { ...s, pruefZiel: f.ziel, pruefPfad: f.pfad };
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

  const laeuft = phase === "kopieren" || phase === "pruefen";
  // Jede Karte kommt in einen eigenen Ordner mit ihrem Namen (z. B. A001R132) unter dem gewählten Ziel.
  const ziele = quelle ? zielOrdner.map((z) => z.replace(/[\\/]+$/, "") + trenner(z) + name(quelle)) : [];

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
      setErgebnis(await karteEinlesen({ quelle, ziele, mitMd5, mindestKopien }));
      setPhase("fertig");
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
        <Kopflampe phase={phase} ergebnis={ergebnis} />
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
            <div className="k-zeile">
              <span className="k-zeile-name">Prüfsumme</span>
              <span className="k-zeile-wert mono">XXH3-128{mitMd5 ? " + MD5" : ""}</span>
            </div>
            <div className="k-zeile">
              <span className="k-zeile-name">Zurücklesen</span>
              <span className="k-zeile-wert">jedes Ziel, ohne Cache</span>
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
              bereit={!!quelle && ziele.length > 0 && !pflicht}
              einlesen={einlesen}
              abbrechen={abbrechenKlick}
              abbruchFragen={abbruchFragen}
            />
            {ergebnis && <Ergebnis ergebnis={ergebnis} />}
          </div>
        </main>
      </div>
    </div>
  );
}

function Kopflampe({ phase, ergebnis }: { phase: Phase; ergebnis: KartenErgebnis | null }) {
  if (phase === "kopieren") return <span className="k-lampe k-lampe-leise"><i /> Kopiert</span>;
  if (phase === "pruefen") return <span className="k-lampe k-lampe-leise"><i /> Prüft</span>;
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
  einlesen: () => void;
  abbrechen: () => void;
  abbruchFragen: boolean;
}) {
  const { phase, stand, ergebnis } = p;

  if (phase === "kopieren" || phase === "pruefen") {
    const anteil = stand.bytes > 0 ? stand.gelesen / stand.bytes : 0;
    const sekunden = Math.max(1, (Date.now() - stand.beginn) / 1000);
    const tempo = stand.gelesen / sekunden;
    return (
      <section className="i-zustand">
        <span className="i-zustand-titel">
          {phase === "kopieren" ? "Kopiert an alle Ziele" : `Liest Ziel ${stand.pruefZiel + 1} von ${p.ziele.length} zurück`}
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
          {ergebnis.freigabe.kennung_unsicher && (
            <span className="k-lampe k-lampe-warn">
              <i /> Platten nur über das Volume erkannt, Seriennummer noch nicht geprüft
            </span>
          )}
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
      <div>
        <button className={`k-taste ${p.bereit ? "k-taste-amber" : ""}`} disabled={!p.bereit} onClick={p.einlesen}>
          <HardDrive size={16} strokeWidth={1.75} /> {ergebnis || phase === "fehler" ? "Neue Karte einlesen" : "Einlesen"}
        </button>
      </div>
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

function Ergebnis({ ergebnis }: { ergebnis: KartenErgebnis }) {
  const { kopie, urteile, kennungen } = ergebnis;

  const summe = kopie.dateien.reduce((s, d) => s + d.groesse, 0);
  return (
    <>
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
              <span className="k-leise mono">{kennungen[i]?.wert}</span>
              {ergebnis.mhl[i] && <span className="k-leise">ASC MHL: <span className="mono">{name(ergebnis.mhl[i]!)}</span></span>}
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
