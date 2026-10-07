// Einrichtung: alles, was man einmal pro Rechner einstellt. Am Set zeigt der Kopf nur, ob es steht.
import { useState } from "react";
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
          {lauf.soll?.fehler && <p className="feld-meldung text-warn">{lauf.soll.fehler}</p>}
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
                Freigabe schon bei einer einzigen Kopie? Fällt diese Platte aus, ist das Material weg. Nur für Tests; gilt bis
                zum Neustart.
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
            <input type="checkbox" role="switch" checked={e.mitMd5} disabled={gesperrt} onChange={(ev) => e.setMitMd5(ev.target.checked)} />
          </Feld>
          <Feld name="Karte zweimal lesen" hilfe="Erkennt einen Kartenleser, der unzuverlässig liefert. Dauert länger.">
            <input
              type="checkbox"
              role="switch"
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
  const [erweitert, setErweitert] = useState(!VORGABE_ADRESSE || !VORGABE_ANON);
  const gesperrt = lauf.laeuft;
  const ton = k.verbindung === "verbunden" ? "ok" : k.verbindung === "fehler" ? "fehler" : k.verbindung === "verbindet" ? "laeuft" : "leise";
  const text =
    k.verbindung === "verbunden"
      ? `Verbunden · ${k.projekte.length} Projekte · ${k.drehs.length} Drehorte`
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
          <button className="knopf knopf-klein" onClick={k.laden}>
            Neu laden
          </button>
        )}
      </div>
      <p className="leise block-text">
        Projekte, Drehorte und Soll-Liste kommen aus der gemeinsamen Datenbank. Dasselbe Konto wie in der iPhone-App; das
        Passwort bleibt im Schlüsselbund dieses Rechners.
      </p>
      {k.konto && (
        <Feld name="Angemeldet als">
          <span className="zahl">{k.konto.email}</span>
          {!k.konto.ingestRecht && <span className="leise">Nur lesen und Projekte anlegen</span>}
        </Feld>
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
                  value={k.zugang.adresse}
                  disabled={gesperrt}
                  spellCheck={false}
                  onChange={(ev) => k.setZugang({ ...k.zugang, adresse: ev.target.value })}
                />
              </Feld>
              <Feld name="Zugangsschlüssel (Anon-Key)">
                <input
                  className="zahl"
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
              autoComplete="current-password"
              value={passwort}
              disabled={gesperrt}
              onChange={(ev) => setPasswort(ev.target.value)}
            />
          </Feld>
          {k.meldung && <p className="feld-meldung text-fehler">{k.meldung}</p>}
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
          </div>
        </form>
      )}
    </section>
  );
}
