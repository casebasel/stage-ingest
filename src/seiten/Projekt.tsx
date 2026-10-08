// Projekt: der Projektmanager des Ingest. Links der Baum Drehort → Plate, rechts die Einzelheiten (Fotos, HDRI,
// Takes, Karten). Beantwortet auch vor dem Formatieren: Ist von diesem Projekt alles da? Was ist zu klären?
// Das Projekt selbst wird oben links gewählt (gilt für alle Seiten). Bearbeiten von Plates und Takes kommt später
// (Systemkarte, Entscheidung „Projektmanager“ vom 08.10.2026); heute nur lesen, Projekte und Drehorte anlegen.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronRight, Image as Bild, MapPinPlus, Plus, RefreshCw, Settings, X } from "lucide-react";
import { NeuerDrehort, NeuesProjekt, ProjektEinstellungen } from "../ProjektEinstellungen";
import { useEinstellungen } from "../einstellungen";
import { useKonto, type Projekt as ProjektT, type Zugang } from "../konto";
import { useLauf } from "../lauf";
import { Status, type Ton } from "../teile";
import { kurz } from "./Einlesen";

type TakeStand = { id: string; nummer: number; art: string; bewertung: string; clip: string; karte: string | null; freigegeben: boolean };
type FotoStand = { id: string; art: string; pfad: string };
type HdriStand = {
  id: string;
  zustand: string;
  erstelltAm: string;
  job: string | null;
  vorschau: string | null;
  /** "dienst" = gerechnetes Panorama, "iphone" = Vorschau der Aufnahme vom iPhone. */
  vorschauQuelle: "dienst" | "iphone" | null;
};
type PlateStand = { id: string; nummer: number; slate: string; name: string; fotos: FotoStand[]; hdri: HdriStand[]; takes: TakeStand[] };
type DrehStand = { id: string; name: string; kurzname: string; datum: string; plates: PlateStand[]; hdri: HdriStand[]; karten: string[] };
type Karte = {
  drehOrdner: string;
  datei: string;
  inhalt: { karte: string; beginn: string; freigegeben: boolean; unabhaengigeKopien: number; clips: unknown[] };
};
type OffenerClip = {
  clip: string;
  karte: string;
  startTc: string | null;
  drehOrdner: string;
  freigegeben: boolean;
  dateien: string[];
};
type Uebersicht = {
  projekt: ProjektT;
  drehs: DrehStand[];
  karten: Karte[];
  zuKlaeren: OffenerClip[];
  unlesbar: string[];
  hinweis: string | null;
};

const ART: Record<string, string> = { graukugel: "Graukugel", chromkugel: "Chromkugel", cleanplate: "Cleanplate", take: "Take" };
const BEWERTUNG: Record<string, string> = { circle: "Favorit", gut: "Gut", schlecht: "Schlecht" };
const FOTO: Record<string, string> = { referenz: "Referenz", set: "Set", position: "Position" };

/** HDRI: Stand des HDRI-Dienstes, wenn es einen Job gibt, sonst der Stand im Plate Assistant. */
function hdriStand(h: HdriStand): { ton: Ton; text: string } {
  const job: Record<string, [Ton, string]> = {
    wartet: ["leise", "wartet auf den Dienst"],
    laeuft: ["laeuft", "wird gerechnet"],
    pausiert: ["leise", "pausiert (Stage läuft)"],
    processed: ["ok", "gerechnet"],
    fehler: ["fehler", "Fehler beim Rechnen"],
    verworfen: ["leise", "verworfen"],
    linked: ["ok", "freigegeben"],
  };
  const zustand: Record<string, [Ton, string]> = {
    aufnahme: ["laeuft", "Aufnahme läuft"],
    captured: ["leise", "aufgenommen, noch nicht hochgeladen"],
    uploaded: ["leise", "hochgeladen"],
    processed: ["ok", "gerechnet"],
    linked: ["ok", "freigegeben"],
  };
  const [ton, text] = (h.job && job[h.job]) || zustand[h.zustand] || ["leise", h.job ?? h.zustand];
  return { ton, text };
}

type Filter = "alle" | "fehlt" | "offen" | "sicher";
type Zeile = { dreh: DrehStand; plate: PlateStand; take: TakeStand; ton: Ton; stand: string };
type Auswahl = { art: "takes" } | { art: "klaeren" } | { art: "karten" } | { art: "dreh"; id: string } | { art: "plate"; id: string };

