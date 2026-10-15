/** Abspielen (Marlon, 14./15.10.2026), wie Quick Look: Leertaste öffnet und schliesst, ← → springen zum vorigen/nächsten
 *  Take, Esc schliesst. MXF/ARRIRAW entwickelt die App selbst, Bild für Bild direkt aus dem Original (keine
 *  Vorschau-Dateien: „generiert zu viel Daten“; der ART Viewer lädt keinen Clip von aussen). MOV/MP4 spielt das
 *  Video-Element. Was die App nicht lesen kann (z. B. ALEXA 35 mit HDE), öffnet der ART Viewer. */
import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { Pause, Play, X } from "lucide-react";
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
        {istArri(t.datei) ? (
          <ArriSpieler key={t.datei} datei={t.datei} />
        ) : z.art === "bereit" ? (
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
          {istArri(t.datei) ? "Im ART Viewer öffnen" : "Original öffnen"}
        </button>
      </div>
      <span className="grossbild-zahl zahl">
        {liste.length > 1 && `${index + 1} / ${liste.length} · ← → Take · `}Leertaste schliesst · K Pause
        {istArri(t.datei) && " · , . Bild"}
      </span>
    </div>
  );
}

type ArriMeta = { id: number; breite: number; hoehe: number; rate: number; startTc: number | null; kamera: string; anzahl: number };

/** Timecode aus Bildern seit Mitternacht. */
function tc(bilder: number, rate: number): string {
  const r = Math.max(1, Math.round(rate));
  const zwei = (n: number) => String(n).padStart(2, "0");
  return `${zwei(Math.floor(bilder / (r * 3600)) % 24)}:${zwei(Math.floor(bilder / (r * 60)) % 60)}:${zwei(Math.floor(bilder / r) % 60)}:${zwei(bilder % r)}`;
}

async function arriBild(m: ArriMeta, n: number): Promise<ImageBitmap> {
  const r = await fetch(convertFileSrc(`${m.id}-${n}`, "arriraw"));
  if (!r.ok) throw new Error(await r.text());
  const w = Number(r.headers.get("X-Breite"));
  const h = Number(r.headers.get("X-Hoehe"));
  const daten = new Uint8ClampedArray(await r.arrayBuffer());
  return createImageBitmap(new ImageData(daten, w, h));
}

/** ARRIRAW abspielen: holt die Bilder nach der Uhr (höchstens vier gleichzeitig, ein paar voraus). Kommt das Netz nicht
 *  mit, werden Bilder übersprungen, die Zeit stimmt; angehalten zeigt es jedes Bild genau (, . Bild für Bild). */
