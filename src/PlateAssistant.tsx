// Plate Assistant: Projekt und Drehort aus der gemeinsamen Supabase wählen (Vertrag plate-assistant docs/ABGLEICH.md).
// Anmeldung mit dem persönlichen Konto wie im iPhone. E-Mail bleibt auf diesem Rechner, das Passwort im Schlüsselbund.
// Löschen (HDRI-Rohdaten) und Meldungen nur mit dem Kennzeichen „ingest“ am Konto (setzt Marlon).
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type Zugang = { adresse: string; anonKey: string; email: string };
export type Projekt = { id: string; name: string; kurzname: string; aktiv: boolean };
export type DrehKurz = { id: string; name: string; datum: string; projektId: string | null; produktion: string };

const KURZNAME = /^[A-Z0-9]+(_[A-Z0-9]+)*$/;

function gemerkt<T>(k: string, s: T): T {
  try {
    const w = localStorage.getItem(`ingest.plate.${k}`);
    return w === null ? s : (JSON.parse(w) as T);
  } catch {
    return s;
  }
}
function merken(k: string, w: unknown) {
  try {
    localStorage.setItem(`ingest.plate.${k}`, JSON.stringify(w));
  } catch {
    // ohne Speicher: nur für diese Sitzung
  }
}

// Beim Bauen eingesetzt (GitHub-Variablen SUPABASE_ADRESSE, SUPABASE_ANON_KEY), nie im Repo. Fehlen sie,
// fragt die App danach.
const VORGABE_ADRESSE = (import.meta.env.VITE_SUPABASE_ADRESSE as string | undefined) ?? "";
const VORGABE_ANON = (import.meta.env.VITE_SUPABASE_ANON_KEY as string | undefined) ?? "";

