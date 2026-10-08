// Projekt: Plan und Stand aus dem Plate Assistant, verbunden mit den eingelesenen Karten auf den Zielen.
// Beantwortet vor dem Formatieren: Ist von diesem Projekt alles da? Welche Takes haben noch keinen Clip?
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MapPinPlus, Plus, RefreshCw, Settings } from "lucide-react";
import { NeuerDrehort, NeuesProjekt, ProjektEinstellungen } from "../ProjektEinstellungen";
import { useEinstellungen } from "../einstellungen";
import { useKonto, type Projekt as ProjektT } from "../konto";
import { useLauf } from "../lauf";
import { Status, type Ton } from "../teile";

type TakeStand = { id: string; nummer: number; art: string; bewertung: string; clip: string; karte: string | null; freigegeben: boolean };
type PlateStand = { slate: string; name: string; fotos: number; hdri: string[]; takes: TakeStand[] };
type DrehStand = { id: string; name: string; datum: string; plates: PlateStand[]; hdri: string[] };
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
const HDRI: Record<string, string> = {
  aufnahme: "läuft",
  captured: "aufgenommen",
  uploaded: "hochgeladen",
  processed: "verarbeitet",
  linked: "verknüpft",
};

type Filter = "alle" | "fehlt" | "offen" | "sicher" | "klaeren";
type Zeile = { dreh: DrehStand; plate: PlateStand; take: TakeStand; ton: Ton; stand: string };

function stand(t: TakeStand): { ton: Ton; stand: string } {
  if (t.freigegeben) return { ton: "ok", stand: "Sicher" };
  if (t.karte) return { ton: "rot", stand: "Nicht freigegeben" };
  return { ton: "fehler", stand: "Karte fehlt" };
}

