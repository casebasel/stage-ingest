// Projekt: Plan und Stand aus dem Plate Assistant, verbunden mit den eingelesenen Karten auf den Zielen.
// Beantwortet vor dem Formatieren: Ist von diesem Projekt alles da? Welche Takes haben noch keinen Clip?
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "lucide-react";
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
type Uebersicht = { projekt: ProjektT; drehs: DrehStand[]; karten: Karte[]; ohneTake: string[]; unlesbar: string[]; hinweis: string | null };

const ART: Record<string, string> = { graukugel: "Graukugel", chromkugel: "Chromkugel", cleanplate: "Cleanplate", take: "Take" };
const BEWERTUNG: Record<string, string> = { circle: "Favorit", gut: "Gut", schlecht: "Schlecht" };
const HDRI: Record<string, string> = {
  aufnahme: "läuft",
  captured: "aufgenommen",
  uploaded: "hochgeladen",
  processed: "verarbeitet",
  linked: "verknüpft",
};

type Filter = "alle" | "fehlt" | "offen" | "sicher";
type Zeile = { dreh: DrehStand; plate: PlateStand; take: TakeStand; ton: Ton; stand: string };

function stand(t: TakeStand): { ton: Ton; stand: string } {
  if (t.freigegeben) return { ton: "ok", stand: "Sicher" };
  if (t.karte) return { ton: "warn", stand: "Nicht freigegeben" };
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
  const [ansicht, setAnsicht] = useState<"takes" | "karten" | "klaerung">("takes");
  const [gewaehlt, setGewaehlt] = useState<string | null>(null);

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
  const zahl = { alle: zeilen.length, fehlt: zeilen.filter((z) => !z.take.karte).length, offen: zeilen.filter((z) => z.take.karte && !z.take.freigegeben).length, sicher: zeilen.filter((z) => z.take.freigegeben).length };
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

      {!u ? (
        <div className="leer">
          <h2>{laedt ? "Lädt die Übersicht …" : "Projekt wählen"}</h2>
          <p>
            Die Übersicht zeigt pro Drehort die gedrehten Takes, welche schon sicher kopiert sind und für welche die Karte
            noch fehlt. Eingelesene Karten werden auf den Zielen gesucht
            {e.ziele.length ? ` (${e.ziele.length} ${e.ziele.length === 1 ? "Ziel" : "Ziele"})` : ", es sind aber noch keine Ziele eingetragen"}.
          </p>
        </div>
      ) : (
        <div className="projekt-rumpf">
          <div className="projekt-liste">
            <div className="reiter" role="tablist" aria-label="Ansicht">
              <button role="tab" aria-selected={ansicht === "takes"} className="reiter-knopf" onClick={() => setAnsicht("takes")}>
                Takes ({zeilen.length})
              </button>
              <button role="tab" aria-selected={ansicht === "karten"} className="reiter-knopf" onClick={() => setAnsicht("karten")}>
                Karten ({u.karten.length})
              </button>
              <button role="tab" aria-selected={ansicht === "klaerung"} className="reiter-knopf" onClick={() => setAnsicht("klaerung")}>
                Klärung ({u.ohneTake.length})
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

            {ansicht === "takes" &&
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
                          <Status ton={k.inhalt.freigegeben ? "ok" : "warn"}>
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

            {ansicht === "klaerung" &&
              (u.ohneTake.length ? (
                <table className="tabelle">
                  <thead>
                    <tr>
                      <th>Clip ohne Take</th>
                    </tr>
                  </thead>
                  <tbody>
                    {u.ohneTake.map((c) => (
                      <tr key={c}>
                        <td>
                          <Status ton="warn">
                            <span className="zahl">{c}</span>
                          </Status>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : (
                <p className="leer-zeile">Jeder eingelesene Clip ist einem Take zugeordnet.</p>
              ))}
          </div>

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
        </div>
      )}
    </div>
  );
}
