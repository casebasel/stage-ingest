// Prüfen: eine bestehende Kopie jederzeit gegen ihr ASC MHL nachprüfen, und der Verlauf aller Karten dieses Rechners.
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { FileText, ShieldCheck } from "lucide-react";
import { bytesText, verlauf, type VerlaufEintrag } from "../kern";
import { useLauf } from "../lauf";
import { Pfad, Status, datumZeit } from "../teile";
import { abweichungText } from "./Einlesen";

export function Pruefen() {
  const lauf = useLauf();
  const n = lauf.nachpruefung;
  const pruefend = lauf.phase === "nachpruefen";
  const [liste, setListe] = useState<VerlaufEintrag[] | null>(null);
  const [suche, setSuche] = useState("");

  useEffect(() => {
    verlauf().then(setListe).catch(() => setListe([]));
  }, [lauf.ergebnis]);

  async function waehlen() {
    const ordner = await open({ directory: true, title: "Kopie mit ascmhl-Ordner wählen (z. B. A001R132)" });
    if (typeof ordner === "string") lauf.nachpruefen(ordner);
  }

  const gefiltert = (liste ?? []).filter((e) => !suche || e.karte.toLowerCase().includes(suche.toLowerCase()));

  return (
    <div className="pruefen">
      <section className="block" aria-labelledby="t-nach">
        <div className="block-kopf">
          <h2 id="t-nach">Kopie nachprüfen</h2>
          <button className="knopf knopf-haupt" onClick={waehlen} disabled={lauf.laeuft}>
            <ShieldCheck size={16} strokeWidth={1.75} aria-hidden /> Kopie wählen und prüfen …
          </button>
        </div>
        <p className="leise block-text">
          Liest jede Datei einer früheren Kopie ohne Cache und vergleicht sie mit dem ASC MHL, das beim Einlesen geschrieben
          wurde. Zum Beispiel vor dem Löschen einer Karte im Archiv oder nach einem Transport der Platte.
        </p>
        {pruefend && (
          <div className="nachpruefung">
            <Status ton="laeuft">
              Prüft · Datei {lauf.stand.pruefNummer} · <Pfad pfad={lauf.stand.pruefPfad || "…"} />
            </Status>
          </div>
        )}
        {!pruefend && lauf.nachpruefFehler && (
          <div className="nachpruefung">
            <Status ton="fehler">Nicht geprüft: {lauf.nachpruefFehler.text}</Status>
            <Pfad pfad={lauf.nachpruefFehler.ordner} className="unterzeile" />
          </div>
        )}
        {!pruefend && n && (
          <div className="nachpruefung">
            <Status ton={n.abweichungen.length === 0 ? "ok" : "rot"}>
              <strong>{n.abweichungen.length === 0 ? "Kopie unverändert" : `Kopie weicht ab (${n.abweichungen.length})`}</strong> ·{" "}
              {n.geprueft} Dateien gegen <span className="zahl">{n.generation}</span> geprüft
            </Status>
            <Pfad pfad={n.ordner} className="unterzeile" />
            {n.abweichungen.map((a) => (
              <span key={abweichungText(a)} className="unterzeile text-fehler">
                {abweichungText(a)}
              </span>
            ))}
          </div>
        )}
      </section>

      <section className="block block-fuellend" aria-labelledby="t-verlauf">
        <div className="block-kopf">
          <h2 id="t-verlauf">Verlauf</h2>
          <span className="leise">Alle Karten, die auf diesem Rechner eingelesen wurden</span>
          <input
            type="search"
            className="suche"
            aria-label="Karte suchen"
            placeholder="z. B. A001R132"
            value={suche}
            onChange={(ev) => setSuche(ev.target.value)}
          />
        </div>
        {liste === null ? null : gefiltert.length === 0 ? (
          <p className="leer-zeile">{liste.length ? "Keine Karte passt zur Suche." : "Noch keine Karte eingelesen."}</p>
        ) : (
          <table className="tabelle">
            <thead>
              <tr>
                <th>Karte</th>
                <th>Eingelesen</th>
                <th className="rechts">Dateien</th>
                <th className="rechts">Grösse</th>
                <th>Ziele</th>
                <th>Urteil</th>
                <th>Bericht</th>
              </tr>
            </thead>
            <tbody>
              {gefiltert.map((e) => {
                const bericht = e.ziele.find((z) => z.bericht)?.bericht;
                return (
                  <tr key={e.beginn + e.karte}>
                    <td className="zahl">{e.karte}</td>
                    <td className="zahl">{datumZeit(e.beginn)}</td>
                    <td className="rechts zahl">{e.dateien}</td>
                    <td className="rechts zahl">{bytesText(e.bytes)}</td>
                    <td className="zahl">
                      {e.ziele.filter((z) => z.gut).length} von {e.ziele.length} gut
                    </td>
                    <td>
                      <Status ton={e.sicher ? "ok" : "rot"} title={e.grund}>
                        {e.sicher ? "Sicher" : "Nicht freigegeben"}
                      </Status>
                    </td>
                    <td>
                      {bericht ? (
                        <button className="knopf knopf-klein" onClick={() => openPath(bericht)}>
                          <FileText size={14} strokeWidth={1.75} aria-hidden /> Öffnen
                        </button>
                      ) : (
                        <span className="leise">–</span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
