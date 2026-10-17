// Protokoll: alle eingelesenen Karten, je Karte die Kopien und je Datei der Beweis (Marlon, 17.10.2026: „einen Verlauf
// der Importvorgänge, wo man visuell sieht, was alles auf der Karte war und dass alles kopiert ist … und wohin … ein
// sicheres Gefühl“). Karten von allen erreichbaren Zielen, auch was ein anderer Rechner eingelesen hat. Der Haken je
// Datei kommt aus der ASC-MHL-Historie der Kopie (beim Einlesen ohne Cache zurückgelesen und mit der Prüfsumme der Karte
// verglichen) und aus dem Ordner (liegt mit gleicher Grösse da); „Nachprüfen“ liest alles nochmals. Die Dateien stehen
// als Liste wie im Finder (Marlon, 17.10.2026: „dass man wirklich sieht, es sieht aus wie ein Dateibaum“); von Hand
// gelöschte Clips bleiben grau darin stehen.
import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type ReactElement } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  ChevronDown,
  ChevronUp,
  ChevronsDownUp,
  ChevronsUpDown,
  CircleAlert,
  CircleCheck,
  CircleDashed,
  CircleX,
  CopyPlus,
  File,
  FileAudio,
  FileImage,
  FileText,
  FileVideo,
  Folder,
  FolderOpen,
  Play,
  RefreshCw,
  ShieldCheck,
  Trash2,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import {
  bytesText,
  historieBereinigen,
  protokollKarte,
  protokollListe,
  type DateiStand,
  type DateiZeile,
  type KopieDetail,
  type ProtokollDetail,
  type ProtokollKarte,
  type ProtokollKopie,
  type Vorgang,
} from "../kern";
import { useEinstellungen } from "../einstellungen";
import { useLauf } from "../lauf";
import { Pfad, Status, datumZeit, type Ton } from "../teile";
import { abweichungText } from "./Einlesen";

/** Je Stand: Signal, Zeichen, kurzes Wort für die Zelle, Erklärung für den Hinweis. */
const STAND: Record<DateiStand, { ton: Ton; zeichen: LucideIcon; wort: string; text: string }> = {
  geprueft: {
    ton: "ok",
    zeichen: CircleCheck,
    wort: "Geprüft",
    text: "Beim Einlesen ohne Cache zurückgelesen und mit der Karte verglichen; liegt mit gleicher Grösse da",
  },
  fehlt: { ton: "rot", zeichen: CircleX, wort: "Fehlt", text: "Fehlt auf dieser Kopie" },
  anders: { ton: "rot", zeichen: CircleX, wort: "Anders", text: "Andere Prüfsumme als auf der anderen Kopie" },
  fehlgeschlagen: { ton: "rot", zeichen: CircleX, wort: "Fehler", text: "Die Prüfung ist fehlgeschlagen" },
  nichtMehrDa: {
    ton: "rot",
    zeichen: CircleX,
    wort: "Nicht mehr da",
    text: "Geprüft, liegt aber nicht mehr da oder hat jetzt eine andere Grösse",
  },
  ungeprueft: {
    ton: "warn",
    zeichen: CircleAlert,
    wort: "Ungeprüft",
    text: "Liegt da, wurde aber nie gegen die Karte geprüft (kein Einlesen hat sie mit der Karte verglichen)",
  },
  geloescht: { ton: "leise", zeichen: Trash2, wort: "Gelöscht", text: "Von Hand gelöscht" },
  unbekannt: { ton: "leise", zeichen: CircleDashed, wort: "Offline", text: "Kopie nicht verbunden" },
};
/** Gilt als in Ordnung: geprüft oder von Hand gelöscht. */
const inOrdnung = (s: DateiStand) => s === "geprueft" || s === "geloescht";

/** Name einer Kopie: Ablage (Merkdatei), sonst das Laufwerk (`/Volumes/NAME`, `F:`), sonst der Ordner. */
function kopieName(k: ProtokollKopie): string {
  if (k.ablage) return k.ablage;
  const mac = k.ordner.match(/^\/Volumes\/([^/]+)/);
  if (mac) return mac[1];
  const win = k.ordner.match(/^([A-Za-z]:)[\\/]/);
  if (win) return win[1];
  return k.ordner.split(/[\\/]/).filter(Boolean)[0] ?? k.ordner;
}

const datumKurz = (iso: string) => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "" : d.toLocaleDateString("de-CH", { day: "2-digit", month: "2-digit", year: "numeric" });
};

/** „16.10.2026 01:05 von marlon“ aus einem Löschvermerk. */
const geloeschtText = (g: { zeit: string; benutzer: string }) => {
  const wer = g.benutzer.split(" auf ")[0];
  return `${g.zeit ? datumZeit(g.zeit) : "früher"}${wer ? ` von ${wer}` : ""}`;
};

