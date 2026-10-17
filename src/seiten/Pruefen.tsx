// Prüfen: eine bestehende Kopie jederzeit gegen ihr ASC MHL nachprüfen, und eine fehlende Kopie aus einer Kopie
// erstellen. Der Verlauf der eingelesenen Karten steht unter „Protokoll“.
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { CopyPlus, FileText, FolderOpen, ShieldCheck } from "lucide-react";

import { useLauf } from "../lauf";
import { Pfad, Status } from "../teile";
import { abweichungText } from "./Einlesen";

export function Pruefen({ vorschlag }: { vorschlag: { ordner: string } | null }) {
  const lauf = useLauf();
  const n = lauf.nachpruefung;
  const pruefend = lauf.phase === "nachpruefen" && !lauf.kaskadeLaeuft;
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

  // Aus dem Protokoll: „Fehlende Kopie erstellen“ mit der vorhandenen Kopie als Quelle.
  useEffect(() => {
    if (!vorschlag) return;
    setQuelleKopie(vorschlag.ordner);
    document.getElementById("t-kaskade")?.scrollIntoView({ behavior: "smooth" });
  }, [vorschlag]);

  async function waehlen() {
    const ordner = await open({ directory: true, title: "Kopie mit ascmhl-Ordner wählen (z. B. A001R132)" });
    if (typeof ordner === "string") lauf.nachpruefen(ordner);
  }


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
          <h2 id="t-kaskade">Fehlende Kopie aus einer Kopie erstellen (Kaskade)</h2>
        </div>
        <p className="leise block-text">
          Wenn die Karte nicht mehr da ist: Eine geprüfte Kopie wird auf eine andere Platte kopiert und gegen die
          ursprünglichen Prüfsummen der Karte (ASC MHL) geprüft, in anderen Programmen „Cascading Copy“. Danach zählen
          beide Kopien für die Freigabe. Ist die Karte noch da, besser beim Einlesen „Kopie ergänzen“.
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

    </div>
  );
}
