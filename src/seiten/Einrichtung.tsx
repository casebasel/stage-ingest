// Einrichtung: alles, was man einmal pro Rechner einstellt. Am Set zeigt der Kopf nur, ob es steht.
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Moon, Sun, SunMoon } from "lucide-react";
import { useEinstellungen } from "../einstellungen";
import { useKonto, VORGABE_ADRESSE, VORGABE_ANON } from "../konto";
import { useLauf } from "../lauf";
import { Feld, Status } from "../teile";
import type { Wahl } from "../thema";

export function Einrichtung({ wahl, setWahl }: { wahl: Wahl; setWahl: (w: Wahl) => void }) {
  const e = useEinstellungen();
  const lauf = useLauf();
  const gesperrt = lauf.laeuft;
  const [schwelleFrage, setSchwelleFrage] = useState(false);

  return (
    <div className="einrichtung">
      <div className="einrichtung-spalte">
        <Konto />

        <section className="block" aria-labelledby="t-stage">
          <div className="block-kopf">
            <h2 id="t-stage">Stage (im Studio)</h2>
            {lauf.soll && !lauf.soll.fehler && <Status ton="ok">{lauf.soll.liste.length} Takes geladen</Status>}
            {lauf.soll?.fehler && <Status ton="warn">Nicht erreichbar</Status>}
          </div>
          <Feld
            name="Adresse des Stage-Servers"
            hilfe="Gedrehte Takes kommen als Soll-Liste; nach dem Einlesen wird die Karte an die Stage gemeldet. Leer lassen ausserhalb des Studios."
          >
            <input
              className="zahl"
              aria-label="Adresse des Stage-Servers"
              placeholder="z. B. http://stage-server:4400"
              value={e.stageAdresse}
              disabled={gesperrt}
              spellCheck={false}
              onChange={(ev) => e.setStageAdresse(ev.target.value)}
            />
            <button className="knopf" onClick={lauf.sollLaden} disabled={gesperrt || !e.stageAdresse.trim()}>
              Verbindung testen
            </button>
          </Feld>
          {lauf.soll?.fehler && (
            <p className="feld-meldung">
              <Status ton="warn">{lauf.soll.fehler}</Status>
            </p>
          )}
        </section>

        <section className="block" aria-labelledby="t-pruef">
          <div className="block-kopf">
            <h2 id="t-pruef">Prüfung</h2>
          </div>
          <Feld
            name="Unabhängige Kopien für die Freigabe"
            hilfe="„Sicher zum Formatieren“ erst, wenn so viele geprüfte Kopien auf bewiesen verschiedenen Platten liegen."
          >
            <input
              className="zahl eingabe-kurz"
              aria-label="Unabhängige Kopien für die Freigabe"
              type="number"
              min={1}
              max={9}
              value={e.mindestKopien}
              disabled={gesperrt}
              onChange={(ev) => {
                const n = Math.min(9, Math.max(1, Number(ev.target.value) || 1));
                if (n < 2) setSchwelleFrage(true);
                else {
                  setSchwelleFrage(false);
                  e.setMindestKopien(n);
                }
              }}
            />
          </Feld>
          {schwelleFrage && (
            <div className="rueckfrage" role="alertdialog" aria-label="Schwelle senken">
              <Status ton="warn">
                Freigabe schon bei einer einzigen Kopie? Fällt diese Platte aus, ist das Material weg. Nur für Tests; bleibt
                gemerkt, bis du sie wieder erhöhst (oben steht dann „Testschwelle“).
              </Status>
              <div className="knopfreihe">
                <button
                  className="knopf knopf-gefahr"
                  onClick={() => {
                    e.setMindestKopien(1);
                    setSchwelleFrage(false);
                  }}
                >
                  Ja, 1 Kopie reicht
                </button>
                <button className="knopf" onClick={() => setSchwelleFrage(false)}>
                  Bei {e.mindestKopien} bleiben
                </button>
              </div>
            </div>
          )}
          <Feld name="Prüfsumme" hilfe="Jedes Ziel wird nach dem Kopieren vollständig und ohne Cache zurückgelesen.">
            <span className="zahl">XXH3-128{e.mitMd5 ? " + MD5" : ""}</span>
          </Feld>
          <Feld name="Zusätzlich MD5" hilfe="Für Häuser, die MD5 verlangen. Kostet Rechenzeit.">
            <input type="checkbox" role="switch" aria-label="Zusätzlich MD5" checked={e.mitMd5} disabled={gesperrt} onChange={(ev) => e.setMitMd5(ev.target.checked)} />
          </Feld>
          <Feld name="Karte zweimal lesen" hilfe="Erkennt einen Kartenleser, der unzuverlässig liefert. Dauert länger.">
            <input
              type="checkbox"
              role="switch"
              aria-label="Karte zweimal lesen"
              checked={e.zweimalLesen}
              disabled={gesperrt}
              onChange={(ev) => e.setZweimalLesen(ev.target.checked)}
            />
          </Feld>
        </section>

        <section className="block" aria-labelledby="t-art">
          <div className="block-kopf">
            <h2 id="t-art">Bewegungsdaten</h2>
            {e.artCmd.trim() ? <Status ton="ok">Eingetragen</Status> : <Status ton="leise">Aus</Status>}
          </div>
          <Feld name="ARRI ART CMD" hilfe="Neigung, Rollen und Brennweite pro Bild in die Metadaten jeder Karte. Optional.">
            <input
              className="zahl"
              aria-label="Pfad zu ARRI ART CMD"
              placeholder="z. B. /Applications/ARRI/art-cmd"
              value={e.artCmd}
              disabled={gesperrt}
              spellCheck={false}
              onChange={(ev) => e.setArtCmd(ev.target.value)}
            />
            <button
              className="knopf"
              disabled={gesperrt}
              onClick={async () => {
                const p = await open({ directory: false, title: "ART CMD wählen" });
                if (typeof p === "string") e.setArtCmd(p);
              }}
            >
              Wählen …
            </button>
          </Feld>
          <ArtCmdLaden gesperrt={!!gesperrt} gesetzt={(p) => e.setArtCmd(p)} />
        </section>

        <section className="block" aria-labelledby="t-darst">
          <div className="block-kopf">
            <h2 id="t-darst">Darstellung</h2>
          </div>
          <Feld name="Hell oder dunkel" hilfe="Tag für Sonne draussen, Nacht für das dunkle Studio. Automatisch folgt dem System.">
            <div className="segment" role="group" aria-label="Darstellung">
              {(
                [
                  ["auto", "Automatisch", SunMoon],
                  ["tag", "Tag", Sun],
                  ["nacht", "Nacht", Moon],
                ] as const
              ).map(([id, text, Z]) => (
                <button key={id} aria-pressed={wahl === id} onClick={() => setWahl(id)}>
                  <Z size={14} strokeWidth={2} aria-hidden /> {text}
                </button>
              ))}
            </div>
          </Feld>
        </section>
      </div>
    </div>
  );
}

