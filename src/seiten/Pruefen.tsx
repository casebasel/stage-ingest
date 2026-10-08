// Prüfen: eine bestehende Kopie jederzeit gegen ihr ASC MHL nachprüfen, und der Verlauf aller Karten dieses Rechners.
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { CopyPlus, FileText, FolderOpen, ShieldCheck } from "lucide-react";
import { bytesText, verlauf, type VerlaufEintrag } from "../kern";
import { useLauf } from "../lauf";
import { Pfad, Status, datumZeit } from "../teile";
import { abweichungText } from "./Einlesen";

export function Pruefen() {
  const lauf = useLauf();
  const n = lauf.nachpruefung;
  const pruefend = lauf.phase === "nachpruefen" && !lauf.kaskadeLaeuft;
  const [liste, setListe] = useState<VerlaufEintrag[] | null>(null);
  const [suche, setSuche] = useState("");
  // Kopie aus Kopie: vorhandene, geprüfte Kopie und Ziel (Ordner auf einer anderen Platte).
  const [quelleKopie, setQuelleKopie] = useState<string | null>(null);
  const [zielBasis, setZielBasis] = useState<string | null>(null);
  const kaskadeLaeuft = lauf.kaskadeLaeuft;
  async function kopieWaehlen() {
    const o = await open({ directory: true, title: "Vorhandene, geprüfte Kopie wählen (Kartenordner mit ascmhl, z. B. A004R132)" });
    if (typeof o === "string") setQuelleKopie(o);
  }
  async function zielWaehlen() {
    const o = await open({ directory: true, title: "Ziel auf einer anderen Platte wählen (z. B. NAS/Footage)" });
    if (typeof o === "string") setZielBasis(o);
  }

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

<section className="block" aria-labelledby="t-kaskade">
        <div className="block-kopf">
          <h2 id="t-kaskade">Fehlende Kopie aus einer Kopie erstellen</h2>
        </div>
        <p className="leise block-text">
          Wenn die Karte nicht mehr da ist: Eine geprüfte Kopie wird auf eine andere Platte kopiert und gegen die
          ursprünglichen Prüfsummen der Karte (ASC MHL) geprüft. Danach zählen beide Kopien für die Freigabe. Ist die Karte
          noch da, besser beim Einlesen „Kopie ergänzen“.
        </p>
        <div className="kaskade">
          <div className="kaskade-schritt">
            <button className="knopf" onClick={kopieWaehlen} disabled={lauf.laeuft}>
              <FolderOpen size={14} strokeWidth={1.75} aria-hidden /> Vorhandene Kopie …
            </button>
            {quelleKopie ? <Pfad pfad={quelleKopie} /> : <span className="leise">Kartenordner einer geprüften Kopie</span>}
          </div>
          <div className="kaskade-schritt">
            <button className="knopf" onClick={zielWaehlen} disabled={lauf.laeuft}>
              <FolderOpen size={14} strokeWidth={1.75} aria-hidden /> Ziel …
            </button>
            {zielBasis ? <Pfad pfad={zielBasis} /> : <span className="leise">Ordner auf einer anderen Platte, die Struktur kommt mit</span>}
          </div>
          <button
            className="knopf knopf-haupt"
            disabled={!quelleKopie || !zielBasis || lauf.laeuft}
            onClick={() => quelleKopie && zielBasis && lauf.kopieAusKopieStarten(quelleKopie, zielBasis)}
          >
            <CopyPlus size={16} strokeWidth={1.75} aria-hidden /> Kopie erstellen und prüfen
          </button>
        </div>
        {kaskadeLaeuft && (
          <div className="nachpruefung">
            <Status ton="laeuft">
              {lauf.stand.pruefNummer ? `Prüft · Datei ${lauf.stand.pruefNummer}` : "Kopiert"} · <Pfad pfad={lauf.stand.pruefPfad || lauf.stand.datei || "…"} />
            </Status>
          </div>
        )}
        {lauf.kaskadeFehler && (
          <div className="nachpruefung">
            <Status ton="fehler">Nicht erstellt: {lauf.kaskadeFehler}</Status>
          </div>
        )}
        {lauf.kaskade && (
          <div className="nachpruefung">
            <Status ton={lauf.kaskade.freigabe.sicher ? "ok" : "rot"}>
              <strong>{lauf.kaskade.freigabe.sicher ? "Sicher zum Formatieren" : "Nicht freigegeben"}</strong> ·{" "}
              {lauf.kaskade.freigabe.grund}
            </Status>
            {lauf.kaskade.urteile.map((u, i) => (
              <span key={u.ordner} className="unterzeile">
                <Status ton={!u.kopierfehler && u.abweichungen.length === 0 ? "ok" : "rot"}>
                  {i === 0 ? "Vorhandene Kopie" : "Neue Kopie"} · {lauf.kaskade!.kennungen[i]?.beschreibung} ·{" "}
                  {!u.kopierfehler && u.abweichungen.length === 0
                    ? `${u.geprueft} Dateien gegen die Karte geprüft`
                    : u.kopierfehler ?? `${u.abweichungen.length} Abweichungen`}
                </Status>
              </span>
            ))}
            {lauf.kaskade.freigabe.hinweise.map((h) => (
              <span key={h} className="unterzeile leise">
                {h}
              </span>
            ))}
            {lauf.kaskade.bericht && (
              <button className="knopf knopf-klein" onClick={() => openPath(lauf.kaskade!.bericht!)}>
                <FileText size={14} strokeWidth={1.75} aria-hidden /> Bericht öffnen
              </button>
            )}
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
                      {!e.sicher && e.ziele.some((z) => z.gut) && (
                        <button
                          className="verweis unterzeile"
                          onClick={() => {
                            setQuelleKopie(e.ziele.find((z) => z.gut)!.ordner);
                            document.getElementById("t-kaskade")?.scrollIntoView({ behavior: "smooth" });
                          }}
                        >
                          Kopie ergänzen
                        </button>
                      )}
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