export function PlateAssistant(p: {
  gesperrt: boolean;
  projekt: Projekt | null;
  dreh: DrehKurz | null;
  onProjekt: (p: Projekt | null) => void;
  onDreh: (d: DrehKurz | null) => void;
}) {
  const [zugang, setZugang] = useState<Zugang>(() => {
    const z = gemerkt("zugang", { adresse: "", anonKey: "", email: "" });
    return { adresse: z.adresse || VORGABE_ADRESSE, anonKey: z.anonKey || VORGABE_ANON, email: z.email };
  });
  const [erweitert, setErweitert] = useState(!VORGABE_ADRESSE || !VORGABE_ANON);
  const [passwort, setPasswort] = useState("");
  const [zustand, setZustand] = useState<{ ok: boolean; text: string } | null>(null);
  const [konto, setKonto] = useState<{ email: string; ingestRecht: boolean } | null>(null);
  const [projekte, setProjekte] = useState<Projekt[]>([]);
  const [drehs, setDrehs] = useState<DrehKurz[]>([]);
  const [neu, setNeu] = useState<{ name: string; kurzname: string; fehler: string | null } | null>(null);
  const bereit = !!(zugang.adresse.trim() && zugang.anonKey.trim() && zugang.email.trim());

  async function laden() {
    if (!bereit) return;
    try {
      const [pr, dr] = await Promise.all([
        invoke<Projekt[]>("plate_projekte", { zugang }),
        invoke<DrehKurz[]>("plate_drehs", { zugang }),
      ]);
      setProjekte(pr);
      setDrehs(dr);
      setZustand({ ok: true, text: `${dr.length} Drehorte · ${pr.length} Projekte` });
    } catch (e) {
      setZustand({ ok: false, text: String(e) });
    }
  }

  useEffect(() => {
    // Beim Start mit dem gemerkten Passwort aus dem Schlüsselbund anmelden (still, wenn keins da ist).
    if (bereit)
      invoke<{ email: string; ingestRecht: boolean }>("plate_anmelden", { zugang, passwort: "" })
        .then((k) => {
          setKonto(k);
          laden();
        })
        .catch(() => {});
  }, []);

  async function anmelden() {
    merken("zugang", zugang);
    try {
      setKonto(await invoke("plate_anmelden", { zugang, passwort }));
      setPasswort("");
      await laden();
    } catch (e) {
      setZustand({ ok: false, text: String(e) });
    }
  }

  async function vorschlag(name: string) {
    const k = (await invoke<string | null>("kurzname_vorschlag", { name })) ?? "";
    setNeu((n) => (n ? { ...n, name, kurzname: k } : n));
  }

  async function anlegen() {
    if (!neu) return;
    if (!neu.name.trim() || !KURZNAME.test(neu.kurzname) || neu.kurzname.length < 2 || neu.kurzname.length > 24) {
      setNeu({ ...neu, fehler: "Kurzname: 2–24 Zeichen, nur A–Z, 0–9 und _ (fest nach dem Anlegen)" });
      return;
    }
    try {
      const id = await invoke<string>("plate_projekt_anlegen", { zugang, name: neu.name.trim(), kurzname: neu.kurzname });
      await laden();
      p.onProjekt({ id, name: neu.name.trim(), kurzname: neu.kurzname, aktiv: true });
      setNeu(null);
    } catch (e) {
      setNeu({ ...neu, fehler: String(e) });
    }
  }

  // Drehorte des gewählten Projekts; alte Drehorte haben nur den Projektnamen als Text.
  const passende = p.projekt
    ? drehs.filter((d) => d.projektId === p.projekt!.id || (!d.projektId && d.produktion === p.projekt!.name))
    : drehs;

  return (
    <section className="k-gruppe">
      <h2>Plate Assistant</h2>
      {!zustand?.ok && (
        <>
          {erweitert && (
            <>
          <input
            className="i-eingabe mono"
            placeholder="Supabase-Adresse"
            value={zugang.adresse}
            disabled={p.gesperrt}
            onChange={(e) => setZugang({ ...zugang, adresse: e.target.value })}
            spellCheck={false}
          />
          <input
            className="i-eingabe mono"
            placeholder="Anon-Key"
            value={zugang.anonKey}
            disabled={p.gesperrt}
            onChange={(e) => setZugang({ ...zugang, anonKey: e.target.value })}
            spellCheck={false}
          />
            </>
          )}
          <input
            className="i-eingabe"
            placeholder="E-Mail"
            value={zugang.email}
            disabled={p.gesperrt}
            onChange={(e) => setZugang({ ...zugang, email: e.target.value })}
            spellCheck={false}
          />
          <input
            className="i-eingabe"
            type="password"
            placeholder="Passwort"
            value={passwort}
            disabled={p.gesperrt}
            onChange={(e) => setPasswort(e.target.value)}
          />
          <div className="i-knopfreihe">
            <button className="k-taste k-taste-klein" disabled={p.gesperrt || !bereit || !passwort} onClick={anmelden}>
              Anmelden
            </button>
            {!erweitert && (
              <button className="k-taste k-taste-klein k-taste-leise" onClick={() => setErweitert(true)}>
                Andere Adresse …
              </button>
            )}
          </div>
          <span className="k-leise k-klein">Dasselbe Konto wie im Plate Assistant. Passwort vergessen: in der iPhone-App zurücksetzen.</span>
        </>
      )}
      {konto && (
        <span className="k-leise k-klein">
          Angemeldet als <span className="mono">{konto.email}</span>
          {!konto.ingestRecht && " · Löschen nicht freigegeben (nur lesen und Projekte anlegen)"}
        </span>
      )}
      {zustand && (
        <span className={`k-lampe ${zustand.ok ? "k-lampe-ok" : "k-lampe-warn"} k-klein`}>
          <i /> {zustand.text}
        </span>
      )}
      {zustand?.ok && (
        <>
          <select
            className="i-eingabe"
            value={p.projekt?.id ?? ""}
            disabled={p.gesperrt}
            onChange={(e) => {
              const pr = projekte.find((x) => x.id === e.target.value) ?? null;
              p.onProjekt(pr);
              p.onDreh(null);
            }}
          >
            <option value="">Projekt wählen …</option>
            {projekte
              .filter((x) => x.aktiv || x.id === p.projekt?.id)
              .map((x) => (
                <option key={x.id} value={x.id}>
                  {x.name} ({x.kurzname})
                </option>
              ))}
          </select>
          <select
            className="i-eingabe"
            value={p.dreh?.id ?? ""}
            disabled={p.gesperrt}
            onChange={(e) => p.onDreh(passende.find((d) => d.id === e.target.value) ?? null)}
          >
            <option value="">Drehort wählen (Soll-Liste) …</option>
            {passende.map((d) => (
              <option key={d.id} value={d.id}>
                {d.datum} · {d.name}
              </option>
            ))}
          </select>
          <div className="i-knopfreihe">
            <button className="k-taste k-taste-klein k-taste-leise" disabled={p.gesperrt} onClick={laden}>
              Neu laden
            </button>
            {!neu && (
              <button
                className="k-taste k-taste-klein k-taste-leise"
                disabled={p.gesperrt}
                onClick={() => setNeu({ name: "", kurzname: "", fehler: null })}
              >
                Neues Projekt …
              </button>
            )}
          </div>
          {neu && (
            <>
              <input
                className="i-eingabe"
                placeholder="Name des Projekts, z. B. Happy End"
                value={neu.name}
                onChange={(e) => {
                  setNeu({ ...neu, name: e.target.value, fehler: null });
                  vorschlag(e.target.value);
                }}
              />
              <input
                className="i-eingabe mono"
                placeholder="KURZNAME"
                value={neu.kurzname}
                onChange={(e) => setNeu({ ...neu, kurzname: e.target.value.toUpperCase(), fehler: null })}
              />
              <span className="k-leise k-klein">Der Kurzname ist der Ordnername und nach dem Anlegen fest.</span>
              {neu.fehler && <span className="k-warn k-klein">{neu.fehler}</span>}
              <div className="i-knopfreihe">
                <button className="k-taste k-taste-klein k-taste-amber" onClick={anlegen}>
                  Anlegen
                </button>
                <button className="k-taste k-taste-klein k-taste-leise" onClick={() => setNeu(null)}>
                  Abbrechen
                </button>
              </div>
            </>
          )}
        </>
      )}
    </section>
  );
}

export const plateSoll = (zugang: Zugang, drehId: string) =>
  invoke<import("./kern").SollClip[]>("plate_soll", { zugang, drehId });
export const gemerkterZugang = (): Zugang => gemerkt("zugang", { adresse: "", anonKey: "", email: "" });
