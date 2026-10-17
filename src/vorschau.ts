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
        groesse: p.endsWith(".ale") ? 4812 : Math.round((27_400_000_000 - 4812) / 14),
        geaendert: jetzt,
        pruefsumme: { xxh128: (0x3c516b751c69e9c4n + BigInt(i)).toString(16).padStart(32, "0"), md5: null },
      })),
      ordner: [],
      ausgelassen: [],
      ziele: ziele.map((o) => ({ ordner: o, fehler: null })),
      beginn: new Date(Date.now() - 214_000).toISOString(),
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
    datenbank: null,
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
      grund: sicher ? "2 unabhängige Kopien geprüft" : "nur 1 von 2 Kopien geprüft, 1 Ziel fehlerhaft",
      hinweise: ["Netzlaufwerk über das Netz zurückgelesen; den Zwischenspeicher des NAS kann keine App umgehen."],
    },
  };
}

async function kopierenNachspielen(ziele: string[]) {
  const gesamt = 27_400_000_000;
  await emit("ingest://fortschritt", { phase: "kopieren", meldung: { art: "begonnen", dateien: 15, bytes: gesamt } });
  const alle = [...clips, "A001R132.ale"];
  for (let i = 1; i <= alle.length; i++) {
    await warte(240);
    const pfad = alle[i - 1];
    const groesse = pfad.endsWith(".ale") ? 4812 : Math.round((27_400_000_000 - 4812) / 14);
    await emit("ingest://fortschritt", { phase: "kopieren", meldung: { art: "datei", nummer: i - 1, pfad, groesse } });
    await emit("ingest://fortschritt", { phase: "kopieren", meldung: { art: "bytes", gelesen: (gesamt * i) / alle.length } });
  }
  for (let z = 0; z < ziele.length; z++)
    for (let i = 0; i < 6; i++) {
      await warte(150);
      await emit("ingest://fortschritt", { phase: "pruefen", ziel: z, pfad: clips[i * 2] });
    }
}

let karteSteckt = true;
const laufwerke = () => [
  ...(karteSteckt
    ? [{ pfad: KARTE, name: "A001R132", gesamt: 256_000_000_000, frei: 228_600_000_000, netz: false, karte: { kamera: "ARRI", clips: 14, bytes: 27_400_000_000 } }]
    : []),
  { pfad: "/Volumes/SAMSUNG T7", name: "SAMSUNG T7", gesamt: 2_000_000_000_000, frei: zustand === "sperre" ? 12_400_000_000 : 1_214_000_000_000, netz: false, karte: null },
  { pfad: "/Volumes/NAS", name: "NAS", gesamt: 48_000_000_000_000, frei: 21_700_000_000_000, netz: true, karte: null },
];

const projekt = { id: "projekt-happy_end", name: "Happy End", kurzname: "HAPPY_END", aktiv: true, fps: 25, codec: "ProRes 422 HQ", aufloesungPx: "3840x2160", art: "werbung", firma: "Beispiel Film AG", regie: "", dop: "" };

