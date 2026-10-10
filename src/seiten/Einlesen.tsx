// Einlesen: links die Quellen (eingesteckte Karten werden erkannt und angeboten), in der Mitte der Auftrag
// (Karte → Ziele → Dreh → Einlesen), während des Laufs der Fortschritt pro Ziel, danach das Urteil über die ganze Breite.
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import {
  ArrowRight,
  ChevronRight,
  CircleCheck,
  CircleX,
  ArrowUpFromLine as Eject,
  FileText,
  FolderOpen,
  HardDrive,
  MemoryStick,
  Network,
  Plus,
  RotateCcw,
  TriangleAlert,
  X,
} from "lucide-react";
import {
  auswerfen,
  bytesText,
  laufwerke as laufwerkeHolen,
  zielGeraete,
  type Abweichung,
  type KartenErgebnis,
  type Laufwerk,
  type ZielGeraet,
} from "../kern";
import { useEinstellungen } from "../einstellungen";
import { drehsVon, useKonto } from "../konto";
import { useLauf, type Quelle } from "../lauf";
import { Pfad, Status, dauerText, name, zahl, type Ton } from "../teile";

const ALLE_MS = 3000;

export function Einlesen({ pflicht, zurEinrichtung }: { pflicht: boolean; zurEinrichtung: () => void }) {
  const lauf = useLauf();
  const e = useEinstellungen();

  // Eingesteckte Laufwerke alle paar Sekunden (nicht während des Kopierens: dann zählt nur die Kopie).
  const [laufwerke, setLaufwerke] = useState<Laufwerk[]>([]);
  useEffect(() => {
    if (lauf.laeuft) return;
    let aktuell = true;
    const holen = () => laufwerkeHolen().then((l) => aktuell && setLaufwerke(l)).catch(() => {});
    holen();
    const t = setInterval(holen, ALLE_MS);
    return () => {
      aktuell = false;
      clearInterval(t);
    };
  }, [lauf.laeuft]);

  // Eine neu erkannte Karte wird angeboten, nie von selbst gewählt (Marlon: „Ja, aber erst nachfragen“).
  const [abgelehnt, setAbgelehnt] = useState<string[]>([]);
  const angebot =
    !lauf.quelle && !lauf.laeuft
      ? laufwerke.find(
          (l) => l.karte && !abgelehnt.includes(l.pfad) && l.pfad !== lauf.letzteQuelle?.pfad && !istZiel(l, e.ziele),
        )
      : undefined;

  return (
    <div className="einlesen">
      <Quellen laufwerke={laufwerke} zielBasis={e.ziele} />
      <main className="arbeit">
        {angebot && (
          <div className="angebot" role="status">
            <MemoryStick size={18} strokeWidth={1.75} aria-hidden />
            <span>
              <strong>{angebot.name}</strong> erkannt · {angebot.karte!.kamera} · {angebot.karte!.clips} Clips ·{" "}
              {bytesText(angebot.karte!.bytes)}. Einlesen?
            </span>
            <span className="angebot-knoepfe">
              <button className="knopf knopf-haupt" onClick={() => lauf.quelleWaehlen(alsQuelle(angebot))}>
                Diese Karte wählen
              </button>
              <button className="knopf" onClick={() => setAbgelehnt([...abgelehnt, angebot.pfad])}>
                Nicht jetzt
              </button>
            </span>
          </div>
        )}
        {lauf.laeuft && lauf.phase !== "nachpruefen" ? (
          <Fortschritt />
        ) : lauf.ergebnis && !lauf.quelle ? (
          <Urteil ergebnis={lauf.ergebnis} laufwerke={laufwerke} />
        ) : (
          <Auftrag pflicht={pflicht} zurEinrichtung={zurEinrichtung} laufwerke={laufwerke} />
        )}
      </main>
    </div>
  );
}

const alsQuelle = (l: Laufwerk): Quelle => ({
  pfad: l.pfad,
  name: l.name,
  laufwerk: true,
  kamera: l.karte?.kamera,
  clips: l.karte?.clips,
  bytes: l.karte?.bytes,
});

const ohneEnde = (p: string) => p.replace(/[\\/]+$/, "");
const istZiel = (l: Laufwerk, ziele: string[]) =>
  ziele.some((z) => ohneEnde(z) === ohneEnde(l.pfad) || ohneEnde(z).startsWith(ohneEnde(l.pfad) + (l.pfad.includes("\\") ? "\\" : "/")));

/* ---------------------------------------------------------------------------------------------------------------- */

function Quellen({ laufwerke, zielBasis }: { laufwerke: Laufwerk[]; zielBasis: string[] }) {
  const lauf = useLauf();
  const karten = laufwerke.filter((l) => l.karte);
  const andere = laufwerke.filter((l) => !l.karte);

  async function ordnerWaehlen() {
    const pfad = await open({ directory: true, title: "Karte oder Reel-Ordner wählen" });
    if (typeof pfad !== "string") return;
    // Ist der Ordner ein eingestecktes Laufwerk, dieses wählen (dann auch mit Auswerfen), nicht einen zweiten Eintrag.
    const lw = laufwerke.find((l) => ohneEnde(l.pfad) === ohneEnde(pfad));
    lauf.quelleWaehlen(lw ? alsQuelle(lw) : { pfad, name: name(pfad), laufwerk: false });
  }

  const zeile = (l: Laufwerk) => {
    const gewaehlt = lauf.quelle?.pfad === l.pfad;
    const ziel = istZiel(l, zielBasis);
    const Z = l.karte ? MemoryStick : l.netz ? Network : HardDrive;
    if (ziel)
      return (
        <li key={l.pfad}>
          <div className="quelle quelle-ziel" title="Hier liegt ein Ziel: keine Quelle">
            <Z size={18} strokeWidth={1.75} aria-hidden className="quelle-zeichen" />
            <span className="quelle-text">
              <span className="quelle-name">{l.name}</span>
              <span className="quelle-info">Ziel{l.frei !== null && ` · ${bytesText(l.frei)} frei`}</span>
            </span>
          </div>
        </li>
      );
    return (
      <li key={l.pfad}>
        <button
          className={`quelle ${gewaehlt ? "quelle-gewaehlt" : ""}`}
          aria-pressed={gewaehlt}
          disabled={lauf.laeuft}
          title={l.pfad}
          onClick={() => lauf.quelleWaehlen(gewaehlt ? null : alsQuelle(l))}
        >
          <Z size={18} strokeWidth={1.75} aria-hidden className="quelle-zeichen" />
          <span className="quelle-text">
            <span className="quelle-name">{l.name}</span>
            <span className="quelle-info">
              {l.karte
                ? `${l.karte.kamera} · ${l.karte.clips} Clips · ${bytesText(l.karte.bytes)}`
                : l.frei !== null
                    ? `${bytesText(l.frei)} frei`
                    : l.netz
                      ? "Netzlaufwerk"
                      : ""}
            </span>
          </span>
        </button>
      </li>
    );
  };

  return (
    <aside className="seitenleiste" aria-label="Quellen">
      <div className="leiste-kopf">
        <h2>Karten</h2>
      </div>
      {karten.length > 0 ? (
        <ul className="quellen">{karten.map(zeile)}</ul>
      ) : (
        <p className="leiste-leer">Keine Kamerakarte eingesteckt. Karte in den Leser stecken; sie erscheint hier.</p>
      )}
      {andere.length > 0 && (
        <>
          <div className="leiste-kopf">
            <h2>Weitere Laufwerke</h2>
          </div>
          <ul className="quellen">{andere.map(zeile)}</ul>
        </>
      )}
      {lauf.quelle && !lauf.quelle.laufwerk && (
        <>
          <div className="leiste-kopf">
            <h2>Gewählter Ordner</h2>
          </div>
          <ul className="quellen">
            <li>
              <button className="quelle quelle-gewaehlt" aria-pressed disabled={lauf.laeuft} onClick={() => lauf.quelleWaehlen(null)}>
                <FolderOpen size={18} strokeWidth={1.75} aria-hidden className="quelle-zeichen" />
                <span className="quelle-text">
                  <span className="quelle-name">{lauf.quelle.name}</span>
                  <Pfad pfad={lauf.quelle.pfad} className="quelle-info" />
                </span>
              </button>
            </li>
          </ul>
        </>
      )}
      <div className="leiste-fuss">
        <button className="knopf knopf-voll" onClick={ordnerWaehlen} disabled={lauf.laeuft}>
          <FolderOpen size={16} strokeWidth={1.75} aria-hidden /> Ordner wählen …
        </button>
      </div>
    </aside>
  );
}

