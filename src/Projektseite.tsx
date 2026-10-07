// Projektübersicht: Plan und Stand aus dem Plate Assistant, verbunden mit den eingelesenen Karten auf den Zielen.
// Beantwortet vor dem Formatieren: Ist von diesem Projekt alles da? Welche Takes haben noch keinen Clip?
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { gemerkterZugang, type Projekt } from "./PlateAssistant";

type TakeStand = {
  id: string;
  nummer: number;
  art: string;
  bewertung: string;
  clip: string;
  karte: string | null;
  freigegeben: boolean;
};
type PlateStand = { slate: string; name: string; fotos: number; hdri: string[]; takes: TakeStand[] };
type DrehStand = { id: string; name: string; datum: string; plates: PlateStand[]; hdri: string[] };
type Karte = {
  drehOrdner: string;
  datei: string;
  inhalt: { karte: string; beginn: string; freigegeben: boolean; unabhaengigeKopien: number; clips: unknown[] };
};
type Uebersicht = {
  projekt: Projekt;
  drehs: DrehStand[];
  karten: Karte[];
  ohneTake: string[];
  unlesbar: string[];
  hinweis: string | null;
};

const ART: Record<string, string> = { graukugel: "Graukugel", chromkugel: "Chromkugel", cleanplate: "Cleanplate" };
const BEWERTUNG: Record<string, string> = { circle: "Favorit", gut: "Gut", schlecht: "Schlecht" };
const HDRI: Record<string, string> = {
  aufnahme: "läuft",
  captured: "aufgenommen",
  uploaded: "hochgeladen",
  processed: "verarbeitet",
  linked: "verknüpft",
};

