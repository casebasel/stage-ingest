/** Abspielen (Marlon, 14.10.2026). MXF/ARRIRAW öffnet der ARRI Reference Tool Viewer mit dem Clip (keine
 *  Vorschau-Dateien: „generiert zu viel Daten“). MOV/MP4 (iPhone, Fotoapparat) spielt der Player in der App, wie
 *  Quick Look: Leertaste öffnet und schliesst, ← → springen zum vorigen/nächsten solchen Take, Esc schliesst. */
import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { X } from "lucide-react";
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

type Zustand = { art: "laedt" } | { art: "bereit"; url: string } | { art: "fehler"; text: string };

/** MXF/ARRIRAW: spielt nur der ART Viewer. */
export const istArri = (datei: string) => /\.(mxf|ari|arx)$/i.test(datei);

/** Das Original öffnen: MXF/ARRIRAW im ARRI Reference Tool Viewer (bei Bedarf von ARRI laden), sonst im Standard-Player. */
export async function originalOeffnen(datei: string) {
  const arri = istArri(datei);
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
      "MXF und ARRIRAW spielt nur der kostenlose ARRI Reference Tool Viewer ab.\n\nJetzt bei ARRI laden (etwa 160 MB) und den Installer von ARRI starten? Es gilt die Lizenz von ARRI.",
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

export function Abspieler({
  liste,
  index,
  setIndex,
}: {
  liste: AbspielEintrag[];
  index: number;
  setIndex: (i: number | null) => void;
}) {
  const [z, setZ] = useState<Zustand>({ art: "laedt" });
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
    setZ({ art: "laedt" });
    invoke("clip_freigeben", { datei: t.datei })
      .then(() => !aus && setZ({ art: "bereit", url: convertFileSrc(t.datei) }))
      .catch((err) => !aus && setZ({ art: "fehler", text: String(err) }));
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
            Lädt …
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
