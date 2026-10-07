// Vorschau im normalen Browser (ohne Tauri und ohne Rust-Kern): Die Aufrufe an den Kern werden mit Beispieldaten
// beantwortet, damit sich die Oberfläche in allen Zuständen ansehen lässt. Wird nur geladen, wenn kein Tauri da ist
// (main.tsx); in der App nie aktiv.
//
// Zustand über die Adresse: ?zustand=ja (Standard) | nein | fehler | sperre, ?thema=tag|nacht
import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";

const params = new URLSearchParams(location.search);
const zustand = params.get("zustand") ?? "ja";
const warte = (ms: number) => new Promise((r) => setTimeout(r, ms));
const KARTE = "/Volumes/A001R132";
const ZIELE = ["/Volumes/SAMSUNG T7/Footage", "/Volumes/NAS/Footage"];

function kurz(p: string) {
  return (
    p
      .replace(/[äÄ]/g, "AE")
      .replace(/[öÖ]/g, "OE")
      .replace(/[üÜ]/g, "UE")
      .normalize("NFD")
      .replace(/[̀-ͯ]/g, "")
      .toUpperCase()
      .replace(/[^A-Z0-9]+/g, "_")
      .replace(/^_+/, "")
      .slice(0, 24)
      .replace(/_+$/, "") || null
  );
}

const clips = Array.from({ length: 14 }, (_, i) => {
  const n = String(i + 1).padStart(3, "0");
  return `A001C${n}_261028_R132.mov`;
});

function ergebnis(ziele: string[]) {
  const sicher = zustand !== "nein";
  const jetzt = new Date().toISOString();
  return {
    kopie: {
      quelle: KARTE,
      dateien: [...clips, "A001R132.ale"].map((p, i) => ({
        pfad: p,
        groesse: p.endsWith(".ale") ? 4812 : 1_840_000_000 + i * 37_000_000,
        geaendert: jetzt,
        pruefsumme: { xxh128: (0x3c516b751c69e9c4n + BigInt(i)).toString(16).padStart(32, "0"), md5: null },
      })),
      ordner: [],
      ausgelassen: [],
      ziele: ziele.map((o) => ({ ordner: o, fehler: null })),
      beginn: jetzt,
      ende: jetzt,
    },
    urteile: ziele.map((o, i) => ({
      ordner: o,
      geprueft: 15,
      abweichungen:
        !sicher && i === 1 ? [{ art: "pruefsumme", pfad: "A001C007_261028_R132.mov", soll: "3c51…", ist: "9f02…" }] : [],
      kopierfehler: null,
    })),
    kennungen: ziele.map((_, i) =>
      i === 0
        ? { wert: "platte:disk4", sicher: true, art: "platte", seriennummer: "S6XNNF0W123456", beschreibung: "Samsung PSSD T7" }
        : { wert: "netz:10.0.0.5", sicher: true, art: "netz", seriennummer: null, beschreibung: "Netzlaufwerk //nas/Footage" },
    ),
    mhl: ziele.map((o, i) => (sicher || i === 0 ? `${o}/ascmhl/0001_A001R132_2026-10-28_094500Z.mhl` : null)),
    clips: [],
    ale: ziele.map(() => null),
    bewegung: [],
    plates: null,
    stage: null,
    abgleich: {
      gefunden: clips.slice(0, 11).map((c, i) => [{ clip: c, szene: "42A", take: String(i + 1), startTc: "", endTc: "", bewertung: "", quelle: "plate" }, c]),
      fehlt: sicher
        ? []
        : [{ clip: "A001C015_261028_R132", szene: "42A", take: "15", startTc: "", endTc: "", bewertung: "Favorit", quelle: "plate" }],
      ueberTimecode: [],
      ueberZeitfenster: clips.slice(11, 13).map((c, i) => [{ clip: "", szene: "42B", take: String(i + 1), startTc: "", endTc: "", bewertung: "", quelle: "plate" }, c]),
      mehrdeutig: [],
      unerwartet: [clips[13]],
    },
    berichte: ziele.map((o) => ({ Ok: `${o}/../04_BERICHTE/A001R132_Bericht_2026-10-28_094500Z.pdf` })),
    freigabe: {
      sicher,
      unabhaengige_kopien: sicher ? 2 : 1,
      mindest_kopien: 2,
      kennung_unsicher: false,
      grund: sicher ? "2 unabhängige Kopien geprüft" : "nur 1 von 2 Kopien geprüft, 1 Ziel(e) fehlerhaft",
      hinweise: ["Netzlaufwerk über das Netz zurückgelesen; den Zwischenspeicher des NAS kann keine App umgehen."],
    },
  };
}

