# HDRI-Scanner – Recherche, Konzept & Briefing

Stand: 7. Oktober 2026 · Filmstudio Basel · Ergänzung zur Plate-App (Swift iOS, Supabase)

> **Hinweis (Stage Ingest, 07.10.2026):** Originalbriefing, unverändert übernommen. Seither entschieden und vorrangig: Die Verarbeitung läuft nicht auf einer RTX 4090, sondern im **HDRI-Dienst von Stage Ingest auf Ada (RTX 6000 Ada, 48 GB)**, nur wenn nDisplay nicht läuft. „Setup“ heisst in allen Apps **Plate**; ein HDRI hängt an einer Plate oder an einem Drehort. Massgeblich ist `docs/KONZEPT.md`.

---

## 1. Ziel

Die bestehende iOS-App zur Koordination von Videoplates und Verwaltung der Aufnahmen am Set bekommt eine Funktion, mit der man direkt mit dem iPhone ein VFX-taugliches HDRI aufnimmt. Das Ergebnis ist ein lineares 32-Bit-EXR in Equirectangular-Projektion (2:1) für Unreal Engine (HDRI Backdrop, Image Based Lighting) und Nuke. Jedes HDRI wird einem Setup zugeordnet und später automatisch mit den Plates des Drehtags verknüpft.

## 2. Kernprinzip: Messen vor Generieren

Das HDRI ist in erster Linie eine Messung. AI ergänzt nur das, was das iPhone physikalisch nicht erfassen kann: geclippte Lichtquellen und Sonne, Stativ und Operator am Nadir, fehlende Frames. Jede ergänzte Stelle bleibt im EXR über einen eigenen Maskenkanal als solche erkennbar, und die rein gemessene Version wird immer mit abgelegt. So bleibt für Lighting, Compositing und A/B-Tests jederzeit klar, was gemessen und was rekonstruiert ist.

## 3. Gesamtpipeline

```
AM SET (iPhone, Plate-App)
  Setup aktiv → Guided Capture mit Belichtungsreihen (RAW) + Gyro/Kompass-Metadaten
  → kleine LDR-Vorschau auf dem Gerät (nur Abdeckungskontrolle)
        │
        ├─ LTE: Metadaten + Vorschau sofort via Supabase; RAW optional im Hintergrund (resumable)
        └─ Studio: voller Ingest übers lokale Netz
        ▼
STUDIO-SERVER (GPU-Worker, RTX 4090)
  sobald RAW-Daten da sind, sofort verarbeiten (nicht auf die Karte warten):
  Debevec-Merge → Stitching (IMU-Startwert + Feature-Verfeinerung) → Equirect EXR + Clip-Maske
  → Nadir-Retusche (ComfyUI) → bei Bedarf Highlight-Rekonstruktion (DiffHDR-Service)
        ▼
KARTEN-INGEST (Amira / Venice 2)
  Clips werden über Timecode-Fenster ihrem Setup zugeordnet
  → HDRI und Plates hängen über das Setup zusammen ("Merge" = Verknüpfung, keine Berechnung)
```

**Wichtige Entscheidung:** Die Berechnung des HDRI braucht keine Footage. Der Server rechnet deshalb sofort, sobald die RAW-Daten ankommen. Beim Einlesen der Karte ist das HDRI bereits fertig; warten muss nur die Verknüpfung.

## 4. Capture-Modul in der iOS-App

**Kamera und Belichtung.** Ultraweitwinkel-Kamera, damit etwa ein Dutzend Positionen plus Zenit und Nadir reichen. Weißabgleich, Fokus und ISO werden pro Session fest eingestellt. Die Belichtungsreihen laufen manuell über `AVCaptureManualExposureBracketedStillImageSettings`. Pro Reihe sind typischerweise nur 3 Bilder möglich (`maxBracketedCapturePhotoCount` am Gerät abfragen), also mehrere Reihen hintereinander auslösen. Aufnahme in RAW bzw. ProRAW, damit die Daten linear sind.

