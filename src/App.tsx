// Gerüst: Kopfleiste mit den vier Seiten (Einlesen · Projekt · Prüfen · Einrichtung) und dem Stand der Verbindungen.
// Der laufende Vorgang liegt über den Seiten (lauf.tsx): ein Seitenwechsel unterbricht nie eine Kopie.
import { useEffect, useState } from "react";
import { FolderKanban, HardDriveDownload, Moon, Settings, ShieldCheck, Sun, SunMoon } from "lucide-react";
import { anwenden, gemerkteWahl, type Wahl } from "./thema";
import { Aktualisierung, useAktualisierung } from "./Aktualisierung";
import { EinstellungenGeben, useEinstellungen } from "./einstellungen";
import { KontoGeben, useKonto } from "./konto";
import { LaufGeben, useLauf } from "./lauf";
import { Status, zahl } from "./teile";
import { Einlesen } from "./seiten/Einlesen";
import { Projekt } from "./seiten/Projekt";
import { Pruefen } from "./seiten/Pruefen";
import { Einrichtung } from "./seiten/Einrichtung";

type Seite = "einlesen" | "projekt" | "pruefen" | "einrichtung";

const SEITEN: { id: Seite; text: string; Zeichen: typeof Settings }[] = [
  { id: "einlesen", text: "Einlesen", Zeichen: HardDriveDownload },
  { id: "projekt", text: "Projekt", Zeichen: FolderKanban },
  { id: "pruefen", text: "Prüfen", Zeichen: ShieldCheck },
  { id: "einrichtung", text: "Einrichtung", Zeichen: Settings },
];

export function App() {
  return (
    <EinstellungenGeben>
      <KontoGeben>
        <LaufGeben>
          <Gerust />
        </LaufGeben>
      </KontoGeben>
    </EinstellungenGeben>
  );
}

const WAHLEN: { wahl: Wahl; wort: string; Zeichen: typeof Sun }[] = [
  { wahl: "auto", wort: "Automatisch", Zeichen: SunMoon },
  { wahl: "tag", wort: "Tag", Zeichen: Sun },
  { wahl: "nacht", wort: "Nacht", Zeichen: Moon },
];

function Gerust() {
  const [seite, setSeite] = useState<Seite>("einlesen");
  const [wahl, setWahl] = useState<Wahl>(gemerkteWahl);
  useEffect(() => anwenden(wahl), [wahl]);
  const { update, pflicht, version } = useAktualisierung();
  const lauf = useLauf();
  const konto = useKonto();
  const e = useEinstellungen();

  const jetzt = WAHLEN.find((w) => w.wahl === wahl)!;
  const weiter = WAHLEN[(WAHLEN.indexOf(jetzt) + 1) % WAHLEN.length];
  const anteil = lauf.stand.bytes > 0 ? (lauf.stand.gelesen / lauf.stand.bytes) * 100 : 0;
  const zurEinrichtung = () => setSeite("einrichtung");

  return (
    <div className="app">
      <header className="kopf">
        <div className="marke">
          <span className="marke-name">Stage Ingest</span>
          <span className="marke-version zahl">{version}</span>
        </div>
        <nav className="seiten" aria-label="Seiten">
          {SEITEN.map(({ id, text, Zeichen }) => (
            <button key={id} className="seite" aria-current={seite === id ? "page" : undefined} onClick={() => setSeite(id)}>
              <Zeichen size={16} strokeWidth={1.75} aria-hidden />
              {text}
            </button>
          ))}
        </nav>
        <div className="kopf-stand">
          {lauf.laeuft && seite !== "einlesen" && lauf.phase !== "nachpruefen" && (
            <button className="kopf-lauf" onClick={() => setSeite("einlesen")}>
              <Status ton="laeuft">
                {lauf.phase === "kopieren" ? `Kopiert ${zahl(anteil, 0)} %` : "Liest zurück"} · {lauf.quelle?.name}
              </Status>
            </button>
          )}
          <button className="kopf-status" onClick={zurEinrichtung} title="Einrichtung öffnen">
            <Status
              ton={konto.verbindung === "verbunden" ? "ok" : konto.verbindung === "fehler" ? "fehler" : konto.verbindung === "verbindet" ? "laeuft" : "leise"}
            >
              Plate Assistant
            </Status>
          </button>
          {e.stageAdresse.trim() && (
            <button className="kopf-status" onClick={zurEinrichtung} title="Einrichtung öffnen">
              <Status ton={lauf.soll?.fehler ? "warn" : lauf.soll ? "ok" : "leise"}>Stage</Status>
            </button>
          )}
          {e.mindestKopien < 2 && (
            <button className="kopf-status" onClick={zurEinrichtung} title="Einrichtung öffnen">
              <Status ton="warn">Testschwelle: 1 Kopie</Status>
            </button>
          )}
          <button
            className="knopf-symbol"
            onClick={() => setWahl(weiter.wahl)}
            aria-label={`Darstellung: ${jetzt.wort}. Wechseln zu ${weiter.wort}`}
            title={`Darstellung: ${jetzt.wort}. Klick: ${weiter.wort}`}
          >
            <jetzt.Zeichen size={16} strokeWidth={1.75} />
          </button>
        </div>
      </header>
      <Aktualisierung update={update} pflicht={pflicht} laeuft={lauf.laeuft} />
      <div className="inhalt">
        {seite === "einlesen" && <Einlesen pflicht={pflicht} zurEinrichtung={zurEinrichtung} />}
        {seite === "projekt" && <Projekt zurEinrichtung={zurEinrichtung} />}
        {seite === "pruefen" && <Pruefen />}
        {seite === "einrichtung" && <Einrichtung wahl={wahl} setWahl={setWahl} />}
      </div>
    </div>
  );
}