/* ---------------------------------------------------------------------------------------------------------------- */

/** Pro Zielordner: zählt er als eigene Kopie, oder liegt er auf derselben Platte wie ein anderer? */
function zaehlung(geraete: (ZielGeraet | undefined | null)[]) {
  const erste = new Map<string, number>();
  const zeilen = geraete.map((g, i): { ton: Ton; text: string } => {
    if (g === null) return { ton: "fehler", text: "Zählt nicht" }; // zu wenig Platz oder betroffen: zählt nicht
    if (!g) return { ton: "laeuft", text: "Wird bestimmt …" };
    if (!g.kennung) return { ton: "fehler", text: g.fehler === "nicht eingesteckt" ? "Nicht eingesteckt" : "Nicht lesbar" };
    const schluessel = g.kennung.sicher ? g.kennung.wert : "unsicher";
    const vorher = erste.get(schluessel);
    if (vorher !== undefined)
      return g.kennung.sicher
        ? { ton: "warn", text: `Gleiche Platte wie Ziel ${vorher + 1}` }
        : { ton: "warn", text: "Platte unbekannt: zählt mit anderen unbekannten als eine" };
    erste.set(schluessel, i);
    return g.kennung.sicher
      ? { ton: "ok", text: "Eigene Kopie" }
      : { ton: "warn", text: "Platte unbekannt: zählt höchstens einmal" };
  });
  return { zeilen, unabhaengig: erste.size };
}