**Führung.** CoreMotion-Attitude für die Zielpunkte auf der Kugel. Die Yaw-Referenz wird durch eine bewusste Nutzeraktion gesetzt (erster Auslöser = 0°), nicht beim ersten Sensorwert. Das Pitch-Vorzeichen sauber testen; genau diese beiden Fehler dokumentiert das Theodolite-Projekt aus seiner ersten Version. ARKit scheidet für die Aufnahme aus, weil es keine manuelle Belichtung und keine Belichtungsreihen zulässt.

**Grenzen der IMU.** CoreMotion driftet um etwa 1 bis 2 Grad, freihändig kommt Parallaxe dazu. Die IMU dient deshalb nur als Startwert; die endgültige Ausrichtung entsteht auf dem Server über Feature-Matching.

**Metadaten pro Frame.** Yaw, Pitch, Roll, FOV, Belichtungszeit, ISO, Zeitstempel, Kompassrichtung, Setup-ID. Format angelehnt an die `metadata.json` von Theodolite, damit man auch extern in Nuke oder Hugin nachstitchen kann.

**Auf dem Gerät.** Nur eine kleine LDR-Vorschau zur Abdeckungskontrolle am Set. Merge, Stitching und AI laufen auf dem Server.

## 5. Datenfluss, Sync und Zuordnung

**Datenmenge.** Grob 12 Positionen × 3 bis 5 Belichtungen × ca. 25 MB (ProRAW 12 MP) ergibt 1 bis 1,5 GB pro HDRI.

**Zwei Übertragungswege.** Über LTE gehen Metadaten und Vorschau sofort per Supabase an den Server, die RAW-Daten optional im Hintergrund. Im Studio läuft der volle Ingest übers lokale Netz. In der App eine Einstellung anbieten: „LTE: nur Metadaten und Vorschau" oder „alles". Beim self-hosted Supabase die Dateigrößengrenze im Storage hochsetzen und wiederaufnehmbare Uploads (TUS) verwenden.

**Zuordnung über das Setup.** HDRI und Footage werden nicht direkt miteinander gematcht, sondern beide an das Setup in Supabase gehängt. Ein HDRI gilt für eine Lichtsituation, also für mehrere Takes. Die App weiß, welches Setup wann aktiv war; beim Karten-Ingest werden die Clips über dieses Zeitfenster zugeordnet. Dafür muss die Kamera auf Tageszeit-Timecode laufen. Minutengenauigkeit genügt.

**Zustandsfolge in der Datenbank.**

| Zustand | Bedeutung |
|---|---|
| `captured` | auf dem iPhone gespeichert |
| `uploaded` | RAW-Daten auf dem Server |
| `processed` | EXR, Maske und Varianten fertig |
| `linked` | Clips des Setups eingelesen und verknüpft |

Ein Worker auf dem GPU-Rechner hört über Supabase Realtime oder eine Job-Tabelle mit und startet die Verarbeitung.

## 6. Verarbeitung auf dem Server

**Merge.** Eigener Debevec-Merge in Python/OpenCV mit den exakten Belichtungszeiten aus den Capture-Metadaten. Nicht den Luminance Stack Processor aus ComfyUI verwenden: dessen Standardalgorithmus ist für KI-generierte Belichtungsreihen gedacht, die keiner realen Physik folgen.

**Stitching.** IMU-Werte als Startwert, danach featurebasierte Verfeinerung (Architektur wie bei jjc256/photosphere). Ausgabe Equirectangular 2:1, linear, 32-Bit-EXR.

**Clip-Maske.** Alle Pixel, die selbst in der dunkelsten Belichtung noch gesättigt sind. Ist sie leer (häufig bei Innenaufnahmen), entfällt die Highlight-Rekonstruktion komplett.

**Nadir-Retusche (fast immer).** In ComfyUI mit Pol-Maske und Flux-Inpainting. Diffusionsmodelle arbeiten im LDR-Bereich, deshalb: Nadir-Bereich tonemappen, inpainten, zurück linearisieren. Das ist vertretbar, weil der Nadir kaum zur Beleuchtung beiträgt.

