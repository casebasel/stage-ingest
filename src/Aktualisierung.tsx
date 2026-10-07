// Updates wie Syncomat: Banner oben. Pflicht nur, wenn latest.json eine mindestVersion über der eigenen Version nennt
// (geänderter Bericht, Struktur oder Schnittstelle). Ein laufender Kopiervorgang wird nie unterbrochen:
// während er läuft, ist „Installieren“ gesperrt.
import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";

// Alle 30 Minuten und immer, wenn das Fenster wieder in den Vordergrund kommt (die App läuft oft den ganzen Drehtag).
const PRUEFEN_ALLE_MS = 30 * 60 * 1000;

/** Vergleicht „1.2.3“-Versionen; negativ, wenn a älter ist. */
export function versionVergleich(a: string, b: string) {
  const x = a.split(".").map(Number);
  const y = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0);
  return 0;
}

export function useAktualisierung() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [pflicht, setPflicht] = useState(false);
  const [version, setVersion] = useState("");

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
    let aus = false;
    const pruefen = async () => {
      try {
        const u = await check();
        if (aus || !u) return;
        const eigene = await getVersion();
        const mindest = (u.rawJson as { mindestVersion?: string }).mindestVersion;
        setPflicht(!!mindest && versionVergleich(eigene, mindest) < 0);
        setUpdate(u);
      } catch {
        // offline oder kein Release: still bleiben, beim nächsten Mal wieder
      }
    };
    pruefen();
    const t = setInterval(pruefen, PRUEFEN_ALLE_MS);
    let zuletzt = Date.now();
    const imVordergrund = () => {
      // Nicht bei jedem Fensterwechsel ins Netz: höchstens alle 2 Minuten.
      if (Date.now() - zuletzt > 2 * 60 * 1000) {
        zuletzt = Date.now();
        pruefen();
      }
    };
    window.addEventListener("focus", imVordergrund);
    return () => {
      aus = true;
      clearInterval(t);
      window.removeEventListener("focus", imVordergrund);
    };
  }, []);

  return { update, pflicht, version };
}

export function Aktualisierung({ update, pflicht, laeuft }: { update: Update | null; pflicht: boolean; laeuft: boolean }) {
  const [installiert, setInstalliert] = useState(false);
  const [fehler, setFehler] = useState<string | null>(null);
  if (!update) return null;

  async function installieren() {
    if (!update || laeuft) return;
    setInstalliert(true);
    try {
      await update.downloadAndInstall();
      await relaunch();
    } catch (e) {
      setFehler(String(e));
      setInstalliert(false);
    }
  }

  return (
    <div className="i-hinweis i-update">
      <span>
        {pflicht ? "Pflicht-Update" : "Update verfügbar"}: <span className="mono">{update.version}</span>
        {pflicht && " · Bericht, Ordnerstruktur oder Schnittstelle haben sich geändert. Neue Karten erst nach dem Update."}
        {laeuft && " · Installieren nach dem Kopieren."}
        {fehler && <span className="k-warn"> · {fehler}</span>}
      </span>
      <button className="k-taste k-taste-klein" disabled={laeuft || installiert} onClick={installieren}>
        {installiert ? "Wird installiert …" : "Installieren und neu starten"}
      </button>
    </div>
  );
}
