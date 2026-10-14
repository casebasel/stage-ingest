/** Player in der App (Marlon, 14.10.2026: „Player in der App“, wie Quick Look und Silverstack). Leertaste öffnet
 *  den gewählten Take und schliesst wieder, ← → springen zum vorigen/nächsten Take mit Kopie, Esc schliesst.
 *  MXF/ARRIRAW spielt eine Rec.709-Vorschau aus ART CMD (src-tauri/src/vorschau_video.rs), MOV/MP4 die Datei selbst.
 *  Das Original öffnet auf Wunsch der ARRI Reference Tool Viewer. */
import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { X } from "lucide-react";
import { useEinstellungen } from "../einstellungen";
import { Status } from "../teile";

export type AbspielEintrag = {
  id: string;
  datei: string;
  /** z. B. „42A · Take 3“ */
  titel: string;
  clip: string;
  bewertung?: string;
};

const BEWERTUNG: Record<string, string> = { circle: "Favorit", gut: "Gut", schlecht: "Schlecht" };

type Zustand = { art: "laedt"; neu: boolean } | { art: "bereit"; url: string } | { art: "fehler"; text: string };

/** Das Original öffnen: MXF/ARRIRAW im ARRI Reference Tool Viewer (bei Bedarf von ARRI laden), sonst im Standard-Player. */
export async function originalOeffnen(datei: string) {
  const klein = datei.toLowerCase();
  const arri = klein.endsWith(".mxf") || klein.endsWith(".ari") || klein.endsWith(".arx");
  try {
    if (!arri) {
      await openPath(datei);
      return;
    }
    if (await invoke<string | null>("art_viewer")) {
      await invoke("im_art_viewer", { datei });
      return;
    }
    const ja = await ask(
      "MXF und ARRIRAW spielt QuickTime nicht ab. Der kostenlose ARRI Reference Tool Viewer kann es.\n\nJetzt bei ARRI laden (etwa 160 MB) und den Installer von ARRI starten? Es gilt die Lizenz von ARRI.",
      { title: "Clip abspielen", kind: "info", okLabel: "Laden und installieren", cancelLabel: "Abbrechen" },
    );
    if (!ja) return;
    await invoke("art_viewer_installieren");
    await message("Der Installer von ARRI ist offen. Nach der Installation den Clip noch einmal abspielen.", {
      title: "ARRI Reference Tool Viewer",
      kind: "info",
    });
  } catch (e) {
    await message(`Der Clip liess sich nicht öffnen.\n\n${datei}\n\n${String(e)}`, { title: "Clip abspielen", kind: "error" }).catch(
      () => {},
    );
  }
}

async function video(datei: string, artCmd: string, nurVorhanden: boolean): Promise<string | null> {
  return invoke<string | null>("clip_video", { datei, artCmd: artCmd.trim() || null, nurVorhanden });
}

export function Abspieler({
  liste,
  index,
  setIndex,
}: {
  liste: AbspielEintrag[];
  index: number;
  setIndex: (i: number | null) => void;
}) {
  const e = useEinstellungen();
  const [z, setZ] = useState<Zustand>({ art: "laedt", neu: false });
  const ref = useRef<HTMLVideoElement>(null);
  const t = liste[index];

  useEffect(() => {
    const taste = (ev: KeyboardEvent) => {
      if (ev.key === "Escape" || ev.key === " ") {
        ev.preventDefault();
        setIndex(null);
      } else if (ev.key === "ArrowRight" || ev.key === "ArrowDown") {
        ev.preventDefault();
        if (index < liste.length - 1) setIndex(index + 1);
      } else if (ev.key === "ArrowLeft" || ev.key === "ArrowUp") {
        ev.preventDefault();
        if (index > 0) setIndex(index - 1);
      } else if (ev.key.toLowerCase() === "k" && ref.current) {
        ref.current.paused ? ref.current.play() : ref.current.pause();
      }
    };
    window.addEventListener("keydown", taste);
    return () => window.removeEventListener("keydown", taste);
  }, [index, liste.length]);

  useEffect(() => {
    let aus = false;
    setZ({ art: "laedt", neu: false });
    (async () => {
      try {
        // Erst nachsehen, ob die Vorschau schon da ist (dann sofort); sonst rechnen lassen.
        let p = await video(t.datei, e.artCmd, true);
        if (!p) {
          if (aus) return;
          setZ({ art: "laedt", neu: true });
          p = await video(t.datei, e.artCmd, false);
        }
        if (!aus && p) setZ({ art: "bereit", url: convertFileSrc(p) });
        // Den nächsten Take schon vorbereiten, damit → sofort spielt.
        const naechster = liste[index + 1];
        if (naechster) video(naechster.datei, e.artCmd, false).catch(() => {});
      } catch (err) {
        if (!aus) setZ({ art: "fehler", text: String(err) });
      }
    })();
    return () => {
      aus = true;
    };
  }, [t.id, t.datei]);

  return (
    <div className="grossbild abspieler" role="dialog" aria-modal="true" aria-label="Take abspielen" onClick={() => setIndex(null)}>
      <button className="knopf-symbol grossbild-zu" aria-label="Schliessen (Leertaste oder Esc)" onClick={() => setIndex(null)}>
        <X size={18} />
      </button>
      <div className="abspieler-bild" onClick={(ev) => ev.stopPropagation()}>
        {z.art === "bereit" ? (
          <video ref={ref} key={z.url} src={z.url} controls autoPlay playsInline />
        ) : z.art === "laedt" ? (
          <div className="vorschau-platz" aria-busy="true">
            {z.neu ? "Vorschau wird gerechnet (ARRI ART CMD) …" : "Lädt …"}
          </div>
        ) : (
          <div className="abspieler-fehler">
            <Status ton="fehler">{z.text}</Status>
          </div>
        )}
      </div>
      <div className="grossbild-text abspieler-text" onClick={(ev) => ev.stopPropagation()}>
        <span>
          <b>{t.titel}</b> <span className="zahl">{t.clip}</span>
          {t.bewertung && BEWERTUNG[t.bewertung] && <> · {BEWERTUNG[t.bewertung]}</>}
        </span>
        <button
          className="verweis"
          onClick={() => {
            ref.current?.pause();
            originalOeffnen(t.datei);
          }}
        >
          Original öffnen
        </button>
      </div>
      <span className="grossbild-zahl zahl">
        {index + 1} / {liste.length} · ← → Take · Leertaste schliesst · K Pause
      </span>
    </div>
  );
}