function stand(t: TakeStand): { ton: Ton; stand: string } {
  if (t.freigegeben) return { ton: "ok", stand: "Sicher" };
  if (t.karte) return { ton: "rot", stand: "Nicht freigegeben" };
  return { ton: "fehler", stand: "Karte fehlt" };
}

const datumKurz = (d: string) => (d ? d.slice(5).split("-").reverse().join(".") : "ohne Datum");
const plateTitel = (p: PlateStand) => p.slate || (p.nummer ? `P${String(p.nummer).padStart(3, "0")}` : "Plate");

export function Projekt({ zurEinrichtung }: { zurEinrichtung: () => void }) {
  const konto = useKonto();
  const e = useEinstellungen();
  const lauf = useLauf();
  const verbunden = konto.verbindung === "verbunden";
  // Das Projekt kommt aus der Kopfleiste; ohne Verbindung aus dem getippten Namen (nur die Karten auf den Zielen).
  const projekt: ProjektT | null = verbunden
    ? lauf.paProjekt
    : lauf.projektText.trim()
      ? { id: "", name: lauf.projektText.trim(), kurzname: kurz(lauf.projektText), aktiv: true }
      : null;
  const [u, setU] = useState<Uebersicht | null>(null);
  const [laedt, setLaedt] = useState(false);
  const [fehler, setFehler] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("alle");
  const [auswahl, setAuswahl] = useState<Auswahl>({ art: "takes" });
  const [zu, setZu] = useState<Set<string>>(new Set());
  const [einstellen, setEinstellen] = useState(false);
  const [anlegen, setAnlegen] = useState(false);
  const [drehortNeu, setDrehortNeu] = useState(false);
  const aktuell = konto.projekte.find((p) => p.id === projekt?.id) ?? projekt;

  async function laden(p: ProjektT) {
    setLaedt(true);
    setFehler(null);
    try {
      setU(await invoke<Uebersicht>("projekt_uebersicht", { zugang: verbunden ? konto.zugang : null, projekt: p, basis: e.ziele }));
    } catch (err) {
      setFehler(String(err));
    }
    setLaedt(false);
  }

  // Neu laden, sobald oben links ein anderes Projekt gewählt wird.
  const schluessel = `${projekt?.id}|${projekt?.kurzname}|${verbunden}`;
  useEffect(() => {
    setU(null);
    setAuswahl({ art: "takes" });
    if (projekt) laden(projekt);
  }, [schluessel]);

  const zeilen: Zeile[] =
    u?.drehs.flatMap((d) => d.plates.flatMap((p) => p.takes.map((t) => ({ dreh: d, plate: p, take: t, ...stand(t) })))) ?? [];
  const zahl: Record<Filter, number> = {
    alle: zeilen.length,
    fehlt: zeilen.filter((z) => !z.take.karte).length,
    offen: zeilen.filter((z) => z.take.karte && !z.take.freigegeben).length,
    sicher: zeilen.filter((z) => z.take.freigegeben).length,
  };
  const plates = u?.drehs.flatMap((d) => d.plates.map((p) => ({ dreh: d, plate: p }))) ?? [];
  const nurEins = (a: "anlegen" | "drehort" | "einstellen") => {
    setAnlegen(a === "anlegen" ? !anlegen : false);
    setDrehortNeu(a === "drehort" ? !drehortNeu : false);
    setEinstellen(a === "einstellen" ? !einstellen : false);
  };

  return (
    <div className="projekt">
      <div className="werkzeugleiste">
        {verbunden && (
          <button className="knopf" aria-pressed={anlegen} onClick={() => nurEins("anlegen")}>
            <Plus size={14} strokeWidth={2} aria-hidden /> Neues Projekt
          </button>
        )}
        {verbunden && projekt?.id && (
          <button
            className="knopf"
            aria-pressed={drehortNeu}
            // Erst wenn der Server Drehort-Kurznamen kennt (Migration 0017; danach hat jeder Drehort einen).
            disabled={!konto.drehs.some((d) => d.kurzname)}
            title={konto.drehs.some((d) => d.kurzname) ? undefined : "Kommt mit der Migration 0017 des Plate Assistant"}
            onClick={() => nurEins("drehort")}
          >
            <MapPinPlus size={14} strokeWidth={2} aria-hidden /> Neuer Drehort
          </button>
        )}
        {verbunden && projekt?.id && (
          <button className="knopf" aria-pressed={einstellen} onClick={() => nurEins("einstellen")} title="Projekt-Einstellungen">
            <Settings size={14} strokeWidth={2} aria-hidden /> Einstellungen
          </button>
        )}
        {projekt && (
          <button className="knopf" disabled={laedt} onClick={() => laden(projekt)}>
            <RefreshCw size={14} strokeWidth={2} aria-hidden className={laedt ? "dreht" : ""} /> {laedt ? "Lädt …" : "Neu laden"}
          </button>
        )}
        {u && (
          <dl className="kennzahlen">
            <div>
              <dt>Drehorte</dt>
              <dd className="zahl">{u.drehs.length}</dd>
            </div>
            <div>
              <dt>Plates</dt>
              <dd className="zahl">{plates.length}</dd>
            </div>
            <div>
              <dt>Takes sicher</dt>
              <dd className="zahl">
                {zahl.sicher}/{zahl.alle}
              </dd>
            </div>
            <div>
              <dt>Karten</dt>
              <dd className="zahl">{u.karten.length}</dd>
            </div>
          </dl>
        )}
      </div>

      {!verbunden && (
        <p className="hinweiszeile">
          Ohne Plate Assistant zeigt die Seite nur die eingelesenen Karten.{" "}
          <button className="verweis" onClick={zurEinrichtung}>
            In der Einrichtung verbinden
          </button>
        </p>
      )}
      {fehler && (
        <p className="hinweiszeile">
          <Status ton="fehler">{fehler}</Status>
        </p>
      )}
      {u?.hinweis && (
        <p className="hinweiszeile">
          <Status ton="warn">{u.hinweis}</Status>
        </p>
      )}

      {drehortNeu && aktuell ? (
        <div className="projekt-formular">
          <NeuerDrehort
            projekt={aktuell}
            fertig={(id) => {
              setDrehortNeu(false);
              if (id && projekt) laden(projekt);
            }}
          />
        </div>
      ) : anlegen ? (
        <div className="projekt-formular">
          <NeuesProjekt
            fertig={(p) => {
              setAnlegen(false);
              if (p) lauf.projektWaehlen(p);
            }}
          />
        </div>
      ) : einstellen && aktuell ? (
        <div className="projekt-formular">
          <ProjektEinstellungen
            projekt={aktuell}
            schliessen={(p) => {
              setEinstellen(false);
              if (p) lauf.setPaProjekt(konto.projekte.find((x) => x.id === p.id) ?? p);
            }}
          />
        </div>
      ) : !u ? (
        <div className="leer">
          <h2>{laedt ? "Lädt das Projekt …" : projekt ? "Projekt nicht geladen" : "Oben links ein Projekt wählen"}</h2>
          <p>
            Hier stehen alle Drehorte, Plates, Takes, Fotos und HDRI des Projekts, welche Takes schon sicher kopiert sind
            und für welche die Karte noch fehlt. Eingelesene Karten werden auf den Zielen gesucht
            {e.ziele.length ? ` (${e.ziele.length} ${e.ziele.length === 1 ? "Ziel" : "Ziele"})` : ", es sind aber noch keine Ziele eingetragen"}.
          </p>
        </div>
      ) : (
        <div className="projekt-rumpf">
          <nav className="baum" aria-label="Projekt">
            <ul>
              <BaumKnopf an={auswahl.art === "takes"} onClick={() => setAuswahl({ art: "takes" })} text="Alle Takes" zahl={zahl.alle} />
              <BaumKnopf
                an={auswahl.art === "klaeren"}
                onClick={() => setAuswahl({ art: "klaeren" })}
                text="Zu klären"
                zahl={u.zuKlaeren.length}
                warn={u.zuKlaeren.length > 0}
              />
              <BaumKnopf an={auswahl.art === "karten"} onClick={() => setAuswahl({ art: "karten" })} text="Karten" zahl={u.karten.length} />
            </ul>
            <h3 className="baum-titel">Drehorte</h3>
            {u.drehs.length === 0 && <p className="baum-leer">Noch kein Drehort. Oben „Neuer Drehort“.</p>}
            <ul>
              {u.drehs.map((d) => {
                const offen = !zu.has(d.id);
                const takes = d.plates.flatMap((p) => p.takes);
                return (
                  <li key={d.id}>
                    <div className="baum-zeile">
                      <button
                        className="baum-klappe"
                        aria-label={offen ? `${d.name} zuklappen` : `${d.name} aufklappen`}
                        aria-expanded={offen}
                        disabled={d.plates.length === 0}
                        onClick={() => {
                          const n = new Set(zu);
                          if (offen) n.add(d.id);
                          else n.delete(d.id);
                          setZu(n);
                        }}
                      >
                        {d.plates.length > 0 && (offen ? <ChevronDown size={14} /> : <ChevronRight size={14} />)}
                      </button>
                      <button
                        className="baum-knoten"
                        aria-current={auswahl.art === "dreh" && auswahl.id === d.id ? "true" : undefined}
                        onClick={() => setAuswahl({ art: "dreh", id: d.id })}
                      >
                        <span className="baum-name">{d.name}</span>
                        <span className="baum-info zahl">
                          {d.kurzname && `${d.kurzname} · `}
                          {datumKurz(d.datum)}
                          {takes.length > 0 && ` · ${takes.filter((t) => t.freigegeben).length}/${takes.length}`}
                        </span>
                      </button>
                    </div>
                    {offen && d.plates.length > 0 && (
                      <ul className="baum-kinder">
                        {d.plates.map((p) => {
                          const fehlt = p.takes.some((t) => !t.karte);
                          return (
                            <li key={p.id}>
                              <button
                                className="baum-knoten"
                                aria-current={auswahl.art === "plate" && auswahl.id === p.id ? "true" : undefined}
                                onClick={() => setAuswahl({ art: "plate", id: p.id })}
                              >
                                <span className="baum-name">
                                  <span className="zahl">{plateTitel(p)}</span> {p.name}
                                </span>
                                <span className="baum-info zahl">
                                  {p.takes.length} {p.takes.length === 1 ? "Take" : "Takes"}
                                  {p.fotos.length > 0 && ` · ${p.fotos.length} Fotos`}
                                  {p.hdri.length > 0 && " · HDRI"}
                                  {fehlt && " · Karte fehlt"}
                                </span>
                              </button>
                            </li>
                          );
                        })}
                      </ul>
                    )}
                  </li>
                );
              })}
            </ul>
          </nav>

          <section className="detail" aria-label="Einzelheiten">
            {auswahl.art === "takes" && (
              <AlleTakes zeilen={zeilen} zahl={zahl} filter={filter} setFilter={setFilter} plateWaehlen={(id) => setAuswahl({ art: "plate", id })} />
            )}
            {auswahl.art === "klaeren" && (
              <ZuKlaeren
                zugang={verbunden ? konto.zugang : null}
                u={u}
                neuerDrehort={() => nurEins("drehort")}
                gespeichert={() => projekt && laden(projekt)}
              />
            )}
            {auswahl.art === "karten" && <KartenListe karten={u.karten} />}
            {auswahl.art === "dreh" &&
              (() => {
                const d = u.drehs.find((x) => x.id === auswahl.id);
                return d ? <DrehDetail d={d} plateWaehlen={(id) => setAuswahl({ art: "plate", id })} /> : null;
              })()}
            {auswahl.art === "plate" &&
              (() => {
                const x = plates.find((x) => x.plate.id === auswahl.id);
                return x ? <PlateDetail dreh={x.dreh} p={x.plate} drehWaehlen={() => setAuswahl({ art: "dreh", id: x.dreh.id })} /> : null;
              })()}
          </section>
        </div>
      )}
    </div>
  );
}

