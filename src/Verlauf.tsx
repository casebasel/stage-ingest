// Verlauf aller eingelesenen Karten auf diesem Rechner (Entscheidung 12). Klick öffnet den Bericht.
import { useEffect, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { bytesText, verlauf, type VerlaufEintrag } from "./kern";

const datum = (iso: string) =>
  new Date(iso).toLocaleString("de-CH", { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit" });

export function Verlauf({ neu }: { neu: string }) {
  const [liste, setListe] = useState<VerlaufEintrag[]>([]);
  useEffect(() => {
    verlauf().then(setListe).catch(() => setListe([]));
  }, [neu]);

  if (liste.length === 0) return null;
  return (
    <section className="i-abschnitt">
      <h2>Verlauf</h2>
      <table className="i-tabelle">
        <thead>
          <tr>
            <th>Karte</th>
            <th>Eingelesen</th>
            <th className="rechts">Dateien</th>
            <th className="rechts">Grösse</th>
            <th>Urteil</th>
            <th>Bericht</th>
          </tr>
        </thead>
        <tbody>
          {liste.map((e) => {
            const bericht = e.ziele.find((z) => z.bericht)?.bericht;
            return (
              <tr key={e.beginn + e.karte}>
                <td className="mono">{e.karte}</td>
                <td className="mono">{datum(e.beginn)}</td>
                <td className="mono">{e.dateien}</td>
                <td className="mono">{bytesText(e.bytes)}</td>
                <td>
                  <span className={`k-lampe ${e.sicher ? "k-lampe-ok" : "k-lampe-warn"}`} title={e.grund}>
                    <i /> {e.sicher ? "freigegeben" : "nicht freigegeben"}
                  </span>
                </td>
                <td>
                  {bericht ? (
                    <button className="k-taste k-taste-klein k-taste-leise" onClick={() => openPath(bericht)}>
                      Öffnen
                    </button>
                  ) : (
                    <span className="k-aus">–</span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </section>
  );
}