**Highlight-Rekonstruktion (nur bei Bedarf).** DiffHDR rekonstruiert Sonne und geclippte Lichter. Die AI-Radianz wird nur innerhalb der Clip-Maske eingesetzt und am Maskenrand in der Helligkeit an die gemessenen Werte angepasst.

**Ausgaben pro HDRI.**

| Datei | Inhalt | Zweck |
|---|---|---|
| Lighting-EXR | mit rekonstruierter Sonne, 1024×2048 | Image Based Lighting in Unreal |
| Backplate-EXR | volle Auflösung, gemessen, nur Nadir retuschiert | sichtbarer Hintergrund, HDRI Backdrop |
| Maskenkanal | gemessen vs. rekonstruiert | Nachvollziehbarkeit für Lighting und Comp |
| Rohversion | rein gemessen, ohne AI | Referenz, Schalter „nur gemessen" |

**Warum zwei EXRs:** DiffHDR liefert nur 1024×2048. Das reicht für Beleuchtung, nicht für einen sichtbaren Hintergrund.

## 7. AI-Komponenten

### DiffHDR (Eyeline Labs / Netflix, mit Paul Debevec)

- Repo: https://github.com/Eyeline-Labs/DiffHDR (ECCV 2026, Apache-2.0)
- Panorama-Erweiterung: https://eyeline-labs.github.io/HDRI/ (SIGGRAPH 2026 Poster)
- Gewichte: https://huggingface.co/ZhengmingYu/DiffHDR

Funktionsweise: LDR-zu-HDR als generatives Inpainting von Radianz im Latent Space eines Video-Diffusionsmodells, im Log-Gamma-Farbraum. Der Panorama-Modus hat eine eigene LoRA (`DiffHDR_Pano.safetensors`, 58 MB) und ein eigenes Skript (`infer_hdri.py`). Er erkennt überbelichtete Bereiche über Luma- und Kanal-Clipping und gibt ein HDR-EXR in 1024×2048 aus. Die Panorama-Variante wurde zusätzlich mit Losses trainiert, die das Beleuchtungsverhalten prüfen (diffus über Spherical Harmonics, glänzend über GGX). Laut Autoren treffen die rekonstruierten Hauptlichter Intensität, Farbe und Richtung gut. Basismodell ist Wan2.1-VACE-14B (ca. 75 GB Download). 10 statt 50 Schritte liefern laut Autoren oft vergleichbare Qualität.

**Integration:** DiffHDR ist kein ComfyUI-Node, sondern ein Python-Skript. Es läuft als eigener Dienst (z. B. FastAPI) mit dauerhaft geladenem Modell neben ComfyUI, damit kein Kaltstart pro Job anfällt und es nicht mit Flux um den VRAM konkurriert.

### ComfyUI

| Aufgabe | Node / Paket | Bemerkung |
|---|---|---|
| Nadir- und Pol-Retusche | ComfyUI_pytorch360convert | Pol-Maske und Naht-Maske (Bild um 50 % verschieben) |
| Lücken / fehlende Frames | ComfyUI-Panorama-Stickers (nomadoor) | UI für FLUX.2-Klein-360-ERP-Outpaint-LoRA |
| EXR speichern | eingebauter Node SaveImageAdvanced | 32-Bit-Float, Eingangsfarbraum „linear" schreibt unverändert scene-linear |
| EXR laden | ComfyUI-HQ-Image-Save (spacepxl) | empfiehlt `--fp32-vae` |
| Nicht verwenden | Luminance Stack Processor | für KI-Belichtungsreihen gedacht, nicht für echte |
| Overkill für Stills | ComfyUI-Seamless-Equirectangular / VR-Outpaint-Tools | für 360°-Video mit LTX-2.3 |

**Anbindung des Workers:** normale ComfyUI-API, also `/upload/image`, `/prompt` mit Workflow im API-Format, Fortschritt per WebSocket, Ergebnis über `/view`.