function BaumKnopf({ an, onClick, text, zahl, warn }: { an: boolean; onClick: () => void; text: string; zahl: number; warn?: boolean }) {
  return (
    <li>
      <button className="baum-knoten baum-oben" aria-current={an ? "true" : undefined} onClick={onClick}>
        <span className="baum-name">{warn ? <Status ton="warn">{text}</Status> : text}</span>
        <span className="baum-info zahl">{zahl}</span>
      </button>
    </li>
  );
}

function AlleTakes({
  zeilen,
  zahl,
  filter,
  setFilter,
  plateWaehlen,
}: {
  zeilen: Zeile[];
  zahl: Record<Filter, number>;
  filter: Filter;
  setFilter: (f: Filter) => void;
  plateWaehlen: (id: string) => void;
}) {
  const sichtbar = zeilen.filter(
    (z) =>
      filter === "alle" ||
      (filter === "fehlt" && !z.take.karte) ||
      (filter === "offen" && z.take.karte && !z.take.freigegeben) ||
      (filter === "sicher" && z.take.freigegeben),
  );
  return (
    <>
      <div className="detail-kopf">
        <h2>Alle Takes</h2>
        <div className="segment" role="group" aria-label="Filter">
          {(
            [
              ["alle", "Alle"],
              ["fehlt", "Karte fehlt"],
              ["offen", "Nicht freigegeben"],
              ["sicher", "Sicher"],
            ] as [Filter, string][]
          ).map(([id, text]) => (
            <button key={id} aria-pressed={filter === id} onClick={() => setFilter(id)}>
              {text} <span className="zahl leise">{zahl[id]}</span>
            </button>
          ))}
        </div>
      </div>
      {sichtbar.length ? (
        <table className="tabelle tabelle-waehlbar">
          <thead>
            <tr>
              <th>Drehort</th>
              <th>Plate</th>
              <th className="rechts">Take</th>
              <th>Art · Bewertung</th>
              <th>Clip</th>
              <th>Karte</th>
              <th>Stand</th>
            </tr>
          </thead>
          <tbody>
            {sichtbar.map((z) => (
              <tr
                key={z.take.id}
                tabIndex={0}
                title="Plate öffnen"
                onClick={() => plateWaehlen(z.plate.id)}
                onKeyDown={(ev) => (ev.key === "Enter" || ev.key === " ") && plateWaehlen(z.plate.id)}
              >
                <td className="ohne-umbruch">
                  {z.dreh.name} <span className="leise zahl">{datumKurz(z.dreh.datum)}</span>
                </td>
                <td className="ohne-umbruch">
                  <span className="zahl">{plateTitel(z.plate)}</span> {z.plate.name}
                </td>
                <td className="rechts zahl">{z.take.nummer}</td>
                <td className="ohne-umbruch">
                  <span className="leise">{ART[z.take.art] ?? z.take.art}</span>
                  {BEWERTUNG[z.take.bewertung] && ` · ${BEWERTUNG[z.take.bewertung]}`}
                </td>
                <td className="zahl">{z.take.clip || <span className="leise">–</span>}</td>
                <td className="zahl">{z.take.karte ?? <span className="leise">–</span>}</td>
                <td>
                  <Status ton={z.ton}>{z.stand}</Status>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className="leer-zeile">
          {zeilen.length ? "Kein Take passt zum Filter." : "Für dieses Projekt sind im Plate Assistant noch keine Takes erfasst."}
        </p>
      )}
    </>
  );
}

function KartenListe({ karten }: { karten: Karte[] }) {
  return (
    <>
      <div className="detail-kopf">
        <h2>Eingelesene Karten</h2>
      </div>
      {karten.length ? (
        <table className="tabelle">
          <thead>
            <tr>
              <th>Karte</th>
              <th>Drehordner</th>
              <th className="rechts">Clips</th>
              <th>Freigabe</th>
            </tr>
          </thead>
          <tbody>
            {karten.map((k) => (
              <tr key={k.datei}>
                <td className="zahl">{k.inhalt.karte}</td>
                <td className="zahl">{k.drehOrdner}</td>
                <td className="rechts zahl">{k.inhalt.clips.length}</td>
                <td>
                  <Status ton={k.inhalt.freigegeben ? "ok" : "rot"}>
                    {k.inhalt.freigegeben
                      ? `Sicher · ${k.inhalt.unabhaengigeKopien} Kopien`
                      : `Nicht freigegeben · ${k.inhalt.unabhaengigeKopien} ${k.inhalt.unabhaengigeKopien === 1 ? "Kopie" : "Kopien"} · beim Einlesen ergänzen`}
                  </Status>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className="leer-zeile">Auf den Zielen liegt noch keine eingelesene Karte dieses Projekts.</p>
      )}
    </>
  );
}

function DrehDetail({ d, plateWaehlen }: { d: DrehStand; plateWaehlen: (id: string) => void }) {
  const takes = d.plates.flatMap((p) => p.takes);
  return (
    <>
      <div className="detail-kopf">
        <h2>{d.name}</h2>
      </div>
      <dl className="eigenschaften detail-eigenschaften">
        <dt>Kurzname</dt>
        <dd className="zahl">{d.kurzname || <span className="leise">– (alter Drehort)</span>}</dd>
        <dt>Datum</dt>
        <dd className="zahl">{d.datum || <span className="leise">ohne Datum (Aufnahmedatum der Karte)</span>}</dd>
        <dt>Takes</dt>
        <dd className="zahl">
          {takes.filter((t) => t.freigegeben).length} von {takes.length} sicher
        </dd>
        <dt>Karten</dt>
        <dd className="zahl">{d.karten.join(", ") || <span className="leise">noch keine eingelesen</span>}</dd>
      </dl>
      {d.hdri.length > 0 && (
        <>
          <h3 className="detail-titel">HDRI des Drehorts</h3>
          <HdriListe hdri={d.hdri} />
        </>
      )}
      <h3 className="detail-titel">Plates</h3>
      {d.plates.length ? (
        <table className="tabelle tabelle-waehlbar">
          <thead>
            <tr>
              <th>Plate</th>
              <th>Name</th>
              <th className="rechts">Fotos</th>
              <th>HDRI</th>
              <th className="rechts">Takes</th>
              <th>Stand</th>
            </tr>
          </thead>
          <tbody>
            {d.plates.map((p) => {
              const sicher = p.takes.filter((t) => t.freigegeben).length;
              const fehlt = p.takes.filter((t) => !t.karte).length;
              return (
                <tr
                  key={p.id}
                  tabIndex={0}
                  onClick={() => plateWaehlen(p.id)}
                  onKeyDown={(ev) => (ev.key === "Enter" || ev.key === " ") && plateWaehlen(p.id)}
                >
                  <td className="zahl">{plateTitel(p)}</td>
                  <td>{p.name}</td>
                  <td className="rechts zahl">{p.fotos.length}</td>
                  <td>{p.hdri.length ? <Status {...hdriTon(p.hdri[0])} /> : <span className="leise">–</span>}</td>
                  <td className="rechts zahl">{p.takes.length}</td>
                  <td>
                    {p.takes.length === 0 ? (
                      <span className="leise">noch nicht gedreht</span>
                    ) : fehlt ? (
                      <Status ton="fehler">{fehlt === 1 ? "1 Karte fehlt" : `${fehlt} Karten fehlen`}</Status>
                    ) : sicher === p.takes.length ? (
                      <Status ton="ok">Sicher</Status>
                    ) : (
                      <Status ton="rot">Nicht freigegeben</Status>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      ) : (
        <p className="leer-zeile">An diesem Drehort ist noch keine Plate angelegt. Plates legt heute das iPhone an.</p>
      )}
    </>
  );
}

const hdriTon = (h: HdriStand) => {
  const s = hdriStand(h);
  return { ton: s.ton, children: s.text };
};

function PlateDetail({ dreh, p, drehWaehlen }: { dreh: DrehStand; p: PlateStand; drehWaehlen: () => void }) {
  const [gross, setGross] = useState<number | null>(null);
  return (
    <>
      <div className="detail-kopf">
        <h2>
          <span className="zahl">{plateTitel(p)}</span> {p.name}
        </h2>
        <button className="verweis" onClick={drehWaehlen}>
          {dreh.name} · {datumKurz(dreh.datum)}
        </button>
      </div>
      <h3 className="detail-titel">Fotos ({p.fotos.length})</h3>
      {p.fotos.length ? (
        <ul className="foto-raster">
          {p.fotos.map((f, i) => (
            <li key={f.id}>
              <button className="foto-knopf" onClick={() => setGross(i)} aria-label={`Foto ${i + 1} gross anzeigen`}>
                <Vorschau bucket="fotos" pfad={f.pfad} breite={320} alt={FOTO[f.art] ?? f.art} />
              </button>
              {f.art && <span className="foto-art">{FOTO[f.art] ?? f.art}</span>}
            </li>
          ))}
        </ul>
      ) : (
        <p className="leer-zeile">Keine Fotos.</p>
      )}
      <h3 className="detail-titel">HDRI</h3>
      {p.hdri.length ? <HdriListe hdri={p.hdri} /> : <p className="leer-zeile">Kein HDRI an dieser Plate.</p>}
      <h3 className="detail-titel">Takes</h3>
      {p.takes.length ? (
        <table className="tabelle">
          <thead>
            <tr>
              <th className="rechts">Take</th>
              <th>Art · Bewertung</th>
              <th>Clip</th>
              <th>Karte</th>
              <th>Stand</th>
            </tr>
          </thead>
          <tbody>
            {p.takes.map((t) => {
              const s = stand(t);
              return (
                <tr key={t.id}>
                  <td className="rechts zahl">{t.nummer}</td>
                  <td className="ohne-umbruch">
                    <span className="leise">{ART[t.art] ?? t.art}</span>
                    {BEWERTUNG[t.bewertung] && ` · ${BEWERTUNG[t.bewertung]}`}
                  </td>
                  <td className="zahl">{t.clip || <span className="leise">–</span>}</td>
                  <td className="zahl">{t.karte ?? <span className="leise">–</span>}</td>
                  <td>
                    <Status ton={s.ton}>{s.stand}</Status>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      ) : (
        <p className="leer-zeile">Noch keine Takes.</p>
      )}
      {gross !== null && <Grossbild fotos={p.fotos} index={gross} setIndex={setGross} />}
    </>
  );
}

function HdriListe({ hdri }: { hdri: HdriStand[] }) {
  const [gross, setGross] = useState<HdriStand | null>(null);
  return (
    <ul className="hdri-liste">
      {hdri.map((h) => {
        const s = hdriStand(h);
        return (
          <li key={h.id}>
            {h.vorschau ? (
              <button className="foto-knopf hdri-bild" onClick={() => setGross(h)} aria-label="HDRI gross anzeigen">
                <Vorschau bucket="hdri" pfad={h.vorschau} breite={640} alt="HDRI" />
              </button>
            ) : (
              <div className="hdri-bild hdri-leer">
                <Bild size={20} strokeWidth={1.5} aria-hidden />
                <span>Noch keine Vorschau</span>
              </div>
            )}
            <div className="hdri-text">
              <Status ton={s.ton}>{s.text}</Status>
              {h.vorschauQuelle === "iphone" && <span className="leise">Bild: Vorschau vom iPhone, nicht das gerechnete HDRI</span>}
              {h.erstelltAm && <span className="leise zahl">{new Date(h.erstelltAm).toLocaleString("de-CH")}</span>}
            </div>
          </li>
        );
      })}
      {gross?.vorschau && (
        <Grossbild fotos={[{ id: gross.id, art: "", pfad: gross.vorschau }]} bucket="hdri" index={0} setIndex={(i) => i === null && setGross(null)} />
      )}
    </ul>
  );
}

// Vorschaubilder einmal pro Sitzung laden (die App speichert sie zusätzlich auf der Platte).
const vorschauen = new Map<string, Promise<string>>();

function Vorschau({ bucket, pfad, breite, alt }: { bucket: string; pfad: string; breite: number; alt: string }) {
  const konto = useKonto();
  const [url, setUrl] = useState<string | null>(null);
  const [fehler, setFehler] = useState(false);
  useEffect(() => {
    const k = `${bucket}|${pfad}|${breite}`;
    let p = vorschauen.get(k);
    if (!p) {
      p = invoke<string>("bild_vorschau", { zugang: konto.zugang, bucket, pfad, breite });
      vorschauen.set(k, p);
      p.catch(() => vorschauen.delete(k));
    }
    let aktiv = true;
    p.then((u) => aktiv && setUrl(u)).catch(() => aktiv && setFehler(true));
    return () => {
      aktiv = false;
    };
  }, [bucket, pfad, breite]);
  if (fehler) return <span className="vorschau-platz">nicht ladbar</span>;
  if (!url) return <span className="vorschau-platz" aria-busy="true" />;
  return <img src={url} alt={alt} loading="lazy" />;
}

function Grossbild({
  fotos,
  index,
  setIndex,
  bucket = "fotos",
}: {
  fotos: FotoStand[];
  index: number;
  setIndex: (i: number | null) => void;
  bucket?: string;
}) {
  useEffect(() => {
    const taste = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") setIndex(null);
      if (ev.key === "ArrowRight" && index < fotos.length - 1) setIndex(index + 1);
      if (ev.key === "ArrowLeft" && index > 0) setIndex(index - 1);
    };
    window.addEventListener("keydown", taste);
    return () => window.removeEventListener("keydown", taste);
  }, [index, fotos.length]);
  const f = fotos[index];
  return (
    <div className="grossbild" role="dialog" aria-modal="true" aria-label="Bild gross" onClick={() => setIndex(null)}>
      <button className="knopf-symbol grossbild-zu" aria-label="Schliessen (Esc)" onClick={() => setIndex(null)}>
        <X size={18} />
      </button>
      <div className="grossbild-bild" onClick={(ev) => ev.stopPropagation()}>
        <Vorschau bucket={bucket} pfad={f.pfad} breite={2048} alt={FOTO[f.art] ?? (f.art || "Bild")} />
      </div>
      {fotos.length > 1 && (
        <span className="grossbild-zahl zahl">
          {index + 1} / {fotos.length} · ← → blättern
        </span>
      )}
    </div>
  );
}

/** Zu klären: Clips auf den Karten ohne Take. Im Nachhinein einem Take oder nur einem Drehort zuordnen
 *  (auch einem neu angelegten). Gespeichert in der Zusammenfassung der Karte auf allen Zielen und, wenn die Karte
 *  in der gemeinsamen Datenbank steht, auch dort (der Plate Assistant zeigt es dann an). */
function ZuKlaeren({
  u,
  zugang,
  neuerDrehort,
  gespeichert,
}: {
  u: Uebersicht;
  zugang: Zugang | null;
  neuerDrehort: () => void;
  gespeichert: () => void;
}) {
  const [wahl, setWahl] = useState<Record<string, { dreh: string; take: string }>>({});
  const [stand, setStand] = useState<Record<string, { ton: Ton; text: string }>>({});
  if (u.zuKlaeren.length === 0)
    return <p className="leer-zeile">Nichts zu klären: Jeder eingelesene Clip ist einem Take oder einem Drehort zugeordnet.</p>;

  async function zuordnen(c: OffenerClip, schluessel: string) {
    const w = wahl[schluessel];
    if (!w?.dreh) return;
    setStand({ ...stand, [schluessel]: { ton: "laeuft", text: "Speichert …" } });
    try {
      await invoke("clip_zuordnen", { zugang, dateien: c.dateien, clip: c.clip, takeId: w.take || null, drehId: w.dreh });
      setStand({ ...stand, [schluessel]: { ton: "ok", text: "Zugeordnet" } });
      gespeichert();
    } catch (e) {
      setStand({ ...stand, [schluessel]: { ton: "fehler", text: String(e) } });
    }
  }

  return (
    <table className="tabelle">
      <thead>
        <tr>
          <th>Clip</th>
          <th>Karte</th>
          <th>Start</th>
          <th>Liegt in</th>
          <th>Drehort</th>
          <th>Take</th>
          <th className="spalte-aktion">
            <span className="unsichtbar">Zuordnen</span>
          </th>
        </tr>
      </thead>
      <tbody>
        {u.zuKlaeren.map((c) => {
          const schluessel = `${c.karte}|${c.clip}`;
          const w = wahl[schluessel] ?? { dreh: "", take: "" };
          const dreh = u.drehs.find((d) => d.id === w.dreh);
          const st = stand[schluessel];
          return (
            <tr key={schluessel}>
              <td className="zahl">
                <Status ton="warn">{c.clip}</Status>
                {st && (
                  <span className="unterzeile">
                    <Status ton={st.ton}>{st.text}</Status>
                  </span>
                )}
              </td>
              <td className="zahl">{c.karte}</td>
              <td className="zahl">{c.startTc ?? "–"}</td>
              <td className="zahl leise">{c.drehOrdner}</td>
              <td>
                <select
                  aria-label={`Drehort für ${c.clip}`}
                  value={w.dreh}
                  onChange={(e) => {
                    if (e.target.value === "__neu") return neuerDrehort();
                    setWahl({ ...wahl, [schluessel]: { dreh: e.target.value, take: "" } });
                  }}
                >
                  <option value="">Drehort wählen …</option>
                  {u.drehs.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.name}
                      {d.datum && ` · ${d.datum}`}
                    </option>
                  ))}
                  <option value="__neu">+ Neuer Drehort …</option>
                </select>
              </td>
              <td>
                <select
                  aria-label={`Take für ${c.clip}`}
                  value={w.take}
                  disabled={!dreh}
                  onChange={(e) => setWahl({ ...wahl, [schluessel]: { ...w, take: e.target.value } })}
                >
                  <option value="">Ohne Take (nur Drehort)</option>
                  {dreh?.plates.flatMap((p) =>
                    p.takes.map((t) => (
                      <option key={t.id} value={t.id}>
                        {p.slate || p.name} · Take {t.nummer}
                        {t.clip ? ` (hat schon ${t.clip})` : ""}
                      </option>
                    )),
                  )}
                </select>
              </td>
              <td className="spalte-aktion">
                <button className="knopf knopf-klein" disabled={!w.dreh || st?.ton === "laeuft"} onClick={() => zuordnen(c, schluessel)}>
                  Zuordnen
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
