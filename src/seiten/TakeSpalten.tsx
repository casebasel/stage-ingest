// Spalten der Take-Tabellen (Marlon, 08.10.2026): alles, was es zu einem Take gibt, als Spalte wählbar, gemerkt pro
// Rechner. Quellen: Plate Assistant (Take, Plate, Kamerawerte von Hand/CAP), Kopie des Clips (Container und
// Metadaten, eigener Leser) und ART CMD (CSV in 05_METADATEN). Namen wie im Stage-CSV für Resolve (`export.ts`),
// damit Konsole und Ingest dasselbe sagen. Technische Werte werden erst gelesen, wenn eine solche Spalte sichtbar ist.
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Columns3 } from "lucide-react";
import { gemerkt, merken } from "../einstellungen";

export type TakeMitWerten = {
  id: string;
  nummer: number;
  art: string;
  bewertung: string;
  clip: string;
  karte: string | null;
  freigegeben: boolean;
  datei?: string | null;
  csv?: string | null;
  werte?: Record<string, unknown>;
};

export type Technik = Record<string, string>;

type Gruppe = "Take" | "Bild" | "Kamerawerte" | "Bewegung" | "Plate Assistant" | "Weitere";

export type Spalte = {
  id: string;
  titel: string;
  gruppe: Gruppe;
  quelle: string;
  rechts?: boolean;
  // Feste Breite (Clipnamen, Karten).
  mono?: boolean;
  // Braucht die Werte aus der Kopie (Container, Metadaten, ART CMD).
  technik?: boolean;
  wert: (t: TakeMitWerten, x: Technik | undefined) => ReactNode;
};

const GRUPPEN: Gruppe[] = ["Take", "Bild", "Kamerawerte", "Bewegung", "Plate Assistant", "Weitere"];

function pa(t: TakeMitWerten, feld: string): string | undefined {
  const w = t.werte?.[feld];
  if (w === undefined || w === null || w === "") return undefined;
  if (typeof w === "boolean") return w ? "Ja" : "Nein";
  if (typeof w === "number") return String(Math.round(w * 100) / 100);
  return String(w);
}

function uhrzeit(iso: string | undefined): string | undefined {
  if (!iso) return undefined;
  const d = new Date(iso);
  return isNaN(d.getTime())
    ? iso
    : d.toLocaleTimeString("de-CH", {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      });
}

// Technischer Wert aus der Kopie, sonst der Wert aus dem Plate Assistant (von Hand oder über CAP).
const tech = (id: string, titel: string, gruppe: Gruppe, quelle: string, rueckfall?: string, rechts = true): Spalte => ({
  id,
  titel,
  gruppe,
  quelle,
  rechts,
  technik: true,
  wert: (t, x) => x?.[id] ?? (rueckfall ? pa(t, rueckfall) : undefined),
});

const paSpalte = (id: string, titel: string, feld: string, rechts = false): Spalte => ({
  id,
  titel,
  gruppe: "Plate Assistant",
  quelle: `Plate Assistant (${feld})`,
  rechts,
  wert: (t) => pa(t, feld),
});

