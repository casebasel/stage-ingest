/** ARRI-Werkzeuge (Marlon, 14.10.2026: „wie ein Windows-User davon erfährt und ihm helfen, es zu installieren“):
 *  ART CMD (Tilt/Roll, Kamerawerte, Vorschaubilder) und der ART Viewer (MXF und ARRIRAW abspielen). Beide gratis bei
 *  ARRI, beide darf die App nicht mitliefern (EULA); sie lädt sie auf Wunsch direkt bei ARRI, am Mac und unter Windows.
 *  Hier: Stand, Laden und eine Hinweiszeile auf der Seite „Einlesen“, solange etwas fehlt. */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { gemerkt, merken, useEinstellungen } from "./einstellungen";
import { Status } from "./teile";

export const ARRI_SEITE = "https://www.arri.com/en/learn-help/learn-help-camera-system/tools/arri-reference-tool";

/** Pfad des installierten ART Viewer oder null; `neu()` fragt erneut (nach dem Installieren). */
export function useArtViewer() {
  const [pfad, setPfad] = useState<string | null | undefined>(undefined);
  const [zaehler, setZaehler] = useState(0);
  useEffect(() => {
    let aus = false;
    invoke<string | null>("art_viewer")
      .then((p) => !aus && setPfad(p))
      .catch(() => !aus && setPfad(null));
    return () => {
      aus = true;
    };
  }, [zaehler]);
  // Nach dem Installer: wieder ins Fenster zurück → neu nachsehen.
  useEffect(() => {
    const zurueck = () => setZaehler((n) => n + 1);
    window.addEventListener("focus", zurueck);
    return () => window.removeEventListener("focus", zurueck);
  }, []);
  return { pfad, neu: () => setZaehler((n) => n + 1) };
}

/** Einrichtung: ART Viewer installiert? Sonst bei ARRI laden und den Installer von ARRI starten. */
export function ArtViewerLaden({ gesperrt }: { gesperrt: boolean }) {
  const v = useArtViewer();
  const [stand, setStand] = useState<"bereit" | "laedt" | "installer" | string>("bereit");
  if (v.pfad === undefined) return null;
  return (
    <div className="art-laden">
      {v.pfad ? (
        <Status ton="ok">ART Viewer installiert</Status>
      ) : (
        <>
          <p className="leise">
            Zum Abspielen von MXF und ARRIRAW (Leertaste in der Projektseite). Die App lädt den kostenlosen ARRI Reference Tool
            Viewer bei ARRI (etwa 150 MB) und startet den Installer von ARRI; dort Lizenz bestätigen
            {navigator.userAgent.includes("Windows") ? " und Administratorrechte erlauben" : " und Passwort eingeben"}. Es gilt
            die Lizenz von ARRI.
          </p>
          <div className="knopfreihe">
            <button
              className="knopf"
              disabled={gesperrt || stand === "laedt"}
              onClick={async () => {
                setStand("laedt");
                try {
                  await invoke("art_viewer_installieren");
                  setStand("installer");
                } catch (err) {
                  setStand(String(err));
                }
              }}
            >
              {stand === "laedt" ? "Lädt bei ARRI …" : "Von ARRI laden und installieren"}
            </button>
            <button className="verweis" onClick={() => openUrl(ARRI_SEITE).catch(() => {})}>
              Seite von ARRI
            </button>
            {stand === "installer" && <Status ton="laeuft">Installer von ARRI ist offen; danach erscheint hier „installiert“</Status>}
            {stand !== "bereit" && stand !== "laedt" && stand !== "installer" && <Status ton="fehler">{stand}</Status>}
          </div>
        </>
      )}
    </div>
  );
}

/** Seite „Einlesen“: eine Zeile, solange ART CMD oder ART Viewer fehlen (ausblendbar). */
export function ArriHinweis({ zurEinrichtung }: { zurEinrichtung: () => void }) {
  const e = useEinstellungen();
  const v = useArtViewer();
  const [aus, setAus] = useState(() => gemerkt("arriHinweisAus", false));
  const fehlt = [!e.artCmd.trim() && "ART CMD", v.pfad === null && "ART Viewer"].filter(Boolean) as string[];
  if (aus || v.pfad === undefined || fehlt.length === 0) return null;
  return (
    <div className="arri-hinweis" role="status">
      <Status ton="warn">
        ARRI-Werkzeuge fehlen ({fehlt.join(", ")}): ohne sie keine Tilt- und Roll-Werte, keine Vorschaubilder
        {v.pfad === null ? ", MXF lässt sich nicht abspielen" : ""}. Beide gibt es gratis bei ARRI, die App lädt sie für dich.
      </Status>
      <button className="verweis" onClick={zurEinrichtung}>
        Einrichten
      </button>
      <button
        className="verweis leise"
        onClick={() => {
          merken("arriHinweisAus", true);
          setAus(true);
        }}
      >
        Ausblenden
      </button>
    </div>
  );
}