## 8. Hardware und Laufzeit (RTX 4090, 24 GB)

Es gibt keine veröffentlichte Laufzeitmessung für den Panorama-Modus. Die folgenden Werte sind Schätzungen aus der Modellarchitektur und müssen gemessen werden.

| Schritt | Schätzung |
|---|---|
| DiffHDR, 10 Schritte, Modell warm | ca. 1–3 min |
| DiffHDR, 50 Schritte, Modell warm | ca. 5–12 min |
| Kaltstart (Gewichte laden) | zusätzlich ca. 1 min oder mehr |
| Nadir-Retusche mit Flux | ca. 20–60 s |

Herleitung: Ein Frame 1024×2048 ergibt im Wan-Modell grob 8'000 Tokens, ein Wan-Video in 480p mit 81 Frames etwa viermal so viele (ca. 30 s pro Schritt auf der 4090). Die Attention wächst quadratisch, daher etwa 4–10 s pro Schritt plus VACE-Kontext, T5 und VAE.

**Engpass ist der VRAM, nicht die Rechenzeit.** Das 14B-Modell braucht in bf16 allein für die Gewichte um die 28 GB, dazu T5. Zwei Wege:

1. **FP8-Quantisierung des DiT:** passt auf die Karte, am schnellsten; Qualitätseinfluss mit der LoRA testen.
2. **Offloading über DiffSynth:** Gewichte im System-RAM, pro Schritt über PCIe nachladen; ca. 1–3 s zusätzlich pro Schritt, mindestens 64 GB, besser 128 GB RAM.

In der Praxis ist ein HDRI wenige Minuten nach dem Upload fertig, bei Innenaufnahmen ohne Clipping fast sofort.

## 9. Training und Lizenzen

**Kein Training nötig.** Alles lässt sich herunterladen: Wan2.1-VACE-14B (Hugging Face, ca. 75 GB), die beiden DiffHDR-LoRAs (je 58 MB), die Flux-360-LoRA, die ComfyUI-Nodes über den Manager.

**Optional später:** DiffHDR bringt ein Trainingsskript mit (`train_hdr.py`). Wenn bei Sonnenscans zusätzlich mit ND-Filter gemessen wird, entstehen nebenbei echte Ground-Truth-Paare, mit denen man das Modell auf eigene Bedingungen nachtrainieren könnte.

**Lizenzen vor Produktionseinsatz prüfen.** DiffHDR: Apache-2.0 (enthält modifizierten DiffSynth-Code, ebenfalls Apache-2.0). Wan2.1: Apache-2.0, verifizieren. Flux-Modelle: Lizenz der konkreten Variante kontrollieren, da sie sich je nach Größe unterscheiden kann. Für kommerzielle Spielfilmproduktion relevant.

## 10. Recherche: bestehende Capture-Projekte