async function kopierenNachspielen(ziele: string[]) {
  const gesamt = 27_400_000_000;
  await emit("ingest://fortschritt", { phase: "kopieren", meldung: { art: "begonnen", dateien: 15, bytes: gesamt } });
  for (let i = 1; i <= 20; i++) {
    await warte(180);
    await emit("ingest://fortschritt", { phase: "kopieren", meldung: { art: "datei", nummer: i % 15, pfad: clips[i % 14] } });
    await emit("ingest://fortschritt", { phase: "kopieren", meldung: { art: "bytes", gelesen: (gesamt * i) / 20 } });
  }
  for (let z = 0; z < ziele.length; z++)
    for (let i = 0; i < 6; i++) {
      await warte(150);
      await emit("ingest://fortschritt", { phase: "pruefen", ziel: z, pfad: clips[i * 2] });
    }
}

const projekt = { id: "projekt-happy_end", name: "Happy End", kurzname: "HAPPY_END", aktiv: true };

const antworten: Record<string, (a: Record<string, unknown>) => unknown> = {
  "plugin:app|version": () => "0.1.6 · Vorschau",
  "plugin:updater|check": () => null,
  "plugin:opener|open_path": () => null,
  "plugin:dialog|open": (a) => {
    const titel = String((a.options as { title?: string })?.title ?? "");
    if (titel.startsWith("Karte")) return KARTE;
    if (titel.startsWith("ART")) return "/Applications/ARRI/art-cmd";
    return ZIELE[0];
  },
  kartenziele: (a) => {
    const quelle = String(a.quelle);
    const karte = quelle.split("/").filter(Boolean).pop() ?? "Karte";
    const d = a.dreh as { projekt: string; kurzname?: string | null; datum: string; name: string } | null;
    return (a.basis as string[]).map((b) =>
      d ? `${b}/${d.kurzname || kurz(d.projekt) || "OHNE_PROJEKT"}/${d.datum}_${d.name}/01_KAMERA/${karte}` : `${b}/${karte}`,
    );
  },
  vorab_pruefen: (a) => {
    const auftrag = a.auftrag as { ziele: string[] };
    if (zustand === "sperre")
      return [{ stufe: "fehler", text: `Zu wenig Platz auf /Volumes/SAMSUNG T7: 12,4 GB frei, 27,4 GB nötig` }];
    return auftrag.ziele.length === 1
      ? [{ stufe: "warnung", text: "Gewählt ist ein Ordner, nicht die ganze Karte: er wird gesichert, die Karte aber nicht zum Formatieren freigegeben" }]
      : [];
  },
  karte_einlesen: async (a) => {
    const ziele = (a.auftrag as { ziele: string[] }).ziele;
    await kopierenNachspielen(ziele);
    if (zustand === "fehler") throw `Ziel existiert schon: ${ziele[0]}`;
    return ergebnis(ziele);
  },
  abbrechen: () => null,
  laeuft: () => false,
  verlauf: () => [
    { beginn: "2026-10-28T09:45:00Z", ende: "", karte: "A001R131", quelle: "", dateien: 22, bytes: 41_200_000_000, sicher: true, grund: "2 unabhängige Kopien geprüft", ziele: [{ ordner: "", gut: true, bericht: "/x.pdf" }] },
    { beginn: "2026-10-27T16:10:00Z", ende: "", karte: "A001R130", quelle: "", dateien: 9, bytes: 12_800_000_000, sicher: false, grund: "nur 1 von 2 Kopien", ziele: [{ ordner: "", gut: true, bericht: null }] },
  ],
  soll_von_stage: () => [],
  stage_projekt: () => null,
  plate_anmelden: () => ({ email: "marlon@ca-se.ch", ingestRecht: false }),
  plate_projekte: () => [projekt, { id: "projekt-moevenpick", name: "Mövenpick Spot", kurzname: "MOEVENPICK", aktiv: false }],
  plate_drehs: () => [
    { id: "d1", name: "Rheinufer", datum: "2026-10-28", projektId: projekt.id, produktion: "" },
    { id: "d2", name: "Münsterplatz", datum: "2026-10-28", projektId: projekt.id, produktion: "" },
  ],
  plate_soll: () => [],
  plate_projekt_anlegen: (a) => `projekt-${String(a.kurzname).toLowerCase()}`,
  kurzname_vorschlag: (a) => kurz(String(a.name)),
  ziel_nachpruefen: async () => {
    await warte(800);
    return { ordner: "/Volumes/NAS/Footage/HAPPY_END/2026-10-27_Rheinufer/01_KAMERA/A001R130", generation: "0001_A001R130_2026-10-27_161000Z.mhl", geprueft: 10, abweichungen: [] };
  },
  projekt_uebersicht: () => ({
    projekt,
    hinweis: null,
    unlesbar: [],
    ohneTake: ["A001C014_261028_R132 (A001R132)"],
    karten: [
      { drehOrdner: "2026-10-28_Rheinufer", datei: "a", inhalt: { karte: "A001R131", beginn: "", freigegeben: true, unabhaengigeKopien: 2, clips: Array(22) } },
      { drehOrdner: "2026-10-28_Rheinufer", datei: "b", inhalt: { karte: "A001R132", beginn: "", freigegeben: false, unabhaengigeKopien: 1, clips: Array(14) } },
    ],
    drehs: [
      {
        id: "d1", name: "Rheinufer", datum: "2026-10-28", hdri: ["uploaded"],
        plates: [
          { slate: "42A", name: "Ufer Süd", fotos: 6, hdri: [], takes: [1, 2, 3].map((n) => ({ id: `t${n}`, nummer: n, art: "take", bewertung: n === 2 ? "circle" : "", clip: `A001C00${n}_261028_R131`, karte: "A001R131", freigegeben: true })) },
          { slate: "42B", name: "Brücke", fotos: 3, hdri: ["processed"], takes: [
            { id: "t4", nummer: 1, art: "take", bewertung: "gut", clip: "A001C011_261028_R132", karte: "A001R132", freigegeben: false },
            { id: "t5", nummer: 2, art: "graukugel", bewertung: "", clip: "A001C012_261028_R132", karte: "A001R132", freigegeben: false },
            { id: "t6", nummer: 3, art: "take", bewertung: "", clip: "", karte: null, freigegeben: false },
          ] },
        ],
      },
    ],
  }),
};