function ArriSpieler({ datei }: { datei: string }) {
  const leinwand = useRef<HTMLCanvasElement>(null);
  const [meta, setMeta] = useState<ArriMeta | null>(null);
  const [fehler, setFehler] = useState<string | null>(null);
  const [laeuft, setLaeuft] = useState(true);
  const [bild, setBild] = useState(0);
  const [rateIst, setRateIst] = useState<number | null>(null);
  const z = useRef({
    cache: new Map<number, ImageBitmap>(),
    unterwegs: new Set<number>(),
    laeuft: true,
    start: 0,
    startBild: 0,
    ziel: 0,
    gezeigt: -1,
    zeiten: [] as number[],
    fehler: 0,
  });

  useEffect(() => {
    let aus = false;
    invoke<ArriMeta>("arriraw_oeffnen", { datei })
      .then((m) => !aus && setMeta(m))
      .catch((e) => !aus && setFehler(String(e)));
    return () => {
      aus = true;
    };
  }, [datei]);

  const springen = (n: number, weiter: boolean) => {
    const s = z.current;
    if (!meta) return;
    const ziel = Math.max(0, Math.min(meta.anzahl - 1, n));
    s.laeuft = weiter && ziel < meta.anzahl - 1;
    s.startBild = ziel;
    s.ziel = ziel;
    s.start = 0;
    s.gezeigt = -1;
    s.zeiten = [];
    setLaeuft(s.laeuft);
    setBild(ziel);
  };

  useEffect(() => {
    if (!meta) return;
    const s = z.current;
    let raf = 0;
    const anfordern = (n: number) => {
      s.unterwegs.add(n);
      arriBild(meta, n)
        .then((b) => s.cache.set(n, b))
        .catch((e) => {
          s.fehler += 1;
          if (s.fehler > 3) setFehler(String(e));
        })
        .finally(() => s.unterwegs.delete(n));
    };
    const schritt = () => {
      const jetzt = performance.now();
      if (s.laeuft) {
        if (s.start === 0 && s.cache.has(s.startBild)) s.start = jetzt;
        s.ziel = s.start === 0 ? s.startBild : Math.min(meta.anzahl - 1, s.startBild + Math.floor(((jetzt - s.start) / 1000) * meta.rate));
      }
      for (let k = s.ziel; k < Math.min(meta.anzahl, s.ziel + (s.laeuft ? 6 : 1)) && s.unterwegs.size < 4; k++) {
        if (!s.cache.has(k) && !s.unterwegs.has(k)) anfordern(k);
      }
      let zeigen = -1;
      if (s.laeuft) {
        for (const k of s.cache.keys()) if (k <= s.ziel && k > s.gezeigt && k > zeigen) zeigen = k;
      } else if (s.cache.has(s.ziel) && s.gezeigt !== s.ziel) zeigen = s.ziel;
      const c = leinwand.current;
      if (zeigen >= 0 && c) {
        const b = s.cache.get(zeigen)!;
        if (c.width !== b.width) c.width = b.width;
        if (c.height !== b.height) c.height = b.height;
        c.getContext("2d")?.drawImage(b, 0, 0);
        s.gezeigt = zeigen;
        setBild(zeigen);
        if (s.laeuft) {
          s.zeiten = [...s.zeiten.slice(-24), jetzt];
          if (s.zeiten.length > 12) setRateIst(((s.zeiten.length - 1) * 1000) / (jetzt - s.zeiten[0]));
        }
      }
      const behalten = s.laeuft ? s.gezeigt : s.ziel - 2;
      for (const [k, b] of s.cache) {
        if (k < behalten || k > s.ziel + 8) {
          b.close();
          s.cache.delete(k);
        }
      }
      if (s.laeuft && s.gezeigt === meta.anzahl - 1) {
        s.laeuft = false;
        s.ziel = s.gezeigt;
        setLaeuft(false);
      }
      raf = requestAnimationFrame(schritt);
    };
    raf = requestAnimationFrame(schritt);
    return () => {
      cancelAnimationFrame(raf);
      for (const b of s.cache.values()) b.close();
      s.cache.clear();
    };
  }, [meta]);

  useEffect(() => {
    const taste = (ev: KeyboardEvent) => {
      const s = z.current;
      if (!meta) return;
      if (ev.key.toLowerCase() === "k") {
        ev.preventDefault();
        if (s.laeuft) springen(s.gezeigt < 0 ? s.ziel : s.gezeigt, false);
        else springen(s.ziel >= meta.anzahl - 1 ? 0 : s.ziel, true);
      } else if (ev.key === "," || ev.key === ".") {
        ev.preventDefault();
        springen((s.laeuft ? s.gezeigt : s.ziel) + (ev.key === "." ? 1 : -1), false);
      }
    };
    window.addEventListener("keydown", taste);
    return () => window.removeEventListener("keydown", taste);
  }, [meta]);

  if (fehler)
    return (
      <div className="abspieler-fehler">
        <Status ton="fehler">{fehler}</Status>
        <p className="leise">Diesen Clip spielt die App nicht selbst ab. „Im ART Viewer öffnen“ geht immer.</p>
      </div>
    );
  if (!meta)
    return (
      <div className="vorschau-platz" aria-busy="true">
        Öffnet …
      </div>
    );
  const umschalten = () => {
    const s = z.current;
    if (s.laeuft) springen(s.gezeigt < 0 ? s.ziel : s.gezeigt, false);
    else springen(s.ziel >= meta.anzahl - 1 ? 0 : s.ziel, true);
  };
  const langsam = laeuft && rateIst !== null && rateIst < meta.rate * 0.85;
  return (
    <div className="arri-spieler">
      <canvas ref={leinwand} width={meta.breite / 2} height={meta.hoehe / 2} onClick={umschalten} />
      <div className="arri-leiste">
        <button className="knopf-symbol" aria-label={laeuft ? "Anhalten (K)" : "Abspielen (K)"} onClick={umschalten}>
          {laeuft ? <Pause size={16} /> : <Play size={16} />}
        </button>
        <input
          type="range"
          aria-label="Position im Clip"
          min={0}
          max={meta.anzahl - 1}
          value={bild}
          onChange={(e) => springen(Number(e.target.value), false)}
        />
        <span className="zahl">{meta.startTc !== null ? tc(meta.startTc + bild, meta.rate) : `${bild + 1} / ${meta.anzahl}`}</span>
        <span className={langsam ? "zahl arri-langsam" : "zahl leise"}>
          {langsam ? `${Math.round(rateIst!)} von ${Math.round(meta.rate)} B/s, Netz zu langsam` : `${meta.rate} B/s`}
        </span>
      </div>
    </div>
  );
}