Ein fertiges Open-Source-Paket in Swift, das HDRI-Aufnahme mit Gyro-Platzierung für VFX komplett abdeckt, existiert nicht. Das einzige iOS-Produkt mit genau diesem Zweck ist kommerziell und geschlossen: HDReye (https://www.hdreye.app/), nützlich als Benchmark.

| Projekt | Was es ist | Nutzen für uns |
|---|---|---|
| [Simple360Camera](https://github.com/l0ckd0wn84/Simple360Camera) | natives Swift, SwiftUI + AVFoundation + CoreMotion, iOS 17 | Gerüst für Capture und `MotionGuide.swift`; Stitcher (nur vertikale Streifen) unbrauchbar |
| [Nalabo360](https://github.com/Agb242/nalabo360) | Kotlin Multiplatform mit iOS-Teil (AVFoundation, CoreMotion) | Konzept: Rotation pro Frame ist gemessen statt geschätzt; Stitcher mit Multiband-Blending als Referenz |
| [jjc256/photosphere](https://github.com/jjc256/photosphere) | Web-App | beste Architekturvorlage: IMU-initialisiertes, featurebasiertes Stitching, FOV-abhängiger Aufnahmeplan mit Pol-Aufnahmen |
| [Theodolite / 360-sphere](https://github.com/DmitryRUS80/360-sphere) | PWA | Metadatenformat (Yaw/Pitch/Roll/FOV pro Frame), dokumentierte Fehler bei Yaw-Referenz und Pitch-Vorzeichen |
| [skfh90/360-photo-app](https://github.com/skfh90/360-photo-app) | Android | saubere Modulaufteilung (Zielliste, Projektion, Overlay) als Vorbild |
| [Authydra](https://github.com/iamagod/Authydra) | Ricoh-Theta-Plugin | HDR-Logik für On-Set-VFX: Auto-Messung, niedrigste ISO, Reihe, OpenCV-Merge zu EXR; benennt das Sonnen-Clipping-Problem |
| [OpenCVSwiftStitch](https://github.com/foundry/OpenCVSwiftStitch) | Beispielprojekt | OpenCV in Swift über Objective-C++-Wrapper einbinden |

## 11. Risiken

- **DiffHDR ist junger Forschungscode** mit wenigen Commits; Reibung beim Aufsetzen einplanen.
- **VRAM der 4090 ist knapp;** FP8 oder Offloading nötig, Qualitätseinfluss offen.
- **Auflösung 1024×2048** begrenzt DiffHDR auf das Lighting-EXR.
- **Sonne:** Das iPhone clippt die Sonne auch bei kürzester Belichtung. Ohne ND-Vorsatz ist der Sonnenwert immer rekonstruiert, nie gemessen.
- **Gyro-Drift und Parallaxe** beim freihändigen Scannen; ein Nodalpunkt-Adapter am Stativ würde die Stitching-Qualität deutlich verbessern.
- **Lizenzen** der AI-Modelle für kommerziellen Einsatz.

## 12. Nächste Schritte

**Phase 0 – Validierung (vor dem Bau, ca. ein Nachmittag).** Einige Außen-HDRIs mit Sonne von Poly Haven künstlich auf iPhone-Niveau clippen, DiffHDR auf der 4090 laufen lassen und in Unreal das Licht an einer Graukugel und einer Chromkugel mit dem Original vergleichen. Parallel Benchmark: FP8 gegen bf16 mit Offloading, jeweils 10 und 20 Schritte; Zeit, VRAM-Spitze und EXR protokollieren.

**Phase 1 – Capture in der App.** Guided Capture, Belichtungsreihen in RAW, Metadaten pro Frame, LDR-Vorschau, Zuordnung zum aktiven Setup.

**Phase 2 – Datenmodell und Sync.** Supabase-Schema (Setup, HDRI-Capture, Frames, Jobs, Clips) mit der Zustandsfolge, TUS-Upload, LTE-Einstellung, Worker-Anbindung.

**Phase 3 – Server-Pipeline ohne AI.** Debevec-Merge, Stitching, EXR-Ausgabe, Clip-Maske.

**Phase 4 – AI-Stufen.** ComfyUI-Workflow für den Nadir, DiffHDR-Dienst, Energieabgleich am Maskenrand, Maskenkanal im EXR.

**Phase 5 – Verknüpfung und Auslieferung.** Clip-Zuordnung beim Karten-Ingest, Ablagestruktur pro Drehtag und Setup, Import in Unreal und Nuke.

## 13. Offene Fragen

- Wie viele Positionen und Belichtungsstufen braucht es in der Praxis (Test mit Ultraweitwinkel)?
- ND-Filter für Sonnenscans einsetzen, um echte Messwerte zu bekommen?
- Ausrichtung des HDRI: Kompass-Nord oder Bezug zur Kamerarichtung des Setups?
- Farbabgleich zwischen HDRI und Plate, z. B. über eine Farbtafel, die in beiden auftaucht?
- Welche iPhone-Modelle sind im Einsatz (ProRAW, Ultraweitwinkel-Auflösung)?