export function einrichten() {
  mockIPC(
    async (cmd, payload) => {
      const f = antworten[cmd];
      if (!f) {
        console.warn("Vorschau: keine Antwort für", cmd, payload);
        return null;
      }
      return f((payload ?? {}) as Record<string, unknown>);
    },
    { shouldMockEvents: true },
  );
  // Vorschau-Daten: zwei Ziele, Stage-Adresse leer; das Thema per ?thema=tag|nacht erzwingbar.
  try {
    if (!localStorage.getItem("ingest.ziele")) localStorage.setItem("ingest.ziele", JSON.stringify(ZIELE));
    const t = params.get("thema");
    if (t === "tag" || t === "nacht") localStorage.setItem("ingest.thema", t);
  } catch {
    // ohne Speicher: dann eben ohne Vorbelegung
  }
  const marke = document.createElement("div");
  marke.textContent = "Vorschau mit Beispieldaten · nichts wird kopiert";
  marke.setAttribute("role", "note");
  Object.assign(marke.style, {
    position: "fixed",
    bottom: "10px",
    right: "12px",
    zIndex: "50",
    padding: "4px 10px",
    font: "500 12px/1.4 var(--schrift)",
    color: "var(--amber-kontrast)",
    background: "var(--amber)",
    borderRadius: "999px",
    pointerEvents: "none",
  });
  document.addEventListener("DOMContentLoaded", () => document.body.appendChild(marke));
  if (document.body) document.body.appendChild(marke);
}