const antworten: Record<string, (a: Record<string, unknown>) => unknown> = {
  "plugin:app|version": () => "Vorschau mit Beispieldaten, nichts wird kopiert",
  "plugin:updater|check": () => null,
  "plugin:opener|open_path": () => null,
  // Rückfragen in der Vorschau bestätigen (es geschieht nichts Echtes): `ask` fragt über `message` und vergleicht mit
  // der Beschriftung des OK-Knopfs.
  "plugin:dialog|message": (a) => {
    const b = a.buttons as { OkCancelCustom?: [string, string] } | string | undefined;
    if (b && typeof b === "object" && b.OkCancelCustom) return b.OkCancelCustom[0];
    return b === "YesNo" ? "Yes" : "Ok";
  },
  "plugin:dialog|open": (a) => {
    const titel = String((a.options as { title?: string })?.title ?? "");
    if (titel.startsWith("Karte")) return KARTE;
    if (titel.startsWith("ART")) return "/Applications/ARRI/art-cmd";
    return ZIELE[0];
  },
  kartenziele: (a) => {
    const quelle = String(a.quelle);
    const karte = quelle.split("/").filter(Boolean).pop() ?? "Karte";
    const d = a.dreh as { projekt: string; kurzname?: string | null; datum: string; name: string; ortKurzname?: string | null } | null;
    return (a.basis as string[]).map((b) =>
      d ? `${b}/${d.kurzname || kurz(d.projekt) || "OHNE_PROJEKT"}/${d.datum}_${d.ortKurzname || d.name}/01_KAMERA/${karte}` : `${b}/${karte}`,
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
  laufwerke,
  auswerfen: async () => {
    await warte(600);
    karteSteckt = false;
    return null;
  },
  // Ziel 2 (NAS) als Beispiel eines zu tief gewählten Ordners (?ablage=falsch).
  ablage_pruefen: (a) =>
    params.get("ablage") === "falsch" && String(a.pfad).includes("NAS")
      ? { art: "darin", vorschlag: "/Volumes/NAS", ablage: { id: "7k3f", name: "NAS · 7K3F", erstellt: "" } }
      : { art: "richtig", ablage: { id: "a1", name: `${String(a.pfad).split("/")[2]} · A1B2`, erstellt: "" } },
  ablage_sicherstellen: () => ({ id: "a1", name: "x", erstellt: "" }),
  ziel_geraete: (a) =>
    (a.basis as string[]).map((pfad) =>
      pfad.includes("NAS")
        ? { pfad, kennung: { wert: "netz:10.0.0.5", sicher: true, art: "netz", seriennummer: null, beschreibung: "Netzlaufwerk //nas/Footage" }, gesamt: 48e12, frei: 21.7e12, fehler: null }
        : { pfad, kennung: { wert: "platte:disk4", sicher: true, art: "platte", seriennummer: "S6XNNF0W123456", beschreibung: "Samsung PSSD T7" }, gesamt: 2e12, frei: zustand === "sperre" ? 12.4e9 : 1.214e12, fehler: null },
    ),
  laeuft: () => false,
  verlauf: () => [
    { beginn: "2026-10-28T09:45:00Z", ende: "", karte: "A001R131", quelle: "", dateien: 22, bytes: 41_200_000_000, sicher: true, grund: "2 unabhängige Kopien geprüft", ziele: [{ ordner: "", gut: true, bericht: "/x.pdf" }] },
    { beginn: "2026-10-27T16:10:00Z", ende: "", karte: "A001R130", quelle: "", dateien: 9, bytes: 12_800_000_000, sicher: false, grund: "nur 1 von 2 Kopien", ziele: [{ ordner: "", gut: true, bericht: null }] },
  ],
  protokoll_liste: () => {
    const gen = (zeit: string, rechner: string, prozess: string, dateien: number) => ({ nr: 1, datei: "0001.mhl", zeit, rechner, werkzeug: "Stage Ingest", version: "0.1.55", prozess, dateien });
    const kopie = (ordner: string, ablage: string | null, erreichbar: boolean, dateien: number, zeit: string) => ({
      ordner, ablage, erreichbar, gut: true, bericht: erreichbar ? "/x.pdf" : null, dateien: erreichbar ? dateien : null,
      bytes: erreichbar ? dateien * 3.2e9 : null, zuletzt: erreichbar ? gen(zeit, "WIN-EDIT", "transfer", dateien) : null,
    });
    return [
      { karte: "A001R132", projekt: "HAPPY_END", drehOrdner: "2026-10-28_RHEINUFER", beginn: "2026-10-28T13:55:00Z", dateien: 56, bytes: 64.3e9, sicher: true, grund: "2 unabhängige Kopien geprüft", unabhaengigeKopien: 2,
        kopien: [kopie("/Volumes/NAS/Footage/HAPPY_END/2026-10-28_RHEINUFER/01_KAMERA/A001R132", "NAS Footage", true, 56, "2026-10-28T13:55:00Z"),
                 kopie("/Volumes/SAMSUNG T7/Footage/HAPPY_END/2026-10-28_RHEINUFER/01_KAMERA/A001R132", "Samsung T7", true, 56, "2026-10-28T13:55:00Z")] },
      { karte: "A001R131", projekt: "HAPPY_END", drehOrdner: "2026-10-28_RHEINUFER", beginn: "2026-10-28T09:45:00Z", dateien: 56, bytes: 41.2e9, sicher: true, grund: "2 unabhängige Kopien geprüft", unabhaengigeKopien: 2,
        kopien: [kopie("/Volumes/NAS/Footage/HAPPY_END/2026-10-28_RHEINUFER/01_KAMERA/A001R131", "NAS Footage", true, 56, "2026-10-28T09:45:00Z"),
                 kopie("E:\\Footage\\HAPPY_END\\2026-10-28_RHEINUFER\\01_KAMERA\\A001R131", null, false, 56, "2026-10-28T09:45:00Z")] },
      { karte: "A001R130", projekt: "HAPPY_END", drehOrdner: "2026-10-27_MUENSTER", beginn: "2026-10-27T16:10:00Z", dateien: 56, bytes: 12.8e9, sicher: false, grund: "nur 1 von 2 unabhängigen Kopien", unabhaengigeKopien: 1,
        kopien: [kopie("/Volumes/NAS/Footage/HAPPY_END/2026-10-27_MUENSTER/01_KAMERA/A001R130", "NAS Footage", true, 56, "2026-10-27T16:10:00Z")] },
    ];
  },
  protokoll_karte: (a) => {
    const ordner = a.ordner as string[];
    const karte = ordner[0].split(/[\\/]/).pop() ?? "A001R132";
    const reel = karte;
    const n = karte.slice(1, 4);
    const r = karte.slice(4);
    const zeit = (min: number) => new Date(Date.UTC(2026, 9, 28, 8, 2) + min * 60_000).toISOString();
    const pfade: [string, number, string][] = [
      [`${reel}/${reel}_AVID.ale`, 18_400, zeit(190)],
      [`${reel}/${reel}.xml`, 2_310, zeit(190)],
      ...[1, 2, 3, 4].map((c): [string, number, string] => [`${reel}/A${n}C00${c}_261028_${r}.mxf`, 3.1e9 + c * 4.2e8, zeit(c * 23)]),
      ...Array.from({ length: 50 }, (_, b): [string, number, string] => [
        `${reel}/A${n}C005_261028_${r}/A${n}C005_261028_${r}.${String(b).padStart(7, "0")}.ari`,
        1.12e7,
        zeit(120),
      ]),
    ];
    // A001R131: C002 von Hand gelöscht, die Historie des NAS nennt ihn noch (wie A008R56E am 16.10.2026).
    const weg = karte === "A001R131" ? `A${n}C002_261028_${r}` : null;
    const loesch = weg ? { clip: weg, zeit: "2026-10-28T19:40:12+01:00", benutzer: "marlon auf MAC-STUDIO" } : null;
    const da = (o: string) => o.startsWith("/");
    const generationen = [
      { nr: 1, datei: "0001.mhl", zeit: "2026-10-28T13:55:00Z", rechner: "WIN-EDIT", werkzeug: "Stage Ingest", version: "0.1.55", prozess: "transfer", dateien: pfade.length },
      { nr: 2, datei: "0002.mhl", zeit: "2026-10-29T08:12:00Z", rechner: "MAC-STUDIO", werkzeug: "Stage Ingest", version: "0.1.55", prozess: "in-place", dateien: pfade.length },
    ];
    const vorgang = (zeit: string, art: string, rechner: string, kopie: number, mehr: object = {}) => ({
      zeit, art, rechner, benutzer: "", werkzeug: "Stage Ingest 0.1.55", dateien: pfade.length, clip: null, historieNeu: false, kopie, ...mehr,
    });
    return {
      kopien: ordner.map((o, i) => ({
        ordner: o,
        erreichbar: da(o),
        generationen: da(o) ? generationen : [],
        fehler: null,
        nenntGeloeschte: weg && i === 0 ? [`${reel}/${weg}.mxf`] : [],
        ausserhalb: [],
      })),
      dateien: pfade.map(([pfad, groesse, geaendert], i) => {
        const geloescht = weg !== null && pfad.includes(weg);
        return {
          pfad,
          groesse,
          geaendert,
          xxh128: "9f2c4e1a7b3d5f6e8a0c2e4f6a8c0e2f",
          geloescht: geloescht ? loesch : null,
          jeKopie: ordner.map((o) =>
            !da(o) ? "unbekannt" : geloescht ? "geloescht" : karte === "A001R130" && i === 3 ? "nichtMehrDa" : karte === "A001R130" && i === 5 ? "ungeprueft" : "geprueft",
          ),
        };
      }),
      verlauf: [
        ...ordner.flatMap((o, i) => (da(o) && i === 0 ? [vorgang("2026-10-29T08:12:00Z", "nachgeprueft", "MAC-STUDIO", i)] : [])),
        ...(loesch ? [vorgang(loesch.zeit, "geloescht", "MAC-STUDIO", 0, { benutzer: "marlon", dateien: null, clip: weg })] : []),
        ...ordner.flatMap((o, i) => (da(o) ? [vorgang("2026-10-28T13:55:00Z", "eingelesen", "WIN-EDIT", i)] : [])),
      ],
    };
  },
  historie_bereinigen: async () => {
    await warte(600);
    return { geloeschte: ["A001R131/A001C002_261028_R131.mxf"], ausserhalb: [], ablage: "/x/frueher", generation: "/x/ascmhl/0001.mhl", protokoll: "/x/04_BERICHTE/x.txt" };
  },
  soll_von_stage: () => [],
  stage_projekt: () => null,
  plate_anmelden: () => {
    if (params.get("konto") === "fehler")
      throw "Der Server lehnt den Zugangsschlüssel (Anon-Key) ab. E-Mail und Passwort wurden noch gar nicht geprüft.";
    return { email: "team@beispiel.invalid", ingestRecht: false };
  },
  plate_projekte: () => [projekt, { id: "projekt-moevenpick", name: "Mövenpick Spot", kurzname: "MOEVENPICK", aktiv: false }],
  plate_drehs: () => [
    { id: "d1", name: "Rheinufer", datum: "2026-10-28", projektId: projekt.id, produktion: "", kurzname: "RHEINUFER" },
    { id: "d2", name: "Münsterplatz", datum: "2026-10-28", projektId: projekt.id, produktion: "" },
  ],
  plate_soll: () => [],
  studio_soll: () => [],
  plate_projekt_aendern: () => null,
  clip_zuordnen: () => null,
  take_loeschen: async () => {
    await warte(400);
    return null;
  },
  stimmt_so: async () => {
    await warte(400);
    return null;
  },
  ohne_aufnahme_bestaetigen: async () => {
    await warte(400);
    return null;
  },
  clip_loeschen: () => ({ kopien: 1, dateien: 2, bytes: 1, protokolle: [] }),
  plate_drehort_kurznamen: () => ["RHEINUFER", "BRUECKE"],
  plate_drehort_anlegen: (a) => `dreh-happy_end-${String(a.kurzname).toLowerCase()}`,
  plate_projekt_anlegen: (a) => `projekt-${String(a.kurzname).toLowerCase()}`,
  kurzname_vorschlag: (a) => (kurz(String(a.name)) ?? "").slice(0, Number(a.laenge ?? 24)).replace(/_+$/, ""),
  ziel_nachpruefen: async () => {
    await warte(800);
    return { ordner: "/Volumes/NAS/Footage/HAPPY_END/2026-10-27_Rheinufer/01_KAMERA/A001R130", generation: "0001_A001R130_2026-10-27_161000Z.mhl", geprueft: 10, abweichungen: [] };
  },
  // Vorschaubild: Farbfläche mit Beschriftung (in der App kommt hier ein verkleinertes JPEG).
  bild_vorschau: (a) => {
    const pfad = String(a.pfad);
    const hdri = a.bucket === "hdri";
    const farbe = (pfad.length * 47) % 360;
    const [w, h] = hdri ? [800, 400] : [800, 600];
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="hsl(${hdri ? 205 : farbe} 45% 62%)"/><stop offset="1" stop-color="hsl(${hdri ? 30 : farbe} 30% 28%)"/></linearGradient></defs><rect width="100%" height="100%" fill="url(#g)"/><text x="50%" y="52%" font-family="sans-serif" font-size="28" fill="white" text-anchor="middle">${pfad}</text></svg>`;
    return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
  },
  // ?stand=gemischt: erstes Ziel schon vollständig kopiert, zweites abweichend (zum Ansehen der Konfliktanzeige).
  ziele_stand: (a) =>
    (a.ziele as string[]).map((_, i) =>
      params.get("stand") === "gemischt"
        ? i === 0
          ? { art: "vorhanden", dateien: 5 }
          : { art: "abweichend", grund: "3 von 5 Dateien fehlen" }
        : { art: "neu" },
    ),
  kopie_aus_kopie: () => ({
    quelle: "/Volumes/SAMSUNG T7/Footage/HAPPY_END/2026-10-28_RHEINUFER/01_KAMERA/A001R132",
    ziel: "/Volumes/NAS/Footage/HAPPY_END/2026-10-28_RHEINUFER/01_KAMERA/A001R132",
    urteile: [
      { ordner: "/Volumes/SAMSUNG T7/…/A001R132", geprueft: 16, abweichungen: [], kopierfehler: null },
      { ordner: "/Volumes/NAS/…/A001R132", geprueft: 16, abweichungen: [], kopierfehler: null },
    ],
    kennungen: [
      { art: "platte", sicher: true, wert: "a", beschreibung: "Samsung PSSD T7", seriennummer: "S6XNNF0W123456" },
      { art: "netz", sicher: true, wert: "b", beschreibung: "Netzlaufwerk //nas/Footage", seriennummer: null },
    ],
    freigabe: {
      sicher: true, unabhaengige_kopien: 2, mindest_kopien: 2, kennung_unsicher: false,
      grund: "2 unabhängige Kopien geprüft",
      hinweise: ["Kopie aus Kopie: aus /Volumes/SAMSUNG T7/…/A001R132 erstellt, geprüft gegen die ursprünglichen Prüfsummen der Karte (ASC MHL)."],
    },
    bericht: "/Volumes/NAS/…/04_BERICHTE/A001R132_Bericht.pdf",
  }),
  einlesen_vorschau: () => ({
    drehorte: [{ id: "d1", name: "Rheinufer", datum: "2026-10-28", kurzname: "RHEINUFER", clips: ["A001C001_261028_R132", "A001C002_261028_R132"] }],
    ohne: ["A001C014_261028_R132"],
    gesamt: 3,
    aufnahmetag: "2026-10-28",
    uhrFalsch: false,
    ordner: "RHEINUFER",
  }),
  art_viewer: () => null,
  artcmd_laden: () => "/Users/beispiel/Library/Application Support/ch.filmstudiobasel.stage-ingest/art-cmd/art-cmd_1.0.0_macos_universal/bin/art-cmd",
  karte_wiedererkennen: () => ({ art: "nichtFormatiert", beginn: "2026-10-28T10:12:00+01:00", sicher: true, bekannt: 12, neue: 3 }),
  clip_freigeben: () => null,
  take_aus_clip: async () => {
    await warte(800);
    return "01NEU";
  },
  export_zielordner: () => "/Volumes/NAS/Footage/HAPPY_END/2026-10-28_RHEINUFER/06_EXPORT/ProRes_HQ_709Look",
  clip_exportieren: async () => {
    await warte(2500);
    return { pfad: "/Volumes/NAS/Footage/HAPPY_END/2026-10-28_RHEINUFER/06_EXPORT/ProRes_HQ_709Look/A001C002_261028_R131_709Look_HQ.mov", hinweis: null };
  },
  clip_mittelbild: async (a) => {
    await warte(150);
    const n = String(a.datei).length;
    const farbe = ["#3a4250", "#6b5a3e", "#2f3d33", "#4a3550", "#35504a"][n % 5];
    return `data:image/svg+xml;utf8,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="480" height="270"><rect width="480" height="270" fill="${farbe}"/><circle cx="${140 + (n % 7) * 30}" cy="135" r="60" fill="#c8b48a"/></svg>`)}`;
  },
  clip_vorschaubilder: () =>
    ["#3a4250", "#6b5a3e", "#2f3d33"].map(
      (farbe) =>
        `data:image/svg+xml;utf8,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="480" height="270"><rect width="480" height="270" fill="${farbe}"/><circle cx="240" cy="135" r="60" fill="#c8b48a"/></svg>`)}`,
    ),
  take_technik: (p) =>
    ((p.anfragen as { datei: string }[]) ?? []).map((_, i) => ({
      codec: "ProRes 4444 XQ", aufloesung: "3840x2160", bildrate: "25", bilder: String(1250 + i * 75), dauer: `0:${50 + i * 3}.0`,
      startTc: `14:0${i + 1}:10:00`, endTc: `14:0${i + 1}:${60 + i * 3 - 60 < 10 ? "0" : ""}${(60 + i * 3) % 60}:00`, groesse: `${(14.2 + i).toFixed(1)} GB`,
      kamera: "ALEXA Mini", seriennummer: "K1.0024581", ei: "800", weissK: "5600", tint: "0", shutter: "172.8", nd: "ND 0.6",
      tilt: (-2.1 + i * 0.4).toFixed(1), tiltBereich: "0.3", roll: "0.4", rollBereich: "0.2", brennweite: "35",
      "datei:com.arri.camera.ExposureIndexAsa": "800", "datei:com.arri.camera.LookName": "ARRI 709",
      "artcmd:lensState/lensIris": "2.8", "artcmd:lensState/lensFocusDistance": "4200",
    })),
  projekt_uebersicht: () => ({
    projekt,
    hinweis: null,
    unlesbar: [],
    zuKlaeren: [
      { clip: "A001C014_261028_R132", karte: "A001R132", startTc: "14:22:05:10", drehOrdner: "2026-10-28_RHEINUFER", freigegeben: false, dateien: ["/a", "/b"], clipdateien: ["/Volumes/NAS/x.mxf"], vorschlagPlate: "p1", vorschlagDreh: "d1" },
      { clip: "A001C015_120101_R132", karte: "A001R132", startTc: "14:31:40:02", drehOrdner: "2026-10-28_RHEINUFER", freigegeben: false, dateien: ["/a", "/b"], clipdateien: ["/Volumes/NAS/x.mxf"], vorschlagDreh: "d1", vorschlagTake: "t2", vorschlagGrund: "Take 2 · Clipname von der Kamera" },
      { clip: "A002C001_120101_R9QX", karte: "A002R9QX", startTc: "15:02:11:00", drehOrdner: "2026-10-28_RHEINUFER", freigegeben: false, dateien: ["/c"], clipdateien: ["/Volumes/NAS/y.mxf"], hinweis: "Take-ID PA:01JAB… im ersten Clip der Karte: der Slate kann von der vorigen Karte stammen" },
    ],
    unstimmigkeiten: [
      { schluessel: "w1", clip: "A001C011_261028_R132", karte: "A001R132", takeId: "t4", vorschlagTake: "t5", vorschlagDreh: "d1", text: "Zugeordnet zu On Loc 2 · Brücke · Take 1 (von Hand). Die Take-ID im Clip passt aber zu On Loc 2 · Brücke · Take 2.", dateien: ["/a", "/b"], clipdateien: ["/Volumes/NAS/x.mxf"] },
      { schluessel: "w2", clip: "A001C012_261028_R132", karte: "A001R132", takeId: "t5", text: "Zugeordnet zu On Loc 2 · Brücke · Take 2 (von Hand). Der Clip beginnt aber nicht zwischen dessen Klappe und der nächsten.", dateien: ["/a", "/b"], clipdateien: ["/Volumes/NAS/x.mxf"] },
    ],
    ohneAufnahme: [
      { takeId: "t6", nummer: 3, plateId: "p2", drehId: "d1", clip: "A001C013_261028_R132", karte: "A001R132", sicher: true, grund: "Zwischen Take 2 (A001C012) und Take 6 (A001C013) liegt kein Clip für diesen Take", dateien: ["/a", "/b"] },
      { takeId: "t7", nummer: 4, plateId: "p2", drehId: "d1", clip: "A001C014_261028_R132", karte: "A001R132", sicher: false, grund: "Ein Clip ohne Take kann zu diesem Take gehören: unter „Clips ohne Take“ nachsehen", dateien: ["/a", "/b"] },
      { takeId: "t8", nummer: 5, plateId: "p2", drehId: "d1", clip: "A001C015_261028_R132", karte: "A001R132", sicher: true, grund: "Zwischen Take 2 (A001C012) und Take 6 (A001C013) liegt kein Clip für diesen Take", dateien: params.get("karten") === "weg" ? [] : ["/a", "/b"] },
    ],
    karten: [
      { drehOrdner: "2026-10-28_Rheinufer", datei: "a", inhalt: { karte: "A001R131", beginn: "", freigegeben: true, unabhaengigeKopien: 2, clips: Array(22) } },
      { drehOrdner: "2026-10-28_Rheinufer", datei: "b", inhalt: { karte: "A001R132", beginn: "", freigegeben: false, unabhaengigeKopien: 1, clips: Array(14) } },
    ],
    drehs: [
      {
        id: "d1", name: "Rheinufer", kurzname: "RHEINUFER", datum: "2026-10-28", karten: ["A001R131", "A001R132"],
        hdri: [{ id: "h0", zustand: "uploaded", erstelltAm: "2026-10-28T08:12:00Z", job: "wartet", vorschau: "h0/vorschau.jpg", vorschauQuelle: "iphone" }],
        plates: [
          { id: "p1", nummer: 1, slate: "42A", name: "Ufer Süd", art: "plate", szene: "42", hdri: [],
            fotos: [
              ...["referenz", "set", "position"].map((art, i) => ({ id: `f${i}`, art, pfad: `p1/f${i}.jpg` })),
              ...[1, 2].map((n) => ({ id: `v${n}`, art: "vorschau", pfad: `p1/v${n}.jpg`, takeId: `t${n}` })),
            ],
            takes: [1, 2, 3].map((n) => ({
              id: `t${n}`, nummer: n, art: "take", bewertung: n === 2 ? "circle" : "", clip: `A001C00${n}_261028_R131`, karte: "A001R131", freigegeben: true,
              datei: `/Volumes/NAS/A001R131/A001C00${n}_261028_R131.mov`, dateien: [`/Volumes/NAS/Footage/HAPPY_END/2026-10-28_RHEINUFER/01_KAMERA/A001R131/A001C00${n}_261028_R131.mov`, `/Volumes/SAMSUNG T7/Footage/HAPPY_END/2026-10-28_RHEINUFER/01_KAMERA/A001R131/A001C00${n}_261028_R131.mov`], csv: `/Volumes/NAS/05_METADATEN/A001C00${n}_261028_R131.csv`,
              werte: { "take.start_tc": `14:0${n}:10:00`, "take.start_zeit": `2026-10-28T12:0${n}:10Z`, "plate.kamera_hoehe_cm": 142, "plate.abstand_cm": 800, "plate.stativ": true, "plate.richtung.azimutGrad": 212.4, "plate.gps.lat": 47.5596, "plate.gps.lon": 7.5886, "plate.kamera.objektiv": "Signature Prime 35", "plate.notiz": "Gegenlicht" },
            })) },
          { id: "p2", nummer: 2, slate: "42B", name: "Brücke", art: "location", szene: "42",
            fotos: [0, 1, 2].map((i) => ({ id: `g${i}`, art: "set", pfad: `p2/g${i}.jpg` })),
            hdri: [{ id: "h1", zustand: "uploaded", erstelltAm: "2026-10-28T10:40:00Z", job: "processed", vorschau: "h1/ergebnis.jpg", vorschauQuelle: "dienst" }],
            takes: [
              { id: "t4", nummer: 1, art: "take", bewertung: "gut", clip: "A001C011_261028_R132", karte: "A001R132", freigegeben: false, unstimmig: true },
              { id: "t5", nummer: 2, art: "graukugel", bewertung: "", clip: "A001C012_261028_R132", karte: "A001R132", freigegeben: false, unstimmig: true },
              { id: "t6", nummer: 3, art: "take", bewertung: "", clip: "A001C013_261028_R132", karte: null, freigegeben: false, ohneAufnahme: true },
              { id: "t7", nummer: 4, art: "take", bewertung: "", clip: "A001C014_261028_R132", karte: null, freigegeben: false, clipOffen: true },
              { id: "t8", nummer: 5, art: "take", bewertung: "", clip: "A001C015_261028_R132", karte: null, freigegeben: false, ohneAufnahme: true },
            ] },
          { id: "p3", nummer: 3, slate: "", name: "Fähre", art: "plate", szene: "", fotos: [], hdri: [], takes: [] },
        ],
      },
      { id: "d2", name: "Gasstrasse", kurzname: "GASSTR", datum: "", karten: [], hdri: [], plates: [] },
      {
        id: "d3", name: "Studio", kurzname: "STUDIO", datum: "", karten: [], hdri: [],
        plates: [
          { id: "e1", nummer: 1, slate: "STUDIO-01", name: "Fenster Küche", art: "studio", szene: "", fotos: [], hdri: [],
            takes: [1, 2].map((n) => ({ id: `s${n}`, nummer: n, art: "take", bewertung: n === 1 ? "gut" : "", clip: `A007C00${n}_261010_R11A`, karte: null, freigegeben: false, studio: true, werte: { "take.notiz": n === 1 ? "Licht zu warm" : "" } })) },
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
    if (params.get("konto") !== "nein" && !localStorage.getItem("ingest.plate.zugang"))
      localStorage.setItem(
        "ingest.plate.zugang",
        JSON.stringify({
          adresse: "https://beispiel.invalid",
          // Beispiel-JWT ohne Wert (Rolle anon), nur damit die Vorschau die Schlüsselangaben zeigen kann.
          anonKey: "eyJhbGciOiJIUzI1NiJ9.eyJyb2xlIjoiYW5vbiIsImlzcyI6InN1cGFiYXNlIn0.beispiel-ohne-wert",
          email: "team@beispiel.invalid",
          eigene: true,
        }),
      );
    const t = params.get("thema");
    if (t === "tag" || t === "nacht") localStorage.setItem("ingest.thema", t);
  } catch {
    // ohne Speicher: dann eben ohne Vorbelegung
  }
}