function Konto() {
  const k = useKonto();
  const lauf = useLauf();
  const [passwort, setPasswort] = useState("");
  const [erweitert, setErweitert] = useState(!VORGABE_ADRESSE || !VORGABE_ANON || k.eigenerZugang);
  const gesperrt = lauf.laeuft;
  const ton = k.verbindung === "verbunden" ? "ok" : k.verbindung === "fehler" ? "fehler" : k.verbindung === "verbindet" ? "laeuft" : "leise";
  const text =
    k.verbindung === "verbunden"
      ? `Verbunden · ${k.projekte.length} ${k.projekte.length === 1 ? "Projekt" : "Projekte"} · ${k.drehs.length} ${
          k.drehs.length === 1 ? "Drehort" : "Drehorte"
        }`
      : k.verbindung === "fehler"
        ? "Nicht verbunden"
        : k.verbindung === "verbindet"
          ? "Verbindet …"
          : "Nicht angemeldet";

  return (
    <section className="block" aria-labelledby="t-konto">
      <div className="block-kopf">
        <h2 id="t-konto">Plate Assistant</h2>
        <Status ton={ton}>{text}</Status>
        {k.verbindung === "verbunden" && (
          <span className="knopfreihe kopf-knoepfe">
            <button className="knopf knopf-klein" onClick={k.laden}>
              Neu laden
            </button>
            <button
              className="knopf knopf-klein"
              disabled={gesperrt}
              title={gesperrt ? "Während des Kopierens nicht möglich" : "Abmelden und das Passwort auf diesem Rechner löschen"}
              onClick={k.abmelden}
            >
              Abmelden
            </button>
          </span>
        )}
      </div>
      <p className="leise block-text">
        Projekte, Drehorte und Soll-Liste kommen aus der gemeinsamen Datenbank. Dasselbe Konto wie in der iPhone-App; das
        Passwort bleibt im Schlüsselbund dieses Rechners.
      </p>
      {k.konto && (
        <Feld name="Angemeldet als" hilfe="Abmelden löscht das Passwort auf diesem Rechner; danach meldet die App nicht mehr von selbst an.">
          <span className="zahl">{k.konto.email}</span>
          {!k.konto.ingestRecht && <span className="leise">Nur lesen und Projekte anlegen</span>}
        </Feld>
      )}
      {k.meldung && k.verbindung === "verbunden" && (
        <p className="feld-meldung">
          <Status ton="fehler">{k.meldung}</Status>
        </p>
      )}
      {k.verbindung !== "verbunden" && (
        <form
          className="anmeldung"
          onSubmit={(ev) => {
            ev.preventDefault();
            k.anmelden(passwort).then(() => setPasswort(""));
          }}
        >
          {erweitert && (
            <>
              <Feld name="Supabase-Adresse">
                <input
                  className="zahl"
                  aria-label="Supabase-Adresse"
                  value={k.zugang.adresse}
                  disabled={gesperrt}
                  spellCheck={false}
                  onChange={(ev) => k.setZugang({ ...k.zugang, adresse: ev.target.value })}
                />
              </Feld>
              <Feld name="Zugangsschlüssel (Anon-Key)">
                <input
                  className="zahl"
                  aria-label="Zugangsschlüssel (Anon-Key)"
                  value={k.zugang.anonKey}
                  disabled={gesperrt}
                  spellCheck={false}
                  onChange={(ev) => k.setZugang({ ...k.zugang, anonKey: ev.target.value })}
                />
              </Feld>
            </>
          )}
          <Feld name="E-Mail">
            <input
              type="email"
              aria-label="E-Mail"
              autoComplete="username"
              value={k.zugang.email}
              disabled={gesperrt}
              spellCheck={false}
              onChange={(ev) => k.setZugang({ ...k.zugang, email: ev.target.value })}
            />
          </Feld>
          <Feld name="Passwort" hilfe="Vergessen? In der iPhone-App zurücksetzen.">
            <input
              type="password"
              aria-label="Passwort"
              autoComplete="current-password"
              value={passwort}
              disabled={gesperrt}
              onChange={(ev) => setPasswort(ev.target.value)}
            />
          </Feld>
          {k.meldung && (
            <p className="feld-meldung">
              <Status ton="fehler">
                {k.meldung}
                {k.eigenerZugang && " Verwendet wird eine von Hand eingetragene Adresse, nicht die eingebaute."}
              </Status>
            </p>
          )}
          {k.verbindung === "fehler" && <Schluesselangaben adresse={k.zugang.adresse} schluessel={k.zugang.anonKey} eigene={k.eigenerZugang} />}
          <div className="knopfreihe">
            <button
              type="submit"
              className="knopf knopf-haupt"
              disabled={gesperrt || !k.vollstaendig || !passwort || k.verbindung === "verbindet"}
            >
              Anmelden
            </button>
            {!erweitert && (
              <button type="button" className="knopf" onClick={() => setErweitert(true)}>
                Andere Adresse …
              </button>
            )}
            {k.eigenerZugang && (
              <button
                type="button"
                className="knopf"
                onClick={() => {
                  k.eingebautVerwenden();
                  setErweitert(false);
                }}
              >
                Eingebaute Adresse verwenden
              </button>
            )}
          </div>
        </form>
      )}
    </section>
  );
}