const KATALOG: Spalte[] = [
  // Bild: aus der Kopie (Container).
  tech("codec", "Codec", "Bild", "Clipdatei (Container)", undefined, false),
  tech("aufloesung", "Auflösung", "Bild", "Clipdatei (Container)"),
  tech("bildrate", "FPS", "Bild", "Clipdatei (Container)"),
  tech("bilder", "Bilder", "Bild", "Clipdatei (Container)"),
  tech("dauer", "Dauer", "Bild", "Clipdatei (Container)"),
  tech("startTc", "Start TC", "Bild", "Clipdatei (Timecode-Spur), sonst Plate Assistant", "take.start_tc"),
  tech("endTc", "End TC", "Bild", "Clipdatei (Timecode-Spur), sonst Plate Assistant", "take.end_tc"),
  tech("groesse", "Grösse", "Bild", "Clipdatei"),
  // Kamerawerte: Metadaten im Clip oder ART CMD, sonst Plate Assistant.
  tech("kamera", "Kamera", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.modell", false),
  tech(
    "seriennummer",
    "Kamera-Seriennummer",
    "Kamerawerte",
    "Clip-Metadaten / ART CMD, sonst Plate Assistant",
    "plate.kamera.seriennummer",
    false,
  ),
  tech("sensorFps", "Sensor FPS", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.sensorFps"),
  tech("shutter", "Shutter", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.shutter"),
  tech("ei", "EI", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.iso"),
  tech("weissK", "Weissabgleich K", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.weissK"),
  tech("tint", "Tint", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.tint"),
  tech("nd", "ND", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.nd", false),
  tech("look", "Look", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.look", false),
  tech("objektiv", "Objektiv", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.objektiv", false),
  tech("brennweite", "Brennweite mm", "Kamerawerte", "ART CMD, sonst Plate Assistant", "plate.kamera.brennweiteMm"),
  tech("fokus", "Fokus", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.fokus"),
  tech("blende", "Blende", "Kamerawerte", "Clip-Metadaten / ART CMD, sonst Plate Assistant", "plate.kamera.blende"),
  // Bewegung: ART CMD (Mittel und Bereich über alle Bilder), sonst der Wert, den der Plate Assistant übernommen hat.
  tech("tilt", "Tilt °", "Bewegung", "ART CMD (Mittel), sonst Plate Assistant", "take.aus_clip.tiltGrad"),
  tech("tiltBereich", "Tilt Bereich °", "Bewegung", "ART CMD", "take.aus_clip.tiltBereich"),
  tech("roll", "Roll °", "Bewegung", "ART CMD (Mittel), sonst Plate Assistant", "take.aus_clip.rollGrad"),
  tech("rollBereich", "Roll Bereich °", "Bewegung", "ART CMD", "take.aus_clip.rollBereich"),
  // Plate Assistant.
  {
    id: "startZeit",
    titel: "Aufnahme",
    gruppe: "Plate Assistant",
    quelle: "Plate Assistant (take.start_zeit)",
    rechts: true,
    wert: (t) => uhrzeit(pa(t, "take.start_zeit")),
  },
  paSpalte("neigungIphone", "Neigung iPhone °", "plate.neigung_grad", true),
  paSpalte("rollenIphone", "Rollen iPhone °", "plate.rollen_grad", true),
  paSpalte("richtung", "Richtung °", "plate.richtung.azimutGrad", true),
  paSpalte("hoehe", "Kamerahöhe cm", "plate.kamera_hoehe_cm", true),
  paSpalte("abstand", "Abstand cm", "plate.abstand_cm", true),
  paSpalte("stativ", "Stativ", "plate.stativ"),
  {
    id: "gps",
    titel: "GPS",
    gruppe: "Plate Assistant",
    quelle: "Plate Assistant (plate.gps)",
    wert: (t) => {
      const lat = pa(t, "plate.gps.lat");
      const lon = pa(t, "plate.gps.lon");
      return lat && lon ? `${lat}, ${lon}` : undefined;
    },
  },
  paSpalte("notiz", "Notiz", "plate.notiz"),
  paSpalte("vfxNotiz", "VFX-Notiz", "plate.vfx_notiz"),
];

// Felder des Plate Assistant, die eine Katalogspalte schon zeigt (erscheinen nicht noch einmal unter „Weitere“).
const IM_KATALOG = new Set([
  "take.nummer",
  "take.art",
  "take.bewertung",
  "take.clip_name",
  "take.clip.name",
  "take.start_tc",
  "take.end_tc",
  "take.start_zeit",
  "take.aus_clip.tiltGrad",
  "take.aus_clip.tiltBereich",
  "take.aus_clip.rollGrad",
  "take.aus_clip.rollBereich",
  "plate.neigung_grad",
  "plate.rollen_grad",
  "plate.richtung.azimutGrad",
  "plate.kamera_hoehe_cm",
  "plate.abstand_cm",
  "plate.stativ",
  "plate.gps.lat",
  "plate.gps.lon",
  "plate.notiz",
  "plate.vfx_notiz",
  "plate.kamera.modell",
  "plate.kamera.seriennummer",
  "plate.kamera.sensorFps",
  "plate.kamera.shutter",
  "plate.kamera.iso",
  "plate.kamera.weissK",
  "plate.kamera.tint",
  "plate.kamera.nd",
  "plate.kamera.look",
  "plate.kamera.objektiv",
  "plate.kamera.brennweiteMm",
  "plate.kamera.fokus",
  "plate.kamera.blende",
  "plate.nummer",
  "plate.name",
]);

// Spalten für Felder, die nur roh vorliegen: alles aus dem Plate Assistant und aus der Datei, was der Katalog nicht kennt.
function weitere(takes: TakeMitWerten[], technik: Record<string, Technik>): Spalte[] {
  const pa_ = new Set<string>();
  const datei = new Set<string>();
  for (const t of takes) {
    for (const k of Object.keys(t.werte ?? {})) if (!IM_KATALOG.has(k)) pa_.add(k);
    const x = t.datei ? technik[t.datei] : undefined;
    for (const k of Object.keys(x ?? {})) if (k.includes(":")) datei.add(k);
  }
  return [
    ...[...pa_].sort().map((k) => ({
      id: `pa:${k}`,
      titel: k,
      gruppe: "Weitere" as Gruppe,
      quelle: "Plate Assistant",
      wert: (t: TakeMitWerten) => pa(t, k),
    })),
    ...[...datei].sort().map((k) => ({
      id: k,
      titel: k.replace(/^datei:/, "").replace(/^artcmd:/, ""),
      gruppe: "Weitere" as Gruppe,
      quelle: k.startsWith("artcmd:") ? "ART CMD" : "Clip-Metadaten",
      technik: true,
      wert: (_: TakeMitWerten, x: Technik | undefined) => x?.[k],
    })),
  ];
}

const STANDARD = ["art", "clip", "karte", "stand"];

// Gewählte Spalten (gemerkt) und der Katalog samt Grundspalten der Tabelle.
export function useTakeSpalten(grund: Spalte[], takes: TakeMitWerten[]) {
  const [an, setAn] = useState<string[]>(() => gemerkt("takeSpalten", STANDARD));
  useEffect(() => merken("takeSpalten", an), [an]);
  const [menueOffen, setMenueOffen] = useState(false);
  // Technik laden, wenn eine sichtbare Spalte sie braucht oder das Menü offen ist (dann zeigt es auch die rohen Felder).
  const brauchtTechnik =
    menueOffen || an.some((id) => KATALOG.find((s) => s.id === id)?.technik || (id.includes(":") && !id.startsWith("pa:")));
  const technik = useTechnik(takes, brauchtTechnik);
  const alle = useMemo(() => [...grund, ...KATALOG, ...weitere(takes, technik)], [grund, takes, technik]);
  // Reihenfolge: wie im Katalog (Grundspalten zuerst), gewählt = sichtbar.
  const sichtbar = alle.filter((s) => an.includes(s.id));
  return { alle, sichtbar, an, setAn, technik, menueOffen, setMenueOffen };
}

// Pro Sitzung: einmal gelesene Werte bleiben (die Kopie ändert sich nicht).
const zwischenspeicher: Record<string, Technik> = {};

function useTechnik(takes: TakeMitWerten[], aktiv: boolean): Record<string, Technik> {
  const [stand, setStand] = useState<Record<string, Technik>>(() => ({
    ...zwischenspeicher,
  }));
  useEffect(() => {
    if (!aktiv) return;
    const fehlen = takes.filter((t) => t.datei && !(t.datei in zwischenspeicher));
    const eindeutig = [...new Map(fehlen.map((t) => [t.datei!, t])).values()];
    if (!eindeutig.length) return;
    let aus = false;
    invoke<Technik[]>("take_technik", {
      anfragen: eindeutig.map((t) => ({ datei: t.datei, csv: t.csv ?? null })),
    })
      .then((liste) => {
        eindeutig.forEach((t, i) => (zwischenspeicher[t.datei!] = liste?.[i] ?? {}));
        if (!aus) setStand({ ...zwischenspeicher });
      })
      .catch(() => {
        // Platte nicht erreichbar: Zellen bleiben leer, nächster Versuch beim nächsten Öffnen.
      });
    return () => {
      aus = true;
    };
  }, [aktiv, takes]);
  return stand;
}

export function SpaltenMenue({
  alle,
  an,
  setAn,
  offen,
  setOffen,
  festeIds,
}: {
  alle: Spalte[];
  an: string[];
  setAn: (a: string[]) => void;
  offen: boolean;
  setOffen: (o: boolean) => void;
  festeIds: string[];
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [suche, setSuche] = useState("");
  useEffect(() => {
    if (!offen) return;
    const zu = (ev: MouseEvent) => ref.current && !ref.current.contains(ev.target as Node) && setOffen(false);
    const taste = (ev: KeyboardEvent) => ev.key === "Escape" && setOffen(false);
    document.addEventListener("mousedown", zu);
    document.addEventListener("keydown", taste);
    return () => {
      document.removeEventListener("mousedown", zu);
      document.removeEventListener("keydown", taste);
    };
  }, [offen, setOffen]);
  const wahl = alle.filter((s) => !festeIds.includes(s.id));
  const q = suche.trim().toLowerCase();
  const passend = q ? wahl.filter((s) => s.titel.toLowerCase().includes(q) || s.quelle.toLowerCase().includes(q)) : wahl;
  const umschalten = (id: string) => setAn(an.includes(id) ? an.filter((x) => x !== id) : [...an, id]);
  const zahl = wahl.filter((s) => an.includes(s.id)).length;
  return (
    <div className="spalten-menue" ref={ref}>
      <button className="knopf" aria-expanded={offen} aria-haspopup="dialog" onClick={() => setOffen(!offen)}>
        <Columns3 size={14} aria-hidden /> Spalten <span className="zahl leise">{zahl}</span>
      </button>
      {offen && (
        <div className="spalten-panel" role="dialog" aria-label="Spalten wählen">
          <div className="spalten-kopf">
            <input
              type="search"
              placeholder="Spalte suchen"
              aria-label="Spalte suchen"
              value={suche}
              autoFocus
              onChange={(ev) => setSuche(ev.target.value)}
            />
            <button className="verweis" onClick={() => setAn(STANDARD)}>
              Standard
            </button>
          </div>
          <div className="spalten-liste">
            {GRUPPEN.map((g) => {
              const teil = passend.filter((s) => s.gruppe === g);
              if (!teil.length) return null;
              return (
                <fieldset key={g}>
                  <legend>{g === "Weitere" ? "Weitere Felder (roh)" : g}</legend>
                  {teil.map((s) => (
                    <label key={s.id} title={`Quelle: ${s.quelle}`}>
                      <input type="checkbox" checked={an.includes(s.id)} onChange={() => umschalten(s.id)} />
                      <span>{s.titel}</span>
                      <span className="leise spalten-quelle">{s.quelle}</span>
                    </label>
                  ))}
                </fieldset>
              );
            })}
            {!passend.length && <p className="leer-zeile">Keine Spalte passt.</p>}
          </div>
        </div>
      )}
    </div>
  );
}

// Kopfzeile und Zellen der gewählten Spalten.
export function SpaltenKopf({ spalten }: { spalten: Spalte[] }) {
  return (
    <>
      {spalten.map((s) => (
        <th key={s.id} className={s.rechts ? "rechts ohne-umbruch" : "ohne-umbruch"} title={`Quelle: ${s.quelle}`}>
          {s.titel}
        </th>
      ))}
    </>
  );
}

export function SpaltenZellen({ spalten, t, technik }: { spalten: Spalte[]; t: TakeMitWerten; technik: Record<string, Technik> }) {
  const x = t.datei ? technik[t.datei] : undefined;
  return (
    <>
      {spalten.map((s) => {
        const w = s.wert(t, x);
        return (
          <td key={s.id} className={s.rechts ? "rechts zahl" : s.technik || s.mono ? "zahl ohne-umbruch" : undefined}>
            {w ?? <span className="leise">–</span>}
          </td>
        );
      })}
    </>
  );
}