export function Projekt({ zurEinrichtung }: { zurEinrichtung: () => void }) {
  const konto = useKonto();
  const e = useEinstellungen();
  const lauf = useLauf();
  const verbunden = konto.verbindung === "verbunden";
  const [projekt, setProjekt] = useState<ProjektT | null>(lauf.paProjekt);
  const [kurzVonHand, setKurzVonHand] = useState("");
  const [u, setU] = useState<Uebersicht | null>(null);
  const [laedt, setLaedt] = useState(false);
  const [fehler, setFehler] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("alle");
  const [drehFilter, setDrehFilter] = useState("");
  const [ansicht, setAnsicht] = useState<"takes" | "karten">("takes");
  const [gewaehlt, setGewaehlt] = useState<string | null>(null);
  const [einstellen, setEinstellen] = useState(false);
  const [anlegen, setAnlegen] = useState(false);
  const [drehortNeu, setDrehortNeu] = useState(false);
  // Aktueller Stand des Projekts (nach dem Speichern neu geladen) für das Formular.
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

  useEffect(() => {
    if (projekt && !u) laden(projekt);
  }, []);

  const zeilen: Zeile[] =
    u?.drehs.flatMap((d) => d.plates.flatMap((p) => p.takes.map((t) => ({ dreh: d, plate: p, take: t, ...stand(t) })))) ?? [];
  const sichtbar = zeilen.filter(
    (z) =>
      (!drehFilter || z.dreh.id === drehFilter) &&
      (filter === "alle" ||
        (filter === "fehlt" && !z.take.karte) ||
        (filter === "offen" && z.take.karte && !z.take.freigegeben) ||
        (filter === "sicher" && z.take.freigegeben)),
  );
  const zahl: Record<Filter, number> = {
    alle: zeilen.length,
    fehlt: zeilen.filter((z) => !z.take.karte).length,
    offen: zeilen.filter((z) => z.take.karte && !z.take.freigegeben).length,
    sicher: zeilen.filter((z) => z.take.freigegeben).length,
    klaeren: u?.zuKlaeren.length ?? 0,
  };
  const auswahl = zeilen.find((z) => z.take.id === gewaehlt) ?? null;

  return (
    <div className="projekt">
      <div className="werkzeugleiste">
        {verbunden ? (
          <select
            aria-label="Projekt"
            value={projekt?.id ?? ""}
            onChange={(ev) => {
              const p = konto.projekte.find((x) => x.id === ev.target.value) ?? null;
              setProjekt(p);
              setU(null);
              setGewaehlt(null);
              if (p) laden(p);
            }}
          >
            <option value="">Projekt wählen …</option>
            {konto.projekte.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} ({p.kurzname}){p.aktiv ? "" : " · abgeschlossen"}
              </option>
            ))}
          </select>
        ) : (
          <>
            <input
              className="zahl"
              aria-label="Kurzname des Projekts"
              placeholder="KURZNAME"
              value={kurzVonHand}
              onChange={(ev) => setKurzVonHand(ev.target.value.toUpperCase())}
            />
            <button
              className="knopf"
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
        {verbunden && (
          <button
            className="knopf"
            aria-pressed={anlegen}
            onClick={() => {
              setAnlegen(!anlegen);
              setEinstellen(false);
              setDrehortNeu(false);
            }}
          >
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
            onClick={() => {
              setDrehortNeu(!drehortNeu);
              setAnlegen(false);
              setEinstellen(false);
            }}
          >
            <MapPinPlus size={14} strokeWidth={2} aria-hidden /> Neuer Drehort
          </button>
        )}
        {verbunden && projekt?.id && (
          <button
            className="knopf"
            aria-pressed={einstellen}
            onClick={() => {
              setEinstellen(!einstellen);
              setAnlegen(false);
              setDrehortNeu(false);
            }}
            title="Projekt-Einstellungen"
          >
            <Settings size={14} strokeWidth={2} aria-hidden /> Einstellungen
          </button>
        )}
        {projekt && (
          <button className="knopf" disabled={laedt} onClick={() => laden(projekt)}>
            <RefreshCw size={14} strokeWidth={2} aria-hidden className={laedt ? "dreht" : ""} /> {laedt ? "Lädt …" : "Neu laden"}
          </button>
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
              if (p) {
                setProjekt(p);
                setU(null);
                laden(p);
              }
            }}
          />
        </div>
      ) : einstellen && aktuell ? (
        <div className="projekt-formular">
          <ProjektEinstellungen
            projekt={aktuell}
            schliessen={(p) => {
              setEinstellen(false);
              if (p) setProjekt(p);
            }}
          />
        </div>
      ) : !u ? (
        <div className="leer">
          <h2>{laedt ? "Lädt die Übersicht …" : verbunden ? "Projekt wählen oder neu anlegen" : "Projekt wählen"}</h2>
          <p>
            Die Übersicht zeigt pro Drehort die gedrehten Takes, welche schon sicher kopiert sind und für welche die Karte
            noch fehlt. Eingelesene Karten werden auf den Zielen gesucht
            {e.ziele.length ? ` (${e.ziele.length} ${e.ziele.length === 1 ? "Ziel" : "Ziele"})` : ", es sind aber noch keine Ziele eingetragen"}.
          </p>
        </div>
      ) : (
        <div className={`projekt-rumpf ${filter === "klaeren" && ansicht === "takes" ? "projekt-rumpf-voll" : ""}`}>
          <div className="projekt-liste">
            <div className="reiter" role="tablist" aria-label="Ansicht">
              <button role="tab" aria-selected={ansicht === "takes"} className="reiter-knopf" onClick={() => setAnsicht("takes")}>
                Takes ({zeilen.length})
              </button>
              <button role="tab" aria-selected={ansicht === "karten"} className="reiter-knopf" onClick={() => setAnsicht("karten")}>
                Karten ({u.karten.length})
              </button>
              {ansicht === "takes" && (
                <div className="reiter-filter">
                  <div className="segment" role="group" aria-label="Filter">
                    {(
                      [
                        ["alle", "Alle"],
                        ["fehlt", "Karte fehlt"],
                        ["offen", "Nicht freigegeben"],
                        ["sicher", "Sicher"],
                        ["klaeren", "Zu klären"],
                      ] as [Filter, string][]
                    ).map(([id, text]) => (
                      <button key={id} aria-pressed={filter === id} onClick={() => setFilter(id)}>
                        {text} <span className="zahl leise">{zahl[id]}</span>
                      </button>
                    ))}
                  </div>
                  {u.drehs.length > 1 && (
                    <select aria-label="Drehort" value={drehFilter} onChange={(ev) => setDrehFilter(ev.target.value)}>
                      <option value="">Alle Drehorte</option>
                      {u.drehs.map((d) => (
                        <option key={d.id} value={d.id}>
                          {d.datum} · {d.name}
                        </option>
                      ))}
                    </select>
                  )}
                </div>
              )}
            </div>

            {ansicht === "takes" && filter === "klaeren" && (
              <ZuKlaeren
                u={u}
                neuerDrehort={() => {
                  setDrehortNeu(true);
                  setAnlegen(false);
                  setEinstellen(false);
                }}
                gespeichert={() => projekt && laden(projekt)}
              />
            )}
            {ansicht === "takes" &&
              filter !== "klaeren" &&
              (sichtbar.length ? (
                <table className="tabelle tabelle-waehlbar">
                  <thead>
                    <tr>
                      <th>Drehort</th>
                      <th>Slate</th>
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
                        aria-selected={gewaehlt === z.take.id}
                        tabIndex={0}
                        onClick={() => setGewaehlt(z.take.id)}
                        onKeyDown={(ev) => (ev.key === "Enter" || ev.key === " ") && setGewaehlt(z.take.id)}
                      >
                        <td className="ohne-umbruch">
                          {z.dreh.name} <span className="leise zahl">{z.dreh.datum.slice(5).split("-").reverse().join(".")}</span>
                        </td>
                        <td className="zahl">{z.plate.slate || "–"}</td>
                        <td className="ohne-umbruch">{z.plate.name}</td>
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
              ))}

            {ansicht === "karten" &&
              (u.karten.length ? (
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
                    {u.karten.map((k) => (
                      <tr key={k.datei}>
                        <td className="zahl">{k.inhalt.karte}</td>
                        <td className="zahl">{k.drehOrdner}</td>
                        <td className="rechts zahl">{k.inhalt.clips.length}</td>
                        <td>
                          <Status ton={k.inhalt.freigegeben ? "ok" : "rot"}>
                            {k.inhalt.freigegeben ? `Sicher · ${k.inhalt.unabhaengigeKopien} Kopien` : "Nicht freigegeben"}
                          </Status>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : (
                <p className="leer-zeile">Auf den Zielen liegt noch keine eingelesene Karte dieses Projekts.</p>
              ))}

          </div>

          {!(filter === "klaeren" && ansicht === "takes") && (
          <aside className="inspektor" aria-label="Einzelheiten">
            {auswahl ? (
              <>
                <div className="leiste-kopf">
                  <h2>
                    {auswahl.plate.slate && <span className="zahl">{auswahl.plate.slate} </span>}
                    {auswahl.plate.name}
                  </h2>
                </div>
                <dl className="eigenschaften">
                  <dt>Drehort</dt>
                  <dd>
                    {auswahl.dreh.name} · <span className="zahl">{auswahl.dreh.datum}</span>
                  </dd>
                  <dt>Fotos</dt>
                  <dd className="zahl">{auswahl.plate.fotos}</dd>
                  <dt>HDRI</dt>
                  <dd>
                    {[...auswahl.plate.hdri, ...auswahl.dreh.hdri].map((h) => HDRI[h] ?? h).join(", ") || <span className="leise">keins</span>}
                  </dd>
                </dl>
                <div className="leiste-kopf">
                  <h2>Takes dieser Plate</h2>
                </div>
                <ul className="inspektor-takes">
                  {auswahl.plate.takes.map((t) => {
                    const s = stand(t);
                    return (
                      <li key={t.id} className={t.id === auswahl.take.id ? "aktuell" : ""}>
                        <span className="zahl">Take {t.nummer}</span>
                        <span className="leise">{ART[t.art] ?? t.art}</span>
                        <Status ton={s.ton}>{t.karte ? `${t.karte}` : s.stand}</Status>
                      </li>
                    );
                  })}
                </ul>
              </>
            ) : (
              <p className="leiste-leer">Einen Take anklicken: hier erscheinen Plate, Fotos, HDRI und alle Takes dazu.</p>
            )}
          </aside>
          )}
        </div>
      )}
    </div>
  );
}

/** Zu klären: Clips auf den Karten ohne Take. Im Nachhinein einem Take oder nur einem Drehort zuordnen
 *  (auch einem neu angelegten). Gespeichert in der Zusammenfassung der Karte auf allen Zielen. */
function ZuKlaeren({ u, neuerDrehort, gespeichert }: { u: Uebersicht; neuerDrehort: () => void; gespeichert: () => void }) {
  const [wahl, setWahl] = useState<Record<string, { dreh: string; take: string }>>({});
  const [stand, setStand] = useState<Record<string, { ton: Ton; text: string }>>({});
  if (u.zuKlaeren.length === 0)
    return <p className="leer-zeile">Nichts zu klären: Jeder eingelesene Clip ist einem Take oder einem Drehort zugeordnet.</p>;

  async function zuordnen(c: OffenerClip, schluessel: string) {
    const w = wahl[schluessel];
    if (!w?.dreh) return;
    setStand({ ...stand, [schluessel]: { ton: "laeuft", text: "Speichert …" } });
    try {
      await invoke("clip_zuordnen", { dateien: c.dateien, clip: c.clip, takeId: w.take || null, drehId: w.dreh });
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