/** Zum Vergleichen mit der iPhone-App: welche Adresse und welcher Schlüssel verwendet werden, ohne den Schlüssel
 *  ganz zu zeigen (Rolle und Ausgeber aus dem Schlüssel, dazu die letzten sechs Zeichen). */
function Schluesselangaben({ adresse, schluessel, eigene }: { adresse: string; schluessel: string; eigene: boolean }) {
  const k = schluessel.trim().replace(/^ANON_KEY=/, "").replace(/^["']|["']$/g, "");
  let inhalt: { role?: string; iss?: string; ref?: string } = {};
  try {
    const teil = k.split(".")[1].replace(/-/g, "+").replace(/_/g, "/");
    inhalt = JSON.parse(atob(teil.padEnd(teil.length + ((4 - (teil.length % 4)) % 4), "=")));
  } catch {
    // kein JWT: dann ist schon das der Fehler
  }
  return (
    <dl className="schluessel">
      <dt>Server</dt>
      <dd className="zahl">{adresse || "–"}</dd>
      <dt>Schlüssel</dt>
      <dd>
        {k ? (
          <>
            {inhalt.role ? `Rolle ${inhalt.role}` : "kein gültiger Schlüssel (kein JWT)"}
            {inhalt.iss && ` · ausgegeben von ${inhalt.iss}`}
            {inhalt.ref && ` · Projekt ${inhalt.ref}`} · endet auf <span className="zahl">…{k.slice(-6)}</span> ·{" "}
            {eigene ? "von Hand eingetragen" : "in die App eingebaut"}
          </>
        ) : (
          "fehlt"
        )}
      </dd>
      <dd className="leise schluessel-hilfe">
        Mit dem Schlüssel der iPhone-App vergleichen: Stimmen Server und letzte Zeichen nicht überein, ist der falsche
        Schlüssel eingebaut (GitHub-Variable SUPABASE_ANON_KEY) oder die falsche Adresse (SUPABASE_ADRESSE).
      </dd>
    </dl>
  );
}

/** ART CMD direkt bei ARRI laden (mitliefern verbietet die Lizenz von ARRI) und eintragen. */
function ArtCmdLaden({ gesperrt, gesetzt }: { gesperrt: boolean; gesetzt: (pfad: string) => void }) {
  const [stand, setStand] = useState<"bereit" | "laedt" | "fertig" | string>("bereit");
  return (
    <div className="art-laden">
      <p className="leise">
        Noch nicht installiert? Die App lädt das kostenlose ARRI Reference Tool CMD direkt bei ARRI (etwa 60 MB) und trägt es
        hier ein. Es gilt die Lizenz von ARRI (EULA im Paket). Danach erscheinen Tilt und Roll auch für schon eingelesene Karten.
      </p>
      <div className="knopfreihe">
        <button
          className="knopf"
          disabled={gesperrt || stand === "laedt"}
          onClick={async () => {
            setStand("laedt");
            try {
              const pfad = await invoke<string>("artcmd_laden");
              gesetzt(pfad);
              setStand("fertig");
            } catch (err) {
              setStand(String(err));
            }
          }}
        >
          {stand === "laedt" ? "Lädt bei ARRI …" : "Von ARRI laden und einrichten"}
        </button>
        {stand === "fertig" && <Status ton="ok">Eingerichtet</Status>}
        {stand !== "bereit" && stand !== "laedt" && stand !== "fertig" && <Status ton="fehler">{stand}</Status>}
      </div>
    </div>
  );
}