export function Protokoll({ kopieErgaenzen }: { kopieErgaenzen: (ordner: string) => void }) {
  const e = useEinstellungen();
  const lauf = useLauf();
  const [liste, setListe] = useState<ProtokollKarte[] | null>(null);
  const [fehler, setFehler] = useState<string | null>(null);
  const [laedt, setLaedt] = useState(false);
  const [suche, setSuche] = useState("");
  const [gewaehlt, setGewaehlt] = useState<string | null>(null);
  // Nach einem Einlesen die neue Karte zeigen (sie steht oben).
  const letztesErgebnis = useRef(lauf.ergebnis);

  const laden = useCallback(async () => {
    setLaedt(true);
    setFehler(null);
    try {
      const l = await protokollListe(e.ziele);
      const neu = letztesErgebnis.current !== lauf.ergebnis;
      letztesErgebnis.current = lauf.ergebnis;
      setListe(l);
      setGewaehlt((g) => (!neu && g && l.some((k) => k.karte === g) ? g : (l[0]?.karte ?? null)));
    } catch (x) {
      setFehler(String(x));
    } finally {
      setLaedt(false);
    }
  }, [e.ziele, lauf.ergebnis]);
  useEffect(() => {
    void laden();
  }, [laden]);

  const gefiltert = (liste ?? []).filter((k) => !suche || k.karte.toLowerCase().includes(suche.toLowerCase()));
  const karte = liste?.find((k) => k.karte === gewaehlt) ?? null;

  return (
    <div className="protokoll">
      <nav className="baum protokoll-liste" aria-label="Eingelesene Karten">
        <div className="protokoll-suche">
          <input
            type="search"
            className="suche"
            aria-label="Karte suchen"
            placeholder="Karte suchen, z. B. A010"
            value={suche}
            onChange={(ev) => setSuche(ev.target.value)}
          />
          <button className="knopf-symbol" onClick={() => void laden()} disabled={laedt} title="Neu laden" aria-label="Neu laden">
            <RefreshCw size={15} strokeWidth={1.75} className={laedt ? "dreht" : undefined} />
          </button>
        </div>
        {fehler && <Status ton="fehler">{fehler}</Status>}
        {liste === null ? (
          <p className="leer-zeile">Sucht eingelesene Karten auf den Zielen …</p>
        ) : gefiltert.length === 0 ? (
          <p className="leer-zeile">
            {liste.length ? "Keine Karte passt zur Suche." : "Noch keine Karte gefunden. Ist das NAS oder die Platte verbunden?"}
          </p>
        ) : (
          <ul>
            {gefiltert.map((k) => {
              const da = k.kopien.filter((x) => x.erreichbar).length;
              return (
                <li key={k.karte}>
                  <button
                    className="baum-knoten protokoll-karte"
                    aria-current={k.karte === gewaehlt ? "true" : undefined}
                    onClick={() => setGewaehlt(k.karte)}
                  >
                    <span className="protokoll-karte-oben">
                      <span className="baum-name zahl">{k.karte}</span>
                      {k.sicher ? (
                        <span className="status-ok protokoll-zeichen" role="img" aria-label="Sicher" title="Sicher">
                          <CircleCheck size={15} strokeWidth={2} />
                        </span>
                      ) : (
                        <span className="status-rot protokoll-zeichen" role="img" aria-label="Nicht freigegeben" title="Nicht freigegeben">
                          <CircleX size={15} strokeWidth={2} />
                        </span>
                      )}
                    </span>
                    <span className="baum-info zahl">
                      {datumKurz(k.beginn)} · {k.dateien} Dateien · {k.kopien.length}{" "}
                      {k.kopien.length === 1 ? "Kopie" : "Kopien"}
                    </span>
                    {k.drehOrdner && <span className="baum-info">{k.drehOrdner}</span>}
                    {da < k.kopien.length && (
                      <span className="baum-info">
                        {k.kopien.length - da === 1 ? "1 Kopie" : `${k.kopien.length - da} Kopien`} nicht verbunden
                      </span>
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </nav>
      <section className="detail protokoll-detail" aria-label="Protokoll der Karte">
        {karte ? (
          <KarteDetail key={karte.karte} k={karte} kopieErgaenzen={kopieErgaenzen} />
        ) : (
          liste !== null && <p className="leer-zeile">Links eine Karte wählen.</p>
        )}
      </section>
    </div>
  );
}

function KarteDetail({ k, kopieErgaenzen }: { k: ProtokollKarte; kopieErgaenzen: (ordner: string) => void }) {
  const lauf = useLauf();
  const [d, setD] = useState<ProtokollDetail | null>(null);
  const [fehler, setFehler] = useState<string | null>(null);
  const [prueft, setPrueft] = useState<string | null>(null);
  const [bereinigt, setBereinigt] = useState<{ ordner: string; ton: Ton; text: string } | null>(null);
  const [neu, setNeu] = useState(0);
  useEffect(() => {
    let aktiv = true;
    setD(null);
    setFehler(null);
    protokollKarte(k.kopien.map((x) => x.ordner))
      .then((x) => aktiv && setD(x))
      .catch((x) => aktiv && setFehler(String(x)));
    return () => {
      aktiv = false;
    };
  }, [k, neu]);

  // Erreichbare Kopien mit lesbarer Historie zählen; je Kopie die geprüften Dateien.
  const lesbar = d ? d.kopien.map((x, i) => (x.erreichbar && !x.fehler ? i : -1)).filter((i) => i >= 0) : [];
  const vorhanden = d ? d.dateien.filter((z) => !z.geloescht || lesbar.some((i) => z.jeKopie[i] !== "geloescht")) : [];
  const geloescht = d ? d.dateien.length - vorhanden.length : 0;
  const geprueftJe = d ? d.kopien.map((_, i) => vorhanden.filter((z) => z.jeKopie[i] === "geprueft").length) : [];
  const auffaellig = vorhanden.filter((z) => lesbar.some((i) => !inOrdnung(z.jeKopie[i])));
  const rot = auffaellig.filter((z) => lesbar.some((i) => STAND[z.jeKopie[i]].ton === "rot"));
  const ungeprueft = auffaellig.filter((z) => lesbar.some((i) => z.jeKopie[i] === "ungeprueft")).length;
  const unsauber = d ? d.kopien.filter((x) => x.nenntGeloeschte.length > 0 || x.ausserhalb.length > 0) : [];
  const gute = k.kopien.find((x) => x.erreichbar && x.gut !== false);
  const urteil = !d || lesbar.length === 0 ? "leise" : rot.length ? "rot" : auffaellig.length || unsauber.length ? "warn" : "ok";

  async function bereinigen(x: ProtokollKopie, det: KopieDetail) {
    const teile = [
      det.nenntGeloeschte.length > 0 &&
        `Die ASC-MHL-Historie nennt ${det.nenntGeloeschte.length === 1 ? "eine Datei" : `${det.nenntGeloeschte.length} Dateien`} noch, die von Hand gelöscht ${det.nenntGeloeschte.length === 1 ? "ist" : "sind"}:\n${det.nenntGeloeschte.join("\n")}`,
      det.ausserhalb.length > 0 &&
        `Im Ordner ascmhl liegt eine Generation, die nicht zur Kette gehört (zwei Löschvorgänge gleichzeitig):\n${det.ausserhalb.join("\n")}`,
    ].filter(Boolean);
    const was =
      det.nenntGeloeschte.length > 0
        ? "Bereinigen legt die alte Historie nach 05_METADATEN/frueher und schreibt eine neue über die Dateien, die daliegen, mit den Prüfsummen aus der alten."
        : "Bereinigen legt diese Generation nach 05_METADATEN/frueher; die Kette bleibt, wie sie ist.";
    const ja = await ask(`${teile.join("\n\n")}\n\n${was} Gelöscht wird nichts. Ein Protokoll kommt nach 04_BERICHTE.`, {
      title: `Historie von ${kopieName(x)} bereinigen`,
      kind: "info",
      okLabel: "Bereinigen",
      cancelLabel: "Abbrechen",
    });
    if (!ja) return;
    setBereinigt({ ordner: x.ordner, ton: "laeuft", text: "Bereinigt …" });
    try {
      const b = await historieBereinigen(x.ordner);
      setBereinigt({
        ordner: x.ordner,
        ton: "ok",
        text: b.generation ? "Bereinigt: neue Historie geschrieben" : b.ausserhalb.length ? "Bereinigt: Generation beiseitegelegt" : "Nichts zu bereinigen",
      });
      setNeu((n) => n + 1);
    } catch (e) {
      setBereinigt({ ordner: x.ordner, ton: "fehler", text: String(e) });
    }
  }

  return (
    <div className="protokoll-karte-detail">
      <div className="detail-kopf">
        <h2 className="zahl">{k.karte}</h2>
        <Status ton={k.sicher ? "ok" : "rot"} title={k.grund}>
          {k.sicher ? "Sicher zum Formatieren" : "Nicht freigegeben"}
        </Status>
        <span className="leise">
          {d ? vorhanden.length : k.dateien} Dateien · {bytesText(d ? vorhanden.reduce((s, z) => s + z.groesse, 0) : k.bytes)} ·
          zuletzt eingelesen {datumZeit(k.beginn)}
          {k.projekt && ` · ${k.projekt}`}
          {k.drehOrdner && ` / ${k.drehOrdner}`}
        </span>
        {!k.sicher && gute && (
          <button className="knopf knopf-klein" style={{ marginLeft: "auto" }} onClick={() => kopieErgaenzen(gute.ordner)}>
            <CopyPlus size={14} strokeWidth={1.75} aria-hidden /> Kopie ergänzen
          </button>
        )}
      </div>

      <div className={`protokoll-urteil protokoll-urteil-${urteil}`}>
        {fehler ? (
          <Status ton="fehler">{fehler}</Status>
        ) : !d ? (
          <Status ton="laeuft">Liest die Historie der Kopien …</Status>
        ) : lesbar.length === 0 ? (
          <Status ton="leise">Keine Kopie verbunden: NAS oder Platte anschliessen, dann neu laden.</Status>
        ) : auffaellig.length === 0 ? (
          <Status ton="ok">
            <strong>
              Alle {vorhanden.length} Dateien der Karte auf {lesbar.length} {lesbar.length === 1 ? "Kopie" : "Kopien"} geprüft
            </strong>
            {geloescht > 0 && ` · ${geloescht} von Hand gelöscht`}
            {lesbar.length < d.kopien.length && ` · ${d.kopien.length - lesbar.length} nicht verbunden`}
          </Status>
        ) : (
          <Status ton={rot.length ? "rot" : "warn"}>
            <strong>
              {auffaellig.length} von {vorhanden.length} Dateien nicht auf jeder Kopie geprüft
            </strong>
            {ungeprueft > 0 && ` · ${ungeprueft} nie gegen die Karte geprüft`}
            {geloescht > 0 && ` · ${geloescht} von Hand gelöscht`}
          </Status>
        )}
        {d && unsauber.length > 0 && (
          <span className="unterzeile">
            <Status ton="warn">
              Die ASC-MHL-Historie auf {unsauber.map((x) => kopieName(k.kopien[d.kopien.indexOf(x)])).join(", ")} ist nach dem
              Löschen nicht sauber: bei den Kopien „Bereinigen“
            </Status>
          </span>
        )}
        {k.grund && <span className="unterzeile leise">{k.grund}</span>}
      </div>

      <section className="protokoll-abschnitt" aria-labelledby="t-kopien">
        <h3 id="t-kopien">Kopien</h3>
        <table className="tabelle">
          <thead>
            <tr>
              <th>Kopie</th>
              <th>Stand</th>
              <th>Eingelesen</th>
              <th className="spalte-aktion">
                <span className="unsichtbar">Aktionen</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {k.kopien.map((x, i) => {
              const det = d?.kopien[i];
              const eingelesen = d?.verlauf.find((v) => v.kopie === i && v.art === "eingelesen");
              const nachgeprueft = d?.verlauf.find((v) => v.kopie === i && v.art === "nachgeprueft");
              const ergebnis = lauf.nachpruefung?.ordner === x.ordner ? lauf.nachpruefung : null;
              const nachFehler = lauf.nachpruefFehler?.ordner === x.ordner ? lauf.nachpruefFehler : null;
              const aufzuraeumen = det && (det.nenntGeloeschte.length > 0 || det.ausserhalb.length > 0);
              const hier = vorhanden.filter((z) => z.jeKopie[i] !== "geloescht").length;
              return (
                <tr key={x.ordner}>
                  <td className="protokoll-kopie">
                    <strong>{kopieName(x)}</strong>
                    <span className="unterzeile">
                      <Pfad pfad={x.ordner} />
                    </span>
                  </td>
                  <td>
                    {!x.erreichbar ? (
                      <Status ton="leise">Nicht verbunden</Status>
                    ) : det?.fehler ? (
                      <Status ton="rot">{det.fehler}</Status>
                    ) : d ? (
                      <Status
                        ton={
                          geprueftJe[i] === hier ? "ok" : vorhanden.some((z) => STAND[z.jeKopie[i]].ton === "rot") ? "rot" : "warn"
                        }
                      >
                        {geprueftJe[i]} von {hier} Dateien geprüft
                      </Status>
                    ) : (
                      <Status ton="laeuft">Liest …</Status>
                    )}
                    {det && det.nenntGeloeschte.length > 0 && (
                      <span className="unterzeile">
                        <Status ton="warn">
                          Historie nennt {det.nenntGeloeschte.length === 1 ? "eine gelöschte Datei" : `${det.nenntGeloeschte.length} gelöschte Dateien`} noch
                        </Status>
                      </span>
                    )}
                    {det && det.ausserhalb.length > 0 && (
                      <span className="unterzeile">
                        <Status ton="warn">
                          {det.ausserhalb.length === 1 ? "Eine Generation" : `${det.ausserhalb.length} Generationen`} ausserhalb der Kette
                        </Status>
                      </span>
                    )}
                    {bereinigt?.ordner === x.ordner && (
                      <span className="unterzeile">
                        <Status ton={bereinigt.ton}>{bereinigt.text}</Status>
                      </span>
                    )}
                    {x.gut === false && (
                      <span className="unterzeile">
                        <Status ton="rot">Beim Einlesen nicht gut</Status>
                      </span>
                    )}
                    {prueft === x.ordner && lauf.phase === "nachpruefen" && (
                      <span className="unterzeile">
                        <Status ton="laeuft">Prüft nach · Datei {lauf.stand.pruefNummer}</Status>
                      </span>
                    )}
                    {ergebnis && (
                      <span className="unterzeile">
                        <Status ton={ergebnis.abweichungen.length === 0 ? "ok" : "rot"}>
                          {ergebnis.abweichungen.length === 0
                            ? `Eben nachgeprüft: ${ergebnis.geprueft} Dateien unverändert`
                            : `Eben nachgeprüft: ${ergebnis.abweichungen.length} Abweichungen`}
                        </Status>
                      </span>
                    )}
                    {ergebnis?.abweichungen.map((a) => (
                      <span key={abweichungText(a)} className="unterzeile text-fehler">
                        {abweichungText(a)}
                      </span>
                    ))}
                    {nachFehler && (
                      <span className="unterzeile">
                        <Status ton="fehler">{nachFehler.text}</Status>
                      </span>
                    )}
                  </td>
                  <td>
                    {eingelesen ? <VorgangWer v={eingelesen} /> : <span className="leise">–</span>}
                    {nachgeprueft && (
                      <span className="unterzeile leise">
                        <span>
                          nachgeprüft <span className="zahl">{datumZeit(nachgeprueft.zeit)}</span> · {nachgeprueft.rechner}
                        </span>
                      </span>
                    )}
                  </td>
                  <td className="spalte-aktion">
                    <span className="aktion-reihe">
                      <button className="knopf-symbol" disabled={!x.erreichbar} onClick={() => openPath(x.ordner)} title="Ordner öffnen" aria-label={`Ordner von ${kopieName(x)} öffnen`}>
                        <FolderOpen size={15} strokeWidth={1.75} />
                      </button>
                      <button className="knopf-symbol" disabled={!x.bericht} onClick={() => x.bericht && openPath(x.bericht)} title="Bericht öffnen" aria-label={`Bericht von ${kopieName(x)} öffnen`}>
                        <FileText size={15} strokeWidth={1.75} />
                      </button>
                      {aufzuraeumen ? (
                        <button
                          className="knopf knopf-klein"
                          disabled={lauf.laeuft || bereinigt?.ton === "laeuft"}
                          onClick={() => det && void bereinigen(x, det)}
                          title="Die ASC-MHL-Historie dieser Kopie nach dem Löschen in Ordnung bringen; gelöscht wird nichts"
                        >
                          <Wrench size={14} strokeWidth={1.75} aria-hidden /> Bereinigen
                        </button>
                      ) : (
                        <button
                          className="knopf knopf-klein"
                          disabled={!x.erreichbar || lauf.laeuft}
                          onClick={() => {
                            setPrueft(x.ordner);
                            void lauf.nachpruefen(x.ordner);
                          }}
                          title="Jede Datei dieser Kopie nochmals ohne Cache lesen und mit dem ASC MHL vergleichen"
                        >
                          <ShieldCheck size={14} strokeWidth={1.75} aria-hidden /> Nachprüfen
                        </button>
                      )}
                    </span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </section>

      {d && <Laeufe d={d} k={k} />}

      {d && d.dateien.length > 0 && <Dateibaum d={d} k={k} geloescht={geloescht} anzahl={vorhanden.length} />}
    </div>
  );
}

/** Zeit, Rechner und Werkzeug eines Vorgangs. */
function VorgangWer({ v }: { v: Vorgang }) {
  return (
    <>
      <span className="zahl">{datumZeit(v.zeit)}</span>
      <span className="unterzeile leise">
        <span>
          {v.rechner}
          {v.werkzeug && ` · ${v.werkzeug}`}
        </span>
      </span>
    </>
  );
}

const VORGANG: Record<Vorgang["art"], string> = {
  eingelesen: "Eingelesen und geprüft",
  nachgeprueft: "Nachgeprüft",
  geloescht: "Von Hand gelöscht",
  bereinigt: "Historie bereinigt",
};

/** Verlauf aus den Historien (auch beiseitegelegten) und den Lösch- und Bereinigungsprotokollen aller Kopien. */
function Laeufe({ d, k }: { d: ProtokollDetail; k: ProtokollKarte }) {
  const laeufe = useMemo(() => {
    const je = new Map<string, { v: Vorgang; kopien: string[] }>();
    for (const v of d.verlauf) {
      // Dieselbe Minute, derselbe Rechner, dieselbe Art, derselbe Clip: ein Vorgang über mehrere Kopien.
      const schluessel = `${datumZeit(v.zeit)}|${v.rechner}|${v.art}|${v.clip ?? ""}`;
      const e = je.get(schluessel) ?? { v, kopien: [] };
      const name = kopieName(k.kopien[v.kopie]);
      if (!e.kopien.includes(name)) e.kopien.push(name);
      je.set(schluessel, e);
    }
    return [...je.values()];
  }, [d, k]);
  if (laeufe.length === 0) return null;
  return (
    <section className="protokoll-abschnitt" aria-labelledby="t-laeufe">
      <h3 id="t-laeufe">Verlauf</h3>
      <table className="tabelle">
        <thead>
          <tr>
            <th>Zeit</th>
            <th>Vorgang</th>
            <th>Rechner</th>
            <th className="rechts">Dateien</th>
            <th>Kopien</th>
          </tr>
        </thead>
        <tbody>
          {laeufe.map(({ v, kopien }) => (
            <tr key={`${v.zeit}|${v.rechner}|${v.art}|${v.clip ?? ""}`}>
              <td className="zahl">{datumZeit(v.zeit)}</td>
              <td>
                {v.art === "geloescht" ? (
                  <>
                    <span className="zahl">{v.clip}</span> von Hand gelöscht
                  </>
                ) : (
                  VORGANG[v.art]
                )}
                {(v.art === "geloescht" || v.art === "bereinigt") && (
                  <span className="unterzeile leise">
                    {v.historieNeu
                      ? `Neue ASC-MHL-Historie über ${v.dateien ?? "die verbleibenden"} Dateien`
                      : v.art === "geloescht"
                        ? "Historie dabei nicht erneuert"
                        : "Generation ausserhalb der Kette beiseitegelegt"}
                  </span>
                )}
              </td>
              <td className="zahl">
                {v.rechner}
                <span className="unterzeile leise">
                  {v.benutzer ? `${v.benutzer} · ` : ""}
                  {v.werkzeug}
                </span>
              </td>
              <td className="rechts zahl">{v.art === "eingelesen" || v.art === "nachgeprueft" ? v.dateien : "–"}</td>
              <td>{kopien.join(", ")}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

type Knoten = { name: string; pfad: string; dateien: DateiZeile[]; kinder: Knoten[]; alle: DateiZeile[] };

function baumBauen(dateien: DateiZeile[]): Knoten {
  const wurzel: Knoten = { name: "", pfad: "", dateien: [], kinder: [], alle: [] };
  for (const z of dateien) {
    const teile = z.pfad.split("/");
    let k = wurzel;
    k.alle.push(z);
    for (let i = 0; i < teile.length - 1; i++) {
      const pfad = teile.slice(0, i + 1).join("/");
      let kind = k.kinder.find((c) => c.pfad === pfad);
      if (!kind) {
        kind = { name: teile[i], pfad, dateien: [], kinder: [], alle: [] };
        k.kinder.push(kind);
      }
      kind.alle.push(z);
      k = kind;
    }
    k.dateien.push(z);
  }
  return wurzel;
}

const name = (z: DateiZeile) => z.pfad.split("/").pop() ?? z.pfad;
const endung = (n: string) => {
  const i = n.lastIndexOf(".");
  return i > 0 ? n.slice(i + 1).toLowerCase() : "";
};
const ARTEN: Record<string, string> = {
  mxf: "MXF-Video",
  mov: "QuickTime-Film",
  mp4: "MPEG-4-Film",
  braw: "Blackmagic RAW",
  r3d: "REDCODE RAW",
  crm: "Canon RAW",
  ari: "ARRIRAW-Bild",
  arx: "ARRIRAW-HDE-Bild",
  dng: "DNG-Bild",
  exr: "OpenEXR-Bild",
  jpg: "JPEG-Bild",
  jpeg: "JPEG-Bild",
  png: "PNG-Bild",
  tif: "TIFF-Bild",
  tiff: "TIFF-Bild",
  wav: "WAV-Audio",
  bwf: "BWF-Audio",
  ale: "ALE-Liste",
  csv: "CSV-Tabelle",
  xml: "XML-Datei",
  json: "JSON-Datei",
  txt: "Text",
  pdf: "PDF-Dokument",
};
const artVon = (n: string) => {
  const e = endung(n);
  return ARTEN[e] ?? (e ? `${e.toUpperCase()}-Datei` : "Datei");
};
const FILM = new Set(["mxf", "mov", "mp4", "braw", "r3d", "crm"]);
const BILD = new Set(["ari", "arx", "dng", "exr", "jpg", "jpeg", "png", "tif", "tiff"]);
const TEXT = new Set(["ale", "csv", "xml", "json", "txt", "mhl"]);
function dateiZeichen(n: string): LucideIcon {
  const e = endung(n);
  if (FILM.has(e)) return FileVideo;
  if (BILD.has(e)) return FileImage;
  if (e === "wav" || e === "bwf") return FileAudio;
  if (TEXT.has(e)) return FileText;
  return File;
}

type Spalte = "name" | "geaendert" | "groesse" | "art";
const zeitwert = (iso: string) => {
  const t = Date.parse(iso);
  return Number.isNaN(t) ? 0 : t;
};

/** Dateien der Karte als Liste wie im Finder: Ordner mit Dreieck zum Aufklappen, je Datei Änderungszeit, Grösse, Art und
 * je Kopie der Stand mit Zeichen und Wort. Ordner mit vielen Dateien (ARRIRAW-Bilder) sind zu. */
function Dateibaum({ d, k, geloescht, anzahl }: { d: ProtokollDetail; k: ProtokollKarte; geloescht: number; anzahl: number }) {
  const wurzel = useMemo(() => baumBauen(d.dateien), [d]);
  const alleOrdner = useMemo(() => {
    const aus: Knoten[] = [];
    const gehen = (n: Knoten) => n.kinder.forEach((c) => (aus.push(c), gehen(c)));
    gehen(wurzel);
    return aus;
  }, [wurzel]);
  const [offen, setOffen] = useState<Set<string>>(() => {
    const s = new Set<string>();
    const gehen = (n: Knoten, tiefe: number) => {
      for (const c of n.kinder) {
        if (tiefe === 0 || c.alle.length <= 40) s.add(c.pfad);
        gehen(c, tiefe + 1);
      }
    };
    gehen(wurzel, 0);
    return s;
  });
  const [sortierung, setSortierung] = useState<{ spalte: Spalte; ab: boolean }>({ spalte: "name", ab: false });
  const umschalten = (p: string) =>
    setOffen((s) => {
      const n = new Set(s);
      if (n.has(p)) n.delete(p);
      else n.add(p);
      return n;
    });
  const sortieren = (spalte: Spalte) =>
    setSortierung((s) => (s.spalte === spalte ? { spalte, ab: !s.ab } : { spalte, ab: spalte === "geaendert" || spalte === "groesse" }));

  const vergleich = (a: DateiZeile, b: DateiZeile) => {
    let r = 0;
    if (sortierung.spalte === "geaendert") r = zeitwert(a.geaendert) - zeitwert(b.geaendert);
    else if (sortierung.spalte === "groesse") r = a.groesse - b.groesse;
    else if (sortierung.spalte === "art") r = artVon(name(a)).localeCompare(artVon(name(b)), "de");
    if (r === 0) r = name(a).localeCompare(name(b), "de", { numeric: true });
    return sortierung.ab ? -r : r;
  };
  const ordnerFolge = (a: Knoten, b: Knoten) => {
    const r = a.name.localeCompare(b.name, "de", { numeric: true });
    return sortierung.spalte === "name" && sortierung.ab ? -r : r;
  };

  const zeilen: ReactElement[] = [];
  const gehen = (n: Knoten, tiefe: number) => {
    for (const c of [...n.kinder].sort(ordnerFolge)) {
      const auf = offen.has(c.pfad);
      const da = c.alle.filter((z) => !z.geloescht);
      zeilen.push(
        <tr key={`o:${c.pfad}`} className="finder-ordner">
          <td className="finder-name">
            <button
              className="finder-auf"
              style={{ paddingLeft: 6 + tiefe * 16 }}
              onClick={() => umschalten(c.pfad)}
              aria-expanded={auf}
              title={c.pfad}
            >
              <Play size={9} strokeWidth={2} fill="currentColor" className="finder-dreieck" aria-hidden />
              <Folder size={16} strokeWidth={1.6} className="finder-symbol finder-symbol-ordner" aria-hidden />
              <span className="finder-text">{c.name}</span>
            </button>
          </td>
          <td className="finder-zahl leise">–</td>
          <td className="finder-zahl leise">{bytesText(da.reduce((s, z) => s + z.groesse, 0))}</td>
          <td className="finder-art leise">
            Ordner · {da.length} {da.length === 1 ? "Datei" : "Dateien"}
          </td>
          {d.kopien.map((_, i) => (
            <td key={i} className="finder-stand-zelle">
              <OrdnerStand dateien={c.alle} i={i} />
            </td>
          ))}
        </tr>,
      );
      if (auf) gehen(c, tiefe + 1);
    }
    for (const z of [...n.dateien].sort(vergleich)) {
      const n2 = name(z);
      const Z = dateiZeichen(n2);
      const titel = [z.pfad, z.xxh128 && `XXH3-128 ${z.xxh128}`, z.geloescht && `Von Hand gelöscht ${geloeschtText(z.geloescht)}`]
        .filter(Boolean)
        .join("\n");
      zeilen.push(
        <tr key={z.pfad} className={z.geloescht && z.jeKopie.every((s) => s === "geloescht" || s === "unbekannt") ? "finder-weg" : undefined}>
          <td className="finder-name" title={titel}>
            <span className="finder-datei" style={{ paddingLeft: 6 + tiefe * 16 + 13 }}>
              <Z size={16} strokeWidth={1.6} className="finder-symbol" aria-hidden />
              <span className="finder-text">{n2}</span>
            </span>
          </td>
          <td className="finder-zahl">{z.geaendert ? datumZeit(z.geaendert) : "–"}</td>
          <td className="finder-zahl">{z.groesse > 0 ? bytesText(z.groesse) : "–"}</td>
          <td className="finder-art">{artVon(n2)}</td>
          {z.jeKopie.map((s, i) => (
            <td key={i} className="finder-stand-zelle">
              <StandZelle s={s} geloescht={z.geloescht} />
            </td>
          ))}
        </tr>,
      );
    }
  };
  gehen(wurzel, 0);

  const kopf = (spalte: Spalte, text: string, klasse = "") => {
    const aktiv = sortierung.spalte === spalte;
    const Pfeil = sortierung.ab ? ChevronDown : ChevronUp;
    return (
      <th className={klasse} aria-sort={aktiv ? (sortierung.ab ? "descending" : "ascending") : undefined}>
        <button className="finder-kopf" onClick={() => sortieren(spalte)}>
          {text}
          {aktiv && <Pfeil size={12} strokeWidth={2} aria-hidden />}
        </button>
      </th>
    );
  };

  return (
    <section className="protokoll-abschnitt" aria-labelledby="t-dateien">
      <h3 id="t-dateien">
        Dateien auf der Karte <span className="leise zahl">{anzahl}</span>
        {geloescht > 0 && <span className="leise">· {geloescht} von Hand gelöscht</span>}
        <span className="finder-knoepfe">
          <button className="knopf-symbol" onClick={() => setOffen(new Set(alleOrdner.map((o) => o.pfad)))} title="Alle Ordner aufklappen" aria-label="Alle Ordner aufklappen">
            <ChevronsUpDown size={15} strokeWidth={1.75} />
          </button>
          <button className="knopf-symbol" onClick={() => setOffen(new Set())} title="Alle Ordner zuklappen" aria-label="Alle Ordner zuklappen">
            <ChevronsDownUp size={15} strokeWidth={1.75} />
          </button>
        </span>
      </h3>
      <div className="finder-rahmen">
        <table className="finder">
          <thead>
            <tr>
              {kopf("name", "Name", "finder-name")}
              {kopf("geaendert", "Geändert", "finder-zahl")}
              {kopf("groesse", "Grösse", "finder-zahl")}
              {kopf("art", "Art")}
              {k.kopien.map((x) => (
                <th key={x.ordner} className="finder-stand-zelle" title={x.ordner}>
                  <span className="finder-kopf-text">{kopieName(x)}</span>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {zeilen.map((z, i) => (
              <Fragment key={i}>{z}</Fragment>
            ))}
          </tbody>
        </table>
      </div>
      <p className="leise protokoll-erklaerung">
        <span>
          Geprüft heisst: beim Einlesen ohne Cache zurückgelesen und mit der Prüfsumme der Karte (XXH3-128) verglichen,
          festgehalten in der ASC-MHL-Historie der Kopie, und die Datei liegt heute mit gleicher Grösse da. Ungeprüft: liegt
          da, aber kein Einlesen hat sie mit der Karte verglichen. „Nachprüfen“ liest jede Datei nochmals.
        </span>
      </p>
    </section>
  );
}

function StandZelle({ s, geloescht }: { s: DateiStand; geloescht?: { zeit: string; benutzer: string } | null }) {
  const { ton, zeichen: Z, wort, text } = STAND[s];
  const titel = s === "geloescht" && geloescht ? `${text} ${geloeschtText(geloescht)}` : text;
  return (
    <span className={`status status-${ton} finder-stand`} title={titel}>
      <Z size={14} strokeWidth={2} aria-hidden />
      <span>{wort}</span>
    </span>
  );
}

/** Zusammengefasst für einen Ordner: alles geprüft, oder wie viele zu klären sind. */
function OrdnerStand({ dateien, i }: { dateien: DateiZeile[]; i: number }) {
  if (dateien.every((z) => z.jeKopie[i] === "unbekannt")) return <StandZelle s="unbekannt" />;
  const offen = dateien.filter((z) => !inOrdnung(z.jeKopie[i]) && z.jeKopie[i] !== "unbekannt");
  if (offen.length === 0) {
    if (dateien.every((z) => z.jeKopie[i] === "geloescht")) return <StandZelle s="geloescht" />;
    return <StandZelle s="geprueft" />;
  }
  const rot = offen.some((z) => STAND[z.jeKopie[i]].ton === "rot");
  const Z = rot ? CircleX : CircleAlert;
  return (
    <span className={`status status-${rot ? "rot" : "warn"} finder-stand`} title={`${offen.length} Dateien nicht geprüft`}>
      <Z size={14} strokeWidth={2} aria-hidden />
      <span>{offen.length} zu klären</span>
    </span>
  );
}