function Auftrag({ pflicht, zurEinrichtung, laufwerke }: { pflicht: boolean; zurEinrichtung: () => void; laufwerke: Laufwerk[] }) {
  const lauf = useLauf();
  const e = useEinstellungen();
  const konto = useKonto();
  const q = lauf.quelle;

  // Geräte hinter den Zielen: neu bestimmen, wenn sich Ziele oder eingesteckte Laufwerke ändern.
  const [geraete, setGeraete] = useState<ZielGeraet[]>([]);
  const eingesteckt = laufwerke.map((l) => l.pfad).join("|");
  useEffect(() => {
    let aktuell = true;
    setGeraete([]);
    if (e.ziele.length) zielGeraete(e.ziele).then((g) => aktuell && setGeraete(g)).catch(() => {});
    return () => {
      aktuell = false;
    };
  }, [e.ziele.join("|"), eingesteckt]);
  // Nach einem Fehler: das Ziel, das die Meldung nennt, in der Tabelle als betroffen zeigen.
  const betroffen =
    lauf.phase === "fehler" && lauf.fehler ? e.ziele.findIndex((z) => lauf.fehler!.includes(ohneEnde(z))) : -1;
  const istKnapp = (g: ZielGeraet | undefined) => g?.frei != null && q?.bytes != null && g.frei < q.bytes;
  // Ein Ziel mit zu wenig Platz oder mit Fehler zählt nicht als Kopie (lieber nein als ein falsches Ja).
  const stand = (i: number) => lauf.zielStaende[i];
  // Eine frühere, vollständige Kopie braucht keinen Platz; ein abweichender Ordner zählt nur, wenn er zur Seite gelegt wird.
  // Beim Fortsetzen fehlt nur ein Teil; den genauen Platz prüft die Vorab-Prüfung.
  const knappHier = (i: number) =>
    stand(i)?.art !== "vorhanden" && !lauf.fortsetzen.includes(lauf.ziele[i]) && istKnapp(geraete[i]);
  const gesperrtHier = (i: number) =>
    (stand(i)?.art === "abweichend" && !lauf.zurSeite.includes(lauf.ziele[i])) ||
    (stand(i)?.art === "unterbrochen" && !lauf.zurSeite.includes(lauf.ziele[i]) && !lauf.fortsetzen.includes(lauf.ziele[i]));
  const { zeilen, unabhaengig } = zaehlung(
    e.ziele.map((_, i) => (i === betroffen || knappHier(i) || gesperrtHier(i) ? null : geraete[i])),
  );
  const genug = unabhaengig >= e.mindestKopien;

  async function zielHinzufuegen() {
    const pfad = await open({ directory: true, title: "Zielordner wählen" });
    if (typeof pfad === "string" && !e.ziele.includes(pfad)) e.setZiele([...e.ziele, pfad]);
  }

  const [wenigFrage, setWenigFrage] = useState(false);
  useEffect(() => setWenigFrage(false), [q?.pfad, e.ziele.join("|"), e.mindestKopien]);
  // Ohne einen einzigen Treffer erst nachfragen (iPhone noch nicht synchronisiert?), dann nach <Datum>_OHNE_DREHORT.
  const [trotzdem, setTrotzdem] = useState(false);
  useEffect(() => setTrotzdem(false), [q?.pfad]);
  const v = lauf.automatisch ? lauf.vorschau : null;
  const ohneTreffer = !!v && v.gesamt > 0 && v.drehorte.length === 0;
  const zuordnungOffen = lauf.automatisch && (lauf.vorschauLaedt || !v || (ohneTreffer && !trotzdem));
  const bereit = !!q && lauf.ziele.length > 0 && !pflicht && !lauf.sperrt && !zuordnungOffen;
  const pp = lauf.paProjekt;
  const kamera = pp
    ? [pp.fps ? `${zahl(pp.fps, 3).replace(/,?0+$/, "")} fps` : "", pp.codec ?? "", pp.aufloesungPx ?? ""].filter(Boolean).join(" · ")
    : "";
  const gesperrtWeil = !q
    ? "Zuerst links eine Karte wählen."
    : e.ziele.length === 0
      ? "Mindestens ein Ziel hinzufügen."
      : pflicht
        ? "Erst das Pflicht-Update installieren (Hinweis oben)."
        : lauf.automatisch && lauf.vorschauLaedt
          ? "Ordnet die Clips den Drehorten zu …"
          : ohneTreffer && !trotzdem
            ? "Kein Clip passt zu einem Take: oben neu laden oder trotzdem einlesen."
            : null;

  return (
    <div className="auftrag">
      {lauf.phase === "fehler" && lauf.fehler && <Fehlerband fehler={lauf.fehler} />}

      <section className="block" aria-labelledby="t-karte">
        <div className="block-kopf">
          <h2 id="t-karte">Karte</h2>
        </div>
        {q ? (
          <div className="karte">
            <MemoryStick size={28} strokeWidth={1.5} aria-hidden className="karte-zeichen" />
            <div className="karte-text">
              <span className="karte-name">{q.name}</span>
              <span className="karte-info">
                {q.kamera ? `${q.kamera} · ${q.clips} Clips · ${bytesText(q.bytes ?? 0)} · ` : "Ordner · "}
                <Pfad pfad={q.pfad} />
              </span>
            </div>
            <button className="knopf" onClick={() => lauf.quelleWaehlen(null)}>
              Andere Karte
            </button>
          </div>
        ) : (
          <p className="leer-zeile">Keine Karte gewählt. Links eine erkannte Karte anklicken oder einen Ordner wählen.</p>
        )}
      </section>

      <section className="block" aria-labelledby="t-ziele">
        <div className="block-kopf">
          <h2 id="t-ziele">Ziele</h2>
          {e.ziele.length > 0 && (
            <Status ton={genug ? "ok" : "warn"}>
              {unabhaengig} von {e.mindestKopien} unabhängigen Kopien
            </Status>
          )}
          <label className="kopien-wahl">
            <span title={lauf.paProjekt ? `Gilt für das Projekt ${lauf.paProjekt.name}` : "Gilt ohne Projekt"}>
              Freigabe ab{lauf.paProjekt ? ` (${lauf.paProjekt.kurzname})` : ""}
            </span>
            <select
              value={e.mindestKopien}
              disabled={lauf.laeuft}
              onChange={(ev) => e.setMindestKopien(Number(ev.target.value))}
              aria-label="Unabhängige Kopien für die Freigabe"
            >
              {[1, 2, 3, 4].map((n) => (
                <option key={n} value={n}>
                  {n} {n === 1 ? "Kopie (nur Test)" : "Kopien"}
                </option>
              ))}
            </select>
          </label>
          <button className="knopf knopf-klein" onClick={zielHinzufuegen}>
            <Plus size={14} strokeWidth={2} aria-hidden /> Ziel hinzufügen
          </button>
        </div>
        {e.ziele.length === 0 ? (
          <p className="leer-zeile">
            Noch kein Ziel. Für „Sicher zum Formatieren“ braucht es mindestens {e.mindestKopien} Ziele auf verschiedenen
            Platten, zum Beispiel eine externe SSD und das NAS.
          </p>
        ) : (
          <table className="tabelle">
            <thead>
              <tr>
                <th className="spalte-nr">#</th>
                <th>Zielordner</th>
                <th>Platte</th>
                <th className="rechts">Frei</th>
                <th>Zählt</th>
                <th className="spalte-aktion">
                  <span className="unsichtbar">Entfernen</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {e.ziele.map((z, i) => {
                const g = geraete[i];
                const knapp = knappHier(i);
                const st = stand(i);
                const zielPfad = lauf.ziele[i];
                const zaehlt: { ton: Ton; text: string } =
                  i === betroffen
                    ? { ton: "fehler", text: "Betroffen, siehe Meldung oben" }
                    : knapp
                      ? { ton: "fehler", text: "Zu wenig Platz" }
                      : zeilen[i];
                return (
                  <tr key={z}>
                    <td className="spalte-nr zahl">{i + 1}</td>
                    <td className="zelle-pfad">
                      <Pfad pfad={z} />
                      {lauf.ziele[i] && lauf.dreh && (
                        <span className="unterzeile">
                          <ArrowRight size={12} strokeWidth={2} aria-hidden />
                          <Pfad pfad={lauf.ziele[i].slice(ohneEnde(z).length + 1)} />
                        </span>
                      )}
                      {st?.art === "vorhanden" && (
                        <span className="unterzeile">
                          <Status ton="ok">Diese Karte liegt hier schon vollständig · wird nicht neu geschrieben, nur nachgeprüft</Status>
                        </span>
                      )}
                      {st?.art === "unterbrochen" && zielPfad && (
                        <span className="unterzeile ziel-konflikt">
                          {lauf.fortsetzen.includes(zielPfad) ? (
                            <>
                              <Status ton="warn">
                                Wird fortgesetzt: {st.vorhanden} von {st.gesamt} Dateien bleiben, der Rest kommt dazu; danach wird alles
                                zurückgelesen
                              </Status>
                              <button className="verweis" onClick={() => lauf.fortsetzenUmschalten(zielPfad)}>
                                Rückgängig
                              </button>
                            </>
                          ) : lauf.zurSeite.includes(zielPfad) ? (
                            <>
                              <Status ton="warn">Wird zur Seite gelegt (umbenannt in …_ALT_Datum_Zeit, nichts gelöscht) und neu kopiert</Status>
                              <button className="verweis" onClick={() => lauf.zurSeiteUmschalten(zielPfad)}>
                                Rückgängig
                              </button>
                            </>
                          ) : (
                            <>
                              <Status ton="warn">
                                Unterbrochene Kopie dieser Karte: {st.vorhanden} von {st.gesamt} Dateien sind schon da
                              </Status>
                              <button className="knopf knopf-klein" onClick={() => lauf.fortsetzenUmschalten(zielPfad)}>
                                Fortsetzen
                              </button>
                              <button className="knopf knopf-klein" onClick={() => lauf.zurSeiteUmschalten(zielPfad)}>
                                Zur Seite legen und neu kopieren
                              </button>
                            </>
                          )}
                        </span>
                      )}
                      {st?.art === "abweichend" && zielPfad && (
                        <span className="unterzeile ziel-konflikt">
                          {lauf.zurSeite.includes(zielPfad) ? (
                            <>
                              <Status ton="warn">Wird zur Seite gelegt (umbenannt in …_ALT_Datum_Zeit, nichts gelöscht) und neu kopiert</Status>
                              <button className="verweis" onClick={() => lauf.zurSeiteUmschalten(zielPfad)}>
                                Rückgängig
                              </button>
                            </>
                          ) : (
                            <>
                              <Status ton="fehler">Ordner existiert schon, passt aber nicht zu dieser Karte: {st.grund}</Status>
                              <button className="knopf knopf-klein" onClick={() => lauf.zurSeiteUmschalten(zielPfad)}>
                                Zur Seite legen und neu kopieren
                              </button>
                            </>
                          )}
                        </span>
                      )}
                    </td>
                    <td>
                      {g?.kennung ? (
                        <>
                          {g.kennung.beschreibung}
                          {g.kennung.seriennummer && <span className="unterzeile zahl">SN {g.kennung.seriennummer}</span>}
                        </>
                      ) : (
                        <span className="leise">–</span>
                      )}
                    </td>
                    <td className={`rechts zahl ${knapp ? "text-fehler" : ""}`}>
                      {g?.frei != null ? bytesText(g.frei) : "–"}
                    </td>
                    <td>
                      <Status ton={zaehlt.ton}>{zaehlt.text}</Status>
                    </td>
                    <td className="spalte-aktion">
                      <button
                        className="knopf-symbol"
                        aria-label={`Ziel ${i + 1} entfernen`}
                        title="Ziel entfernen"
                        onClick={() => e.setZiele(e.ziele.filter((x) => x !== z))}
                      >
                        <X size={16} strokeWidth={1.75} />
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>

      {q && !lauf.laeuft && <KartenGedaechtnis pfad={q.pfad} />}
      <section className="block" aria-labelledby="t-dreh">
        <div className="block-kopf">
          <h2 id="t-dreh">Dreh</h2>
          {(lauf.paDreh || lauf.automatisch) && <Status ton="ok">Takes aus dem Plate Assistant</Status>}
          {lauf.soll && !lauf.soll.fehler && <Status ton="ok">{lauf.soll.liste.length} Takes von der Stage</Status>}
          {lauf.soll?.fehler && <Status ton="warn">Soll-Liste nicht geladen</Status>}
          {kamera && (
            <span className="leise" title="Abweichende Clips erscheinen nur als Hinweis; die Freigabe hängt nie daran.">
              Kamera laut Projekt: <span className="zahl">{kamera}</span>
            </span>
          )}
        </div>
        {lauf.automatisch ? <Zuordnung trotzdem={trotzdem} setTrotzdem={setTrotzdem} /> : <DrehZeile />}
        <p className="ablage">
          {lauf.dreh ? (
            <>
              Ablage <span className="zahl">{lauf.dreh.kurzname ?? kurz(lauf.dreh.projekt)}/{lauf.dreh.datum}_{lauf.dreh.ortKurzname || lauf.dreh.name}/01_KAMERA/{q?.name ?? "…"}</span>
            </>
          ) : (
            "Ohne Projekt und Drehort kommt die Karte direkt in den Zielordner."
          )}
          {konto.verbindung !== "verbunden" && (
            <>
              {" "}
              ·{" "}
              <button className="verweis" onClick={zurEinrichtung}>
                Plate Assistant verbinden
              </button>{" "}
              für Projekte und Soll-Liste.
            </>
          )}
        </p>
      </section>

      <div className="startleiste">
        <div className="startleiste-befunde">
          {lauf.befunde.map((b) => (
            <Status key={b.text} ton={b.stufe === "fehler" ? "fehler" : "warn"}>
              {b.stufe === "fehler" && <strong>Sperrt: </strong>}
              {b.text}
            </Status>
          ))}
          {!bereit && gesperrtWeil && <span className="leise">{gesperrtWeil}</span>}
          {bereit && lauf.befunde.length === 0 && (
            <span className="leise">
              Liest die Karte einmal, schreibt gleichzeitig an alle Ziele und liest jedes Ziel danach ohne Cache zurück.
            </span>
          )}
        </div>
        {wenigFrage ? (
          <div className="weniger-frage" role="alertdialog" aria-label="Weniger Kopien als verlangt">
            <Status ton="warn">
              Verlangt sind {e.mindestKopien} unabhängige Kopien, möglich ist hier nur {unabhaengig}. Die Karte wird nicht
              freigegeben und darf nicht formatiert werden. Die fehlende Kopie kannst du später ergänzen.
            </Status>
            <div className="knopfreihe">
              <button
                className="knopf knopf-gefahr"
                onClick={() => {
                  setWenigFrage(false);
                  lauf.einlesen(true);
                }}
              >
                Mit nur {unabhaengig} {unabhaengig === 1 ? "Kopie" : "Kopien"} einlesen
              </button>
              <button className="knopf" onClick={() => setWenigFrage(false)}>
                Abbrechen
              </button>
            </div>
          </div>
        ) : (
          <button
            className="knopf knopf-haupt knopf-gross"
            disabled={!bereit}
            onClick={() => (genug ? lauf.einlesen(false) : setWenigFrage(true))}
          >
            <HardDrive size={18} strokeWidth={1.75} aria-hidden />
            {lauf.phase === "fehler" ? "Nochmals einlesen" : genug ? "Einlesen" : `Einlesen (nur ${unabhaengig} von ${e.mindestKopien} Kopien)`}
          </button>
        )}
      </div>
    </div>
  );
}

export const kurz = (p: string) =>
  p
    .trim()
    .replace(/[äÄ]/g, "AE")
    .replace(/[öÖ]/g, "OE")
    .replace(/[üÜ]/g, "UE")
    .replace(/[ßẞ]/g, "SS")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, "_")
    .replace(/^_+/, "")
    .slice(0, 24)
    .replace(/_+$/, "") || "OHNE_PROJEKT";

/** Mit Projekt aus dem Plate Assistant: kein Drehort von Hand. Zeigt, wohin die Clips der Karte gehören (über die
 *  Takes aller Drehorte des Projekts); die Karte kommt zum Drehort mit den meisten Clips. */
function Zuordnung({ trotzdem, setTrotzdem }: { trotzdem: boolean; setTrotzdem: (b: boolean) => void }) {
  const lauf = useLauf();
  const v = lauf.vorschau;
  if (!lauf.quelle) return <p className="zuordnung leise">Karte wählen: Die App ordnet ihre Clips selbst den Drehorten des Projekts zu.</p>;
  if (lauf.vorschauLaedt || !v) return <p className="zuordnung"><Status ton="laeuft">Ordnet die Clips den Drehorten zu …</Status></p>;
  const ohne = v.gesamt > 0 && v.drehorte.length === 0;
  return (
    <div className="zuordnung">
      <ul className="zuordnung-liste">
        {v.drehorte.map((d, i) => (
          <li key={d.id}>
            <Status ton="ok">
              {d.name} · {d.clips.length} {d.clips.length === 1 ? "Clip" : "Clips"}
            </Status>
            {i === 0 && v.drehorte.length > 1 && <span className="leise">Ordner der Karte (meiste Clips)</span>}
          </li>
        ))}
        {v.ohne.length > 0 && (
          <li>
            <Status ton="warn">
              Zu klären · {v.ohne.length} {v.ohne.length === 1 ? "Clip" : "Clips"} ohne Take
            </Status>
            <span className="leise">nach dem Kopieren sucht die App noch über Timecode und Uhrzeit, der Rest auf der Projekt-Seite</span>
          </li>
        )}
        {v.gesamt === 0 && (
          <li>
            <Status ton="leise">Keine Clips erkannt</Status>
          </li>
        )}
        {v.uhrFalsch && (
          <li>
            <Status ton="warn">Kamerauhr prüfen: Die Clips tragen ein unglaubwürdiges Datum</Status>
          </li>
        )}
      </ul>
      {ohne && !trotzdem && (
        <div className="zuordnung-frage" role="alert">
          <p>
            <b>Kein Clip passt zu einem Take.</b> Hat das iPhone schon synchronisiert? Sonst kommt die Karte in den Ordner
            <span className="zahl"> {v.aufnahmetag ?? "heute"}_OHNE_DREHORT</span> und alle Clips stehen unter „Zu klären“.
          </p>
          <div className="zuordnung-knoepfe">
            <button className="knopf" onClick={lauf.vorschauLaden}>
              <RotateCcw size={14} strokeWidth={2} aria-hidden /> Neu laden
            </button>
            <button className="knopf" onClick={() => setTrotzdem(true)}>
              Trotzdem einlesen
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/** Datum und Drehort des Projekts (das Projekt selbst wird oben links gewählt und gilt für alle Seiten).
 *  Drehorte aus dem Plate Assistant, oder von Hand, wenn keine Verbindung besteht. */
function DrehZeile() {
  const lauf = useLauf();
  const konto = useKonto();
  const verbunden = konto.verbindung === "verbunden";
  const passende = drehsVon(konto.drehs, lauf.paProjekt);
  const ohneProjekt = verbunden ? !lauf.paProjekt : !lauf.projektText.trim();

  return (
    <div className="dreh">
      <label className="eingabe-gruppe eingabe-datum">
        <span>Datum</span>
        <input type="date" className="zahl" value={lauf.drehDatum} onChange={(ev) => lauf.setDrehDatum(ev.target.value)} />
      </label>
      <label className="eingabe-gruppe">
        <span>Drehort</span>
        {verbunden && passende.length > 0 ? (
          <select
            value={lauf.paDreh?.id ?? (lauf.drehName ? "__hand" : "")}
            onChange={(ev) => {
              const d = passende.find((x) => x.id === ev.target.value) ?? null;
              lauf.setPaDreh(d);
              if (d) {
                lauf.setDrehName(d.name);
                lauf.setDrehDatum(d.datum);
              } else lauf.setDrehName("");
            }}
          >
            <option value="">Drehort wählen …</option>
            {lauf.drehName && !lauf.paDreh && <option value="__hand">{lauf.drehName} (von Hand)</option>}
            {passende.map((d) => (
              <option key={d.id} value={d.id}>
                {d.datum} · {d.name}
              </option>
            ))}
          </select>
        ) : (
          <input value={lauf.drehName} placeholder="z. B. Rheinufer" onChange={(ev) => lauf.setDrehName(ev.target.value)} />
        )}
      </label>
      {ohneProjekt && <Status ton="warn">Oben links ein Projekt wählen</Status>}
    </div>
  );
}

/* ---------------------------------------------------------------------------------------------------------------- */

/** Was nach einem Fehler zu tun ist, aus dem Fehlertext abgeleitet (Kern und App-Hülle melden auf Deutsch). */
function naechsterSchritt(fehler: string): string {
  const f = fehler.toLowerCase();
  if (f.includes("abgebrochen")) return "Die halben Kopien sind weggeräumt. Die Karte ist unverändert; einfach erneut einlesen.";
  if (f.includes("existiert schon")) return "Ein anderes Ziel wählen oder den vorhandenen Ordner prüfen; er wird nie überschrieben.";
  if (f.includes("zu wenig platz")) return "Platz auf dem Ziel schaffen oder ein grösseres Ziel wählen.";
  if (f.includes("auf der karte") || f.includes("derselben platte"))
    return "Ein Ziel auf einer anderen Platte wählen; die Karte selbst ist kein Ziel.";
  if (f.includes("alle ziele ausgefallen")) return "Platten prüfen (Kabel, Strom, Schreibschutz) und erneut einlesen.";
  if (f.includes("quelle") || f.includes("karte nicht lesbar") || f.includes("verändert"))
    return "Karte und Kartenleser prüfen, Karte neu einstecken und erneut einlesen.";
  if (f.includes("nicht erreichbar") || f.includes("nicht lesbar") || f.includes("nicht schreibbar"))
    return "Ist die Platte eingesteckt und beschreibbar? Danach erneut einlesen.";
  return "Meldung unten lesen und erneut einlesen.";
}

function Fehlerband({ fehler }: { fehler: string }) {
  // Erste Zeile ist die Meldung des Kerns in Klartext (Problem und betroffener Pfad); weitere Zeilen sind Einzelheiten.
  const [problem, ...rest] = fehler.trim().split("\n");
  return (
    <section className="urteil urteil-rot" role="alert">
      <CircleX size={40} strokeWidth={1.75} aria-hidden className="urteil-zeichen" />
      <div className="urteil-text">
        <h1>Nicht kopiert · Karte nicht formatieren</h1>
        <p className="urteil-problem">{problem}</p>
        <p>
          <strong>Nächster Schritt:</strong> {naechsterSchritt(fehler)}
        </p>
        {rest.length > 0 && (
          <details className="aufklapp">
            <summary>
              <ChevronRight size={14} strokeWidth={2} aria-hidden /> Einzelheiten
            </summary>
            <code>{rest.join("\n")}</code>
          </details>
        )}
      </div>
    </section>
  );
}

/* ---------------------------------------------------------------------------------------------------------------- */

function Fortschritt() {
  const lauf = useLauf();
  const { stand, phase } = lauf;
  const [, setTakt] = useState(0);
  useEffect(() => {
    const t = setInterval(() => setTakt((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, []);

  const anteil = stand.bytes > 0 ? Math.min(1, stand.gelesen / stand.bytes) : 0;
  const sekunden = Math.max(1, (Date.now() - stand.beginn) / 1000);
  const tempo = stand.gelesen / sekunden;
  const rest = tempo > 0 ? (stand.bytes - stand.gelesen) / tempo : NaN;
  const schritte = [
    { id: "kopieren", text: "Kopieren" },
    { id: "pruefen", text: "Zurücklesen" },
    ...(lauf.ergebnis === null && phase === "nachlesen" ? [{ id: "nachlesen", text: "Karte nochmals lesen" }] : []),
  ];
  const jetzt = schritte.findIndex((s) => s.id === phase);
  // Nach dem Zurücklesen meldet der Kern auch Bewegungsdaten und Plates als „nachlesen“.
  const abschluss = phase === "nachlesen" && /^(ART CMD|Plates|Vorschaubilder)/.test(stand.pruefPfad);
  const ausgefallen = (ordner: string) => stand.ausfaelle.find((a) => ordner.startsWith(ohneEnde(a.ordner)) || a.ordner.startsWith(ordner));

  return (
    <div className="lauf">
      <div className="lauf-kopf">
        <div>
          <h1>
            {phase === "kopieren"
              ? "Kopiert an alle Ziele"
              : phase === "pruefen"
                ? stand.zielZahl > 1
                  ? "Liest die Ziele zurück (verschiedene Platten gleichzeitig)"
                  : "Liest das Ziel zurück"
                : abschluss
                  ? "Schliesst ab"
                  : "Liest die Karte ein zweites Mal"}
          </h1>
          <p className="leise">
            {lauf.quelle?.name} · {stand.dateien} Dateien · {bytesText(stand.bytes)} · Karte nicht entfernen
          </p>
        </div>
        <ol className="schritte" aria-label="Ablauf">
          {schritte.map((s, i) => (
            <li key={s.id} className={i < jetzt ? "schritt-fertig" : i === jetzt ? "schritt-jetzt" : ""}>
              {i < jetzt ? <CircleCheck size={14} strokeWidth={2} aria-hidden /> : <span className="schritt-nr">{i + 1}</span>}
              {s.text}
            </li>
          ))}
          <li>
            <span className="schritt-nr">{schritte.length + 1}</span>Urteil
          </li>
        </ol>
      </div>

      {phase === "kopieren" && (
        <div className="gesamt">
          <div className="balken" role="progressbar" aria-valuenow={Math.round(anteil * 100)} aria-valuemin={0} aria-valuemax={100}>
            <div style={{ transform: `scaleX(${anteil.toFixed(4)})` }} />
          </div>
          <dl className="werte">
            <div>
              <dt>Fortschritt</dt>
              <dd className="zahl">{zahl(anteil * 100)} %</dd>
            </div>
            <div>
              <dt>Kopiert</dt>
              <dd className="zahl">
                {bytesText(stand.gelesen)} von {bytesText(stand.bytes)}
              </dd>
            </div>
            <div>
              <dt>Tempo</dt>
              <dd className="zahl">{bytesText(tempo)}/s</dd>
            </div>
            <div>
              <dt>Restzeit Kopie</dt>
              <dd className="zahl">{dauerText(rest)}</dd>
            </div>
            <div className="werte-breit">
              <dt>Datei {stand.dateiNummer > 0 && <span className="zahl">{stand.dateiNummer} von {stand.dateien}</span>}</dt>
              <dd>
                <Pfad pfad={stand.datei || "…"} />
              </dd>
            </div>
          </dl>
        </div>
      )}

      <table className="tabelle">
        <thead>
          <tr>
            <th className="spalte-nr">#</th>
            <th>Ziel</th>
            <th className="spalte-balken">Fortschritt</th>
            <th>Stand</th>
          </tr>
        </thead>
        <tbody>
          {lauf.ziele.slice(0, stand.zielZahl).map((z, i) => {
            const aus = ausgefallen(z);
            let ton: Ton = "leise";
            let text = "Wartet auf das Zurücklesen";
            // Zurückgelesen: Dateien und Bytes, die dieses Ziel schon abgeschlossen hat (Ziele auf verschiedenen Platten
            // laufen gleichzeitig, auf derselben Platte nacheinander).
            const p = stand.pruefJeZiel[i];
            const fertig = p?.bytes ?? 0;
            const durch = !!p && p.nummer >= stand.dateien;
            let teil = 0;
            if (aus) {
              ton = "fehler";
              text = `Ausgefallen: ${aus.fehler}`;
            } else if (phase === "kopieren") {
              ton = "laeuft";
              text = `Schreibt · ${bytesText(tempo)}/s`;
              teil = anteil;
            } else if (phase === "pruefen" && p && !durch) {
              ton = "laeuft";
              const s = Math.max(1, (Date.now() - p.beginn) / 1000);
              const t = fertig / s;
              teil = stand.bytes > 0 ? fertig / stand.bytes : 0;
              text = `Liest zurück · Datei ${Math.min(p.nummer + 1, stand.dateien)} von ${stand.dateien}${
                t > 0 ? ` · ${bytesText(t)}/s · noch ${dauerText((stand.bytes - fertig) / t)}` : ""
              }`;
            } else if (phase === "pruefen" && !p) {
              text = "Wartet (gleiche Platte wie ein anderes Ziel)";
            } else if (phase !== "pruefen" || durch) {
              ton = "ok";
              text = "Zurückgelesen, Ergebnis am Schluss";
              teil = 1;
            }
            return (
              <tr key={z}>
                <td className="spalte-nr zahl">{i + 1}</td>
                <td className="zelle-pfad">
                  <Pfad pfad={z} />
                </td>
                <td className="spalte-balken">
                  <div
                    className={`balken balken-klein ${ton === "ok" ? "balken-ok" : ton === "fehler" ? "balken-fehler" : ""}`}
                    role="progressbar"
                    aria-label={`Ziel ${i + 1}`}
                    aria-valuenow={Math.round(teil * 100)}
                    aria-valuemin={0}
                    aria-valuemax={100}
                  >
                    <div style={{ transform: `scaleX(${teil.toFixed(4)})` }} />
                  </div>
                </td>
                <td>
                  <Status ton={ton}>{text}</Status>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>

      <Dateiliste />

      <div className="lauf-fuss">
        <span className="leise">
          Die App lässt sich während des Kopierens nicht schliessen. Abbrechen räumt die halben Kopien weg; die Karte bleibt
          unverändert.
        </span>
        <button className={`knopf ${lauf.abbruchFragen ? "knopf-gefahr" : ""}`} onClick={lauf.abbrechen}>
          {lauf.abbruchFragen ? "Wirklich abbrechen?" : "Abbrechen"}
        </button>
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------------------------------------------------------- */

type Reiter = "ziele" | "abgleich" | "dateien" | "metadaten";

function Urteil({ ergebnis, laufwerke }: { ergebnis: KartenErgebnis; laufwerke: Laufwerk[] }) {
  const lauf = useLauf();
  const { freigabe, urteile, kennungen, abgleich } = ergebnis;
  const fehlerhaft = urteile.some((u) => u.kopierfehler || u.abweichungen.length > 0);
  const nurKopienFehlen = !freigabe.sicher && !fehlerhaft && freigabe.unabhaengige_kopien < freigabe.mindest_kopien;
  // Rot heisst Datenverlust droht: jede nicht freigegebene Karte (gemeinsames Design).
  const ton = freigabe.sicher ? "ok" : "rot";
  const karte = lauf.letzteQuelle;
  const steckt = !!karte?.laufwerk && laufwerke.some((l) => l.pfad === karte.pfad);
  const [auswurf, setAuswurf] = useState<{ ton: Ton; text: string } | null>(null);
  const [reiter, setReiter] = useState<Reiter>("ziele");
  const neu = useRef(ergebnis);
  if (neu.current !== ergebnis) {
    neu.current = ergebnis;
    setAuswurf(null);
  }

  const gefunden = abgleich ? abgleich.gefunden.length + abgleich.ueberTimecode.length + abgleich.ueberZeitfenster.length : 0;
  const offen = abgleich ? abgleich.fehlt.length + abgleich.mehrdeutig.length + abgleich.unerwartet.length : 0;
  const summe = ergebnis.kopie.dateien.reduce((n, d) => n + d.groesse, 0);
  const dauer = (Date.parse(ergebnis.kopie.ende) - Date.parse(ergebnis.kopie.beginn)) / 1000 || 0;
  const bericht = ergebnis.berichte.find((b): b is { Ok: string } => "Ok" in b)?.Ok;

  async function auswerfenKlick() {
    if (!karte) return;
    setAuswurf({ ton: "laeuft", text: "Wirft aus …" });
    try {
      await auswerfen(karte.pfad);
      setAuswurf({ ton: "ok", text: `${karte.name} ausgeworfen. Die Karte kann entnommen und in der Kamera formatiert werden.` });
    } catch (err) {
      setAuswurf({ ton: "fehler", text: String(err) });
    }
  }

  return (
    <div className="ergebnis">
      <section className={`urteil urteil-${ton}`} aria-live="polite">
        {ton === "ok" ? (
          <CircleCheck size={44} strokeWidth={1.75} aria-hidden className="urteil-zeichen" />
        ) : fehlerhaft ? (
          <CircleX size={44} strokeWidth={1.75} aria-hidden className="urteil-zeichen" />
        ) : (
          <TriangleAlert size={44} strokeWidth={1.75} aria-hidden className="urteil-zeichen" />
        )}
        <div className="urteil-text">
          <h1>{freigabe.sicher ? "Sicher zum Formatieren" : "Nicht freigegeben · Karte nicht formatieren"}</h1>
          <p>
            {karte?.name ?? name(ergebnis.kopie.quelle)} · {freigabe.grund}
          </p>
          {!freigabe.sicher && !fehlerhaft && freigabe.unabhaengige_kopien < freigabe.mindest_kopien && (
            <p className="urteil-grund">
              Alle Dateien sind fehlerfrei kopiert und geprüft. Es fehlt nur{" "}
              {freigabe.mindest_kopien - freigabe.unabhaengige_kopien === 1
                ? "eine weitere unabhängige Kopie"
                : `${freigabe.mindest_kopien - freigabe.unabhaengige_kopien} weitere unabhängige Kopien`}
              . Sobald eine weitere Platte da ist: „Kopie ergänzen“. Die vorhandene Kopie wird dann nur nachgeprüft, nicht neu
              geschrieben. {steckt
                ? ""
                : "Dafür die Karte wieder einstecken (sie ist ja nicht formatiert). Ist sie nicht mehr da: unter „Prüfen“ die fehlende Kopie aus dieser Kopie erstellen."}
            </p>
          )}
          <p className="urteil-kennwerte">
            {ergebnis.kopie.dateien.length} Dateien · {bytesText(summe)} · XXH3-128
            {ergebnis.kopie.dateien.some((d) => d.pruefsumme.md5) && " + MD5"}
            {dauer > 0 && ` · ${dauerText(dauer)} · ${bytesText(summe / dauer)}/s im Schnitt`}
          </p>
          <ul className="belege">
            {urteile.map((u, i) => {
              const gut = !u.kopierfehler && u.abweichungen.length === 0;
              const k = kennungen[i];
              return (
                <li key={u.ordner}>
                  <Status ton={gut ? "ok" : "rot"}>
                    {k?.beschreibung || name(u.ordner)}
                    {k?.seriennummer && <span className="zahl"> · SN {k.seriennummer}</span>} ·{" "}
                    {gut ? `${u.geprueft} Dateien zurückgelesen` : "fehlerhaft"}
                  </Status>
                </li>
              );
            })}
            {abgleich && (
              <li>
                <Status ton={abgleich.fehlt.length ? "warn" : "ok"}>
                  {abgleich.fehlt.length
                    ? `${abgleich.fehlt.length} gedrehte ${abgleich.fehlt.length === 1 ? "Take fehlt" : "Takes fehlen"} auf dieser Karte`
                    : `Alle ${gefunden} erwarteten Takes da`}
                </Status>
              </li>
            )}
            {abgleich && abgleich.mehrdeutig.length + abgleich.unerwartet.length > 0 && (
              <li>
                <Status ton="warn">
                  {abgleich.mehrdeutig.length + abgleich.unerwartet.length === 1
                    ? "1 Clip ohne eindeutigen Take"
                    : `${abgleich.mehrdeutig.length + abgleich.unerwartet.length} Clips ohne eindeutigen Take`}{" "}
                  (Reiter Abgleich)
                </Status>
              </li>
            )}
          </ul>
          {freigabe.hinweise.map((h) => (
            <p key={h} className="urteil-hinweis">
              {h}
            </p>
          ))}
          {(ergebnis.nachtraege?.length ?? 0) > 0 && (
            <div className="nachtraege" role="alert">
              <Status ton="warn">Nach dem Bericht nicht alles geklappt (die Kopie ist davon nicht betroffen):</Status>
              <ul>
                {ergebnis.nachtraege!.map((n) => (
                  <li key={n}>{n}</li>
                ))}
              </ul>
              <p className="leise">Steht auch als Nachtrag neben dem Bericht in 04_BERICHTE.</p>
            </div>
          )}
          {auswurf && (
            <p className="urteil-auswurf">
              <Status ton={auswurf.ton}>{auswurf.text}</Status>
            </p>
          )}
        </div>
        <div className="urteil-knoepfe">
          {freigabe.sicher && steckt && auswurf?.ton !== "ok" && (
            <button className="knopf knopf-haupt knopf-gross" onClick={auswerfenKlick} disabled={auswurf?.ton === "laeuft"}>
              <Eject size={18} strokeWidth={1.75} aria-hidden /> Karte auswerfen
            </button>
          )}
          {!freigabe.sicher && karte && (
            // Fehlen nur Kopien: „Kopie ergänzen“. Zurück zum Auftrag; die vorhandene Kopie wird dort erkannt und nur
            // nachgeprüft, ein weiteres Ziel kommt dazu. Sonst: nochmals einlesen.
            <button className="knopf knopf-gross" onClick={() => lauf.quelleWaehlen(karte)}>
              {nurKopienFehlen ? (
                <>
                  <Plus size={16} strokeWidth={1.75} aria-hidden /> Kopie ergänzen
                </>
              ) : (
                <>
                  <RotateCcw size={16} strokeWidth={1.75} aria-hidden /> Nochmals einlesen
                </>
              )}
            </button>
          )}
          {bericht && (
            <button className="knopf" onClick={() => openPath(bericht)}>
              <FileText size={16} strokeWidth={1.75} aria-hidden /> Bericht öffnen
            </button>
          )}
        </div>
      </section>

      <div className="reiter" role="tablist" aria-label="Einzelheiten">
        {(
          [
            ["ziele", `Ziele (${urteile.length})`],
            ["abgleich", abgleich ? `Abgleich (${offen ? `${offen} offen` : "vollständig"})` : "Abgleich"],
            ["dateien", `Dateien (${ergebnis.kopie.dateien.length})`],
            ["metadaten", "Metadaten"],
          ] as [Reiter, string][]
        ).map(([id, text]) => (
          <button key={id} role="tab" aria-selected={reiter === id} className="reiter-knopf" onClick={() => setReiter(id)}>
            {text}
          </button>
        ))}
      </div>
      <div className="reiter-inhalt" role="tabpanel">
        {reiter === "ziele" && <ZieleTabelle ergebnis={ergebnis} />}
        {reiter === "abgleich" && <AbgleichTabelle ergebnis={ergebnis} />}
        {reiter === "dateien" && <DateienTabelle ergebnis={ergebnis} />}
        {reiter === "metadaten" && <Metadaten ergebnis={ergebnis} />}
      </div>
    </div>
  );
}

export function abweichungText(a: Abweichung) {
  switch (a.art) {
    case "fehlt":
      return `Fehlt: ${a.pfad}`;
    case "groesse":
      return `Falsche Grösse: ${a.pfad} (${a.ist} statt ${a.soll} Bytes)`;
    case "pruefsumme":
      return `Prüfsumme weicht ab: ${a.pfad}`;
    case "unlesbar":
      return `Nicht lesbar: ${a.pfad} (${a.fehler})`;
    case "zusaetzlich":
      return `Nicht von der Karte: ${a.pfad}`;
  }
}

function ZieleTabelle({ ergebnis }: { ergebnis: KartenErgebnis }) {
  return (
    <table className="tabelle">
      <thead>
        <tr>
          <th className="spalte-nr">#</th>
          <th>Ziel</th>
          <th>Platte</th>
          <th>Prüfung</th>
          <th>Bericht</th>
        </tr>
      </thead>
      <tbody>
        {ergebnis.urteile.map((u, i) => {
          const k = ergebnis.kennungen[i];
          const gut = !u.kopierfehler && u.abweichungen.length === 0;
          const b = ergebnis.berichte[i];
          return (
            <tr key={u.ordner}>
              <td className="spalte-nr zahl">{i + 1}</td>
              <td className="zelle-pfad">
                <Pfad pfad={u.ordner} />
                {ergebnis.mhl[i] && <span className="unterzeile">ASC MHL {name(ergebnis.mhl[i]!)}</span>}
              </td>
              <td>
                {k?.beschreibung || k?.wert}
                {k?.seriennummer && <span className="unterzeile zahl">SN {k.seriennummer}</span>}
                {k && !k.sicher && <span className="unterzeile">Platte nicht bestimmbar</span>}
              </td>
              <td>
                <Status ton={gut ? "ok" : "rot"}>{gut ? `${u.geprueft} Dateien geprüft` : "Fehlerhaft"}</Status>
                {u.kopierfehler && <span className="unterzeile text-fehler">Kopieren: {u.kopierfehler}</span>}
                {u.abweichungen.map((a) => (
                  <span key={abweichungText(a)} className="unterzeile text-fehler">
                    {abweichungText(a)}
                  </span>
                ))}
              </td>
              <td>
                {b && "Ok" in b ? (
                  <button className="knopf knopf-klein" onClick={() => openPath(b.Ok)}>
                    Öffnen
                  </button>
                ) : (
                  <span className="text-fehler">{b ? `Nicht geschrieben: ${(b as { Err: string }).Err}` : "–"}</span>
                )}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function AbgleichTabelle({ ergebnis }: { ergebnis: KartenErgebnis }) {
  const a = ergebnis.abgleich;
  if (!a)
    return (
      <p className="leer-zeile">
        Ohne Soll-Liste kein Abgleich. Im Studio die Stage in der Einrichtung eintragen, draussen beim Dreh einen Drehort
        aus dem Plate Assistant wählen.
      </p>
    );
  type Zeile = { ton: Ton; stand: string; take: string; clip: string; weg: string };
  const take = (s: { szene: string; take: string }) => `${s.szene} / ${s.take}`;
  const zeilen: Zeile[] = [
    ...a.fehlt.map((s) => ({ ton: "warn" as Ton, stand: "Fehlt auf der Karte", take: take(s), clip: s.clip, weg: s.bewertung })),
    ...a.mehrdeutig.map(([s, ps]) => ({ ton: "warn" as Ton, stand: "Mehrdeutig, zu klären", take: take(s), clip: ps.map(name).join(", "), weg: "" })),
    ...a.unerwartet.map((p) => ({ ton: "warn" as Ton, stand: "Ohne Take, zu klären", take: "–", clip: name(p), weg: "" })),
    ...a.gefunden.map(([s, p]) => ({ ton: "ok" as Ton, stand: "Da", take: take(s), clip: name(p), weg: "Clipname" })),
    ...a.ueberTimecode.map(([s, p]) => ({ ton: "ok" as Ton, stand: "Da", take: take(s), clip: name(p), weg: "Timecode" })),
    ...a.ueberZeitfenster.map(([s, p]) => ({ ton: "ok" as Ton, stand: "Da", take: take(s), clip: name(p), weg: "Klappenzeit" })),
  ];
  return (
    <table className="tabelle">
      <thead>
        <tr>
          <th>Stand</th>
          <th>Szene / Take</th>
          <th>Clip</th>
          <th>Zugeordnet über</th>
        </tr>
      </thead>
      <tbody>
        {zeilen.map((z, i) => (
          <tr key={i}>
            <td>
              <Status ton={z.ton}>{z.stand}</Status>
            </td>
            <td className="zahl">{z.take}</td>
            <td className="zahl">{z.clip || "–"}</td>
            <td className="leise">{z.weg}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function DateienTabelle({ ergebnis }: { ergebnis: KartenErgebnis }) {
  const mitMd5 = ergebnis.kopie.dateien.some((d) => d.pruefsumme.md5);
  return (
    <table className="tabelle">
      <thead>
        <tr>
          <th>Datei</th>
          <th className="rechts">Grösse</th>
          <th>XXH3-128</th>
          {mitMd5 && <th>MD5</th>}
        </tr>
      </thead>
      <tbody>
        {ergebnis.kopie.dateien.map((d) => (
          <tr key={d.pfad}>
            <td className="zahl">{d.pfad}</td>
            <td className="rechts zahl">{bytesText(d.groesse)}</td>
            <td className="zahl leise auswaehlbar">{d.pruefsumme.xxh128}</td>
            {mitMd5 && <td className="zahl leise auswaehlbar">{d.pruefsumme.md5}</td>}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Metadaten({ ergebnis }: { ergebnis: KartenErgebnis }) {
  const s = ergebnis.stage;
  let stage: { ton: Ton; text: string } | null = null;
  if (s && "Err" in s) stage = { ton: "warn", text: `An die Stage nicht gemeldet: ${s.Err}` };
  else if (s) {
    const d = s.Ok as { ok?: boolean; zugeordnet?: number; mehrdeutig?: number; ohneTake?: number; meldung?: string };
    const offen = (d.mehrdeutig ?? 0) + (d.ohneTake ?? 0);
    stage = d.ok
      ? { ton: "ok", text: `An die Stage gemeldet: ${d.zugeordnet ?? 0} Clips zugeordnet${offen ? `, ${offen} in der Konsole zu klären` : ""}` }
      : { ton: "warn", text: `Stage hat die Karte abgelehnt: ${d.meldung ?? "ohne Grund"}` };
  }
  const db = ergebnis.datenbank;
  const p = ergebnis.plates;
  const ales = ergebnis.ale.filter(Boolean) as string[];
  return (
    <div className="metadaten">
      <ul className="liste">
        <li>{stage ? <Status ton={stage.ton}>{stage.text}</Status> : <Status ton="leise">Keine Stage eingetragen</Status>}</li>
        <li>
          {!db ? (
            <Status ton="leise">Nicht in der Datenbank (kein Projekt aus dem Plate Assistant gewählt)</Status>
          ) : "Err" in db ? (
            <Status ton="warn">Nicht in der Datenbank: {db.Err}</Status>
          ) : (ergebnis.datenbankAbgelehnt?.length ?? 0) > 0 ? (
            <Status ton="warn">
              In der Datenbank, aber {ergebnis.datenbankAbgelehnt!.length} {ergebnis.datenbankAbgelehnt!.length === 1 ? "Zeile" : "Zeilen"} nicht
              übernommen: {ergebnis.datenbankAbgelehnt!.join("; ")}
            </Status>
          ) : (
            <Status ton="ok">Karte und Clips in der Datenbank, der Plate Assistant sieht sie</Status>
          )}
        </li>
        <li>
          {p ? (
            <Status ton={p.fehler.length ? "warn" : "ok"}>
              02_PLATES: {p.plates} Plates, {p.fotosNeu} neue Fotos{p.fehler.length > 0 && ` · ${p.fehler.length} Fehler`}
            </Status>
          ) : (
            <Status ton="leise">Keine Plates (kein Drehort aus dem Plate Assistant)</Status>
          )}
        </li>
        <li>
          {ales.length ? <Status ton="ok">ALE für den Schnitt: {name(ales[0])}</Status> : <Status ton="leise">Keine ALE (keine Clips mit Timecode)</Status>}
        </li>
      </ul>
      {ergebnis.bewegung.length > 0 ? (
        <table className="tabelle">
          <thead>
            <tr>
              <th>Clip</th>
              <th className="rechts">Neigung</th>
              <th className="rechts">Rollen</th>
              <th className="rechts">Bereich</th>
              <th className="rechts">Brennweite</th>
            </tr>
          </thead>
          <tbody>
            {ergebnis.bewegung.map(([clip, b]) => (
              <tr key={clip}>
                <td className="zahl">{name(clip)}</td>
                <td className="rechts zahl">{b.tilt ? `${zahl(b.tilt.mittel)}°` : "–"}</td>
                <td className="rechts zahl">{b.roll ? `${zahl(b.roll.mittel)}°` : "–"}</td>
                <td className="rechts zahl">
                  {b.tilt && b.roll ? `${zahl(b.tilt.max - b.tilt.min)}° / ${zahl(b.roll.max - b.roll.min)}°` : "–"}
                </td>
                <td className="rechts zahl">{b.brennweiteMm ? `${zahl(b.brennweiteMm)} mm` : "–"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className="leer-zeile">Keine Bewegungsdaten. Dafür ARRI ART CMD in der Einrichtung eintragen.</p>
      )}
    </div>
  );
}

/** Dateien in der Reihenfolge des Kopierens, mit dem Stand pro Ziel (wie die Jobliste in Silverstack). */
function Dateiliste() {
  const { stand, phase, ziele } = useLauf();
  const zahlZiele = stand.zielZahl;
  // Sehr volle Karten: nur die letzten Einträge zeichnen, die Zahl steht im Kopf.
  const GRENZE = 400;
  const sichtbar = stand.liste.length > GRENZE ? stand.liste.slice(-GRENZE) : stand.liste;
  const zuletzt = stand.liste.length - 1;
  // Beim Kopieren die laufende Datei im Blick halten (unten), solange niemand selbst hochgescrollt hat.
  const rumpf = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const r = rumpf.current;
    if (r && phase === "kopieren" && r.scrollHeight - r.scrollTop - r.clientHeight < 80) r.scrollTop = r.scrollHeight;
  }, [stand.liste.length, phase]);
  if (stand.liste.length === 0) return null;
  return (
    <section className="block dateiliste" aria-labelledby="t-dateien">
      <div className="block-kopf">
        <h2 id="t-dateien">Dateien</h2>
        <span className="leise zahl">
          {stand.liste.length} von {stand.dateien}
          {stand.liste.length > GRENZE && ` · die letzten ${GRENZE} gezeigt`}
        </span>
      </div>
      <div className="dateiliste-rumpf" ref={rumpf}>
        <table className="tabelle">
          <thead>
            <tr>
              <th>Datei</th>
              <th className="rechts">Grösse</th>
              {ziele.slice(0, zahlZiele).map((z, i) => (
                <th key={z} title={z}>
                  Ziel {i + 1}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {sichtbar.map((d, k) => {
              const index = stand.liste.length - sichtbar.length + k;
              const schreibt = phase === "kopieren" && index === zuletzt;
              return (
                <tr key={d.pfad}>
                  <td className="zahl">{d.pfad}</td>
                  <td className="rechts zahl">{d.groesse ? bytesText(d.groesse) : "–"}</td>
                  {Array.from({ length: zahlZiele }, (_, t) => (
                    <td key={t}>
                      {schreibt ? (
                        <Status ton="laeuft">Schreibt</Status>
                      ) : (d.geprueft >> t) & 1 ? (
                        <Status ton="ok">Geprüft</Status>
                      ) : (
                        <Status ton="leise">Geschrieben</Status>
                      )}
                    </td>
                  ))}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

type Wiedererkannt =
  | { art: "gleich"; beginn: string; sicher: boolean }
  | { art: "nichtFormatiert"; beginn: string; sicher: boolean; bekannt: number; neue: number };

/** Kartengedächtnis: schon eingelesen, oder danach nicht formatiert (neue Clips zu schon gesicherten)? Nur Hinweis. */
function KartenGedaechtnis({ pfad }: { pfad: string }) {
  const [w, setW] = useState<Wiedererkannt | null>(null);
  useEffect(() => {
    let aus = false;
    setW(null);
    invoke<Wiedererkannt | null>("karte_wiedererkennen", { quelle: pfad })
      .then((x) => !aus && setW(x))
      .catch(() => {});
    return () => {
      aus = true;
    };
  }, [pfad]);
  if (!w) return null;
  const wann = new Date(w.beginn).toLocaleString("de-CH", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });
  return (
    <div className="gedaechtnis" role="status">
      {w.art === "gleich" ? (
        <Status ton={w.sicher ? "warn" : "leise"}>
          Diese Karte wurde am {wann} schon eingelesen{w.sicher ? " und ist sicher" : ", damals ohne Freigabe"}. Noch einmal
          einlesen ist nur nötig, wenn eine Kopie fehlt.
        </Status>
      ) : (
        <Status ton="warn">
          Karte wurde nach dem Einlesen am {wann} nicht formatiert: {w.bekannt} Clips von damals sind noch drauf,{" "}
          <b>
            {w.neue} {w.neue === 1 ? "Clip ist neu" : "Clips sind neu"} und noch nirgends gesichert
          </b>
          . Jetzt einlesen.
        </Status>
      )}
    </div>
  );
}