export function Projektseite({ basis }: { basis: string[] }) {
  const zugang = gemerkterZugang();
  const mitZugang = !!(zugang.adresse && zugang.anonKey && zugang.email);
  const [projekte, setProjekte] = useState<Projekt[]>([]);
  const [projekt, setProjekt] = useState<Projekt | null>(null);
  const [kurzVonHand, setKurzVonHand] = useState("");
  const [u, setU] = useState<Uebersicht | null>(null);
  const [laedt, setLaedt] = useState(false);
  const [fehler, setFehler] = useState<string | null>(null);

  useEffect(() => {
    if (mitZugang) invoke<Projekt[]>("plate_projekte", { zugang }).then(setProjekte).catch((e) => setFehler(String(e)));
  }, []);

  async function laden(p: Projekt) {
    setLaedt(true);
    setFehler(null);
    try {
      setU(await invoke<Uebersicht>("projekt_uebersicht", { zugang: mitZugang ? zugang : null, projekt: p, basis }));
    } catch (e) {
      setFehler(String(e));
    }
    setLaedt(false);
  }

  const takes = u?.drehs.flatMap((d) => d.plates.flatMap((p) => p.takes)) ?? [];
  const eingelesen = takes.filter((t) => t.karte).length;
  const freigegeben = takes.filter((t) => t.freigegeben).length;
  const fehlen = takes.length - eingelesen;

  return (
    <div className="i-raster">
      <section className="i-zustand">
        <div className="i-knopfreihe" style={{ marginTop: 0 }}>
          {mitZugang ? (
            <select
              className="i-eingabe"
              style={{ maxWidth: 360, marginTop: 0 }}
              value={projekt?.id ?? ""}
              onChange={(e) => {
                const p = projekte.find((x) => x.id === e.target.value) ?? null;
                setProjekt(p);
                setU(null);
                if (p) laden(p);
              }}
            >
              <option value="">Projekt wählen …</option>
              {projekte.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} ({p.kurzname}){p.aktiv ? "" : " · abgeschlossen"}
                </option>
              ))}
            </select>
          ) : (
            <>
              <input
                className="i-eingabe mono"
                style={{ maxWidth: 240, marginTop: 0 }}
                placeholder="KURZNAME"
                value={kurzVonHand}
                onChange={(e) => setKurzVonHand(e.target.value.toUpperCase())}
              />
              <button
                className="k-taste"
                disabled={!kurzVonHand}
                onClick={() => {
                  const p = { id: "", name: kurzVonHand, kurzname: kurzVonHand, aktiv: true };
                  setProjekt(p);
                  laden(p);
                }}
              >
                Anzeigen
              </button>
            </>
          )}
          {projekt && (
            <button className="k-taste k-taste-leise" disabled={laedt} onClick={() => laden(projekt)}>
              {laedt ? "Lädt …" : "Neu laden"}
            </button>
          )}
        </div>
        <span className="k-leise k-klein">
          Eingelesene Karten werden auf den Zielordnern gesucht ({basis.length ? basis.join(", ") : "keine Ziele gewählt"}).
        </span>
        {fehler && <span className="k-lampe k-lampe-warn"><i /> {fehler}</span>}
        {u?.hinweis && <span className="k-lampe k-lampe-warn"><i /> {u.hinweis}</span>}
        {u && (
          <div className="i-werte">
            <span>
              <span className="mono">{takes.length}</span> Takes gedreht
            </span>
            <span>
              <span className="mono">{eingelesen}</span> eingelesen
            </span>
            <span>
              <span className="mono">{freigegeben}</span> freigegeben
            </span>
            <span className={fehlen ? "k-warn" : ""}>
              <span className="mono">{fehlen}</span> ohne Clip
            </span>
            <span>
              <span className="mono">{u.karten.length}</span> Karten
            </span>
          </div>
        )}
      </section>

      {u && u.ohneTake.length > 0 && (
        <section className="i-abschnitt">
          <h2>Klärung: Clips ohne Take</h2>
          {u.ohneTake.map((c) => (
            <span key={c} className="k-lampe k-lampe-warn">
              <i /> <span className="mono">{c}</span>
            </span>
          ))}
        </section>
      )}

      {u?.drehs.map((d) => (
        <section key={d.id} className="i-abschnitt">
          <h2>
            {d.name} <span className="k-leise mono">· {d.datum}</span>
            {d.hdri.length > 0 && <span className="k-leise"> · HDRI {d.hdri.map((h) => HDRI[h] ?? h).join(", ")}</span>}
          </h2>
          <table className="i-tabelle">
            <thead>
              <tr>
                <th>Plate</th>
                <th className="rechts">Take</th>
                <th>Clip</th>
                <th>Stand</th>
                <th>Bewertung</th>
              </tr>
            </thead>
            <tbody>
              {d.plates.map((p) =>
                p.takes.map((t, i) => (
                  <tr key={t.id}>
                    <td>
                      {i === 0 && (
                        <>
                          <span className="mono">{p.slate || "–"}</span> {p.name}
                          <span className="k-leise k-klein">
                            {p.fotos} Fotos{p.hdri.length > 0 && ` · HDRI ${p.hdri.map((h) => HDRI[h] ?? h).join(", ")}`}
                          </span>
                        </>
                      )}
                    </td>
                    <td className="mono">
                      {t.nummer}
                      {ART[t.art] && <span className="k-leise"> {ART[t.art]}</span>}
                    </td>
                    <td className="mono">{t.clip || <span className="k-aus">–</span>}</td>
                    <td>
                      {t.freigegeben ? (
                        <span className="k-lampe k-lampe-ok"><i /> {t.karte}</span>
                      ) : t.karte ? (
                        <span className="k-lampe k-lampe-warn"><i /> {t.karte} · nicht freigegeben</span>
                      ) : (
                        <span className="k-lampe k-lampe-warn"><i /> Karte fehlt</span>
                      )}
                    </td>
                    <td>{BEWERTUNG[t.bewertung] ?? ""}</td>
                  </tr>
                )),
              )}
            </tbody>
          </table>
        </section>
      ))}

      {u && u.karten.length > 0 && (
        <section className="i-abschnitt">
          <h2>Eingelesene Karten</h2>
          <table className="i-tabelle">
            <thead>
              <tr>
                <th>Karte</th>
                <th>Dreh</th>
                <th className="rechts">Clips</th>
                <th>Freigabe</th>
              </tr>
            </thead>
            <tbody>
              {u.karten.map((k) => (
                <tr key={k.datei}>
                  <td className="mono">{k.inhalt.karte}</td>
                  <td className="mono">{k.drehOrdner}</td>
                  <td className="mono">{k.inhalt.clips.length}</td>
                  <td>
                    <span className={`k-lampe ${k.inhalt.freigegeben ? "k-lampe-ok" : "k-lampe-warn"}`}>
                      <i /> {k.inhalt.freigegeben ? `${k.inhalt.unabhaengigeKopien} Kopien` : "nicht freigegeben"}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
      {!u && !laedt && (
        <span className="i-leer">
          Projekt wählen: die Übersicht zeigt pro Drehort die gedrehten Takes, welche schon sicher kopiert sind und für
          welche die Karte noch fehlt.
        </span>
      )}
    </div>
  );
}
