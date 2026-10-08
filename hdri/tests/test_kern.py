"""Künstliche Szene mit bekanntem Licht: aufnehmen wie das iPhone (Positionen × Belichtungen, gesättigt), dann
zusammenführen und zum Panorama legen; das Ergebnis muss dem Original entsprechen."""

import numpy as np
import pytest

from hdri_dienst import exr
from hdri_dienst.merge import Belichtung, zusammenfuehren
from hdri_dienst.projektion import Kamera, Position, panorama, quaternion_zu_matrix, richtungen

# Kamera → Welt bei Yaw 0, Pitch 0: Kamera x → Welt x, Kamera y (oben) → Welt z, Blick −z → Welt y.
R0 = np.array([[1.0, 0, 0], [0, 0, -1], [0, 1, 0]])


def lage(yaw_grad: float, pitch_grad: float) -> np.ndarray:
    """Lage aus Yaw (nach rechts, zu +x) und Pitch (nach oben)."""
    p, y = np.radians(pitch_grad), np.radians(-yaw_grad)
    rx = np.array([[1, 0, 0], [0, np.cos(p), -np.sin(p)], [0, np.sin(p), np.cos(p)]])
    rz = np.array([[np.cos(y), -np.sin(y), 0], [np.sin(y), np.cos(y), 0], [0, 0, 1]])
    return rz @ rx @ R0


def szene(hoehe: int) -> np.ndarray:
    """Panorama mit sanftem Himmelsverlauf, farbigen Flächen und einer sehr hellen „Sonne“."""
    w = richtungen(hoehe)
    himmel = 0.2 + 0.8 * np.clip(w[..., 2], 0, 1)
    bild = np.stack([himmel * 0.6, himmel * 0.8, himmel], axis=-1)
    bild[..., 0] += 0.5 * (np.sin(4 * np.arctan2(w[..., 0], w[..., 1])) > 0.5)  # rote Streifen am Horizont
    sonne = np.arccos(np.clip(w @ np.array([0.3, 0.5, 0.81]) / np.linalg.norm([0.3, 0.5, 0.81]), -1, 1)) < 0.05
    bild[sonne] = 2000.0
    return bild.astype(np.float32)


def aufnehmen(gt: np.ndarray, kamera: Kamera, r: np.ndarray, faktor: float) -> np.ndarray:
    """Lochkamera-Bild der Szene, belichtet und bei 1 gesättigt (wie der Sensor)."""
    import cv2

    u, v = np.meshgrid(np.arange(kamera.breite) + 0.5, np.arange(kamera.hoehe) + 0.5)
    strahl = np.stack([(u - kamera.breite / 2) / kamera.fx, -(v - kamera.hoehe / 2) / kamera.fy, -np.ones_like(u)], -1)
    strahl /= np.linalg.norm(strahl, axis=-1, keepdims=True)
    welt = strahl @ r.T
    h = gt.shape[0]
    az = np.arctan2(welt[..., 0], welt[..., 1])
    hw = np.arcsin(np.clip(welt[..., 2], -1, 1))
    mx = ((az / (2 * np.pi) + 0.5) * 2 * h - 0.5).astype(np.float32)
    my = ((0.5 - hw / np.pi) * h - 0.5).astype(np.float32)
    bild = cv2.remap(gt, mx, my, cv2.INTER_LINEAR, borderMode=cv2.BORDER_WRAP)
    return np.clip(bild * faktor, 0, 1).astype(np.float32)


def test_quaternion():
    assert np.allclose(quaternion_zu_matrix((1, 0, 0, 0)), np.eye(3))
    # 90° um z: x → y
    r = quaternion_zu_matrix((np.cos(np.pi / 4), 0, 0, np.sin(np.pi / 4)))
    assert np.allclose(r @ [1, 0, 0], [0, 1, 0])


def test_merge_linear_und_clip():
    wahr = np.array([[[0.01, 0.5, 30.0]]], dtype=np.float32) * np.ones((4, 4, 1), np.float32)
    reihe = [Belichtung(np.clip(wahr * f, 0, 1), zeit_s=f, iso=100) for f in (0.01, 0.1, 1.0, 10.0)]
    hdr, clip = zusammenfuehren(reihe)
    assert np.allclose(hdr, wahr, rtol=0.02)
    assert not clip.any()
    # Sonne: auch in der kürzesten Belichtung gesättigt → Maske, Wert = Untergrenze
    hell = np.full((2, 2, 3), 1000.0, np.float32)
    hdr2, clip2 = zusammenfuehren([Belichtung(np.clip(hell * f, 0, 1), f, 100) for f in (0.01, 0.1)])
    assert clip2.all() and np.allclose(hdr2, 100.0)


def test_panorama_aus_aufnahmen():
    hoehe = 128
    gt = szene(hoehe)
    kamera = Kamera(breite=160, hoehe=120, hfov_grad=100, vfov_grad=80)
    positionen = []
    for pitch, schritte in ((0, 6), (45, 4), (-45, 4), (90, 1)):
        for i in range(schritte):
            r = lage(360 / schritte * i, pitch)
            reihe = [Belichtung(aufnehmen(gt, kamera, r, f), zeit_s=f / 4, iso=400) for f in (0.0005, 0.02, 0.5, 4.0)]
            hdr, clip = zusammenfuehren(reihe)
            positionen.append(Position(hdr, r, kamera, clip))
    bild, maske, abdeckung = panorama(positionen, hoehe)
    assert bild.shape == (hoehe, 2 * hoehe, 3)
    bewertbar = (abdeckung > 0.5) & (gt.max(axis=2) < 100)
    # Ränder der Interpolation abziehen: mittlere relative Abweichung unter 3 %
    fehler = np.abs(bild - gt)[bewertbar] / np.maximum(gt[bewertbar], 0.05)
    assert np.median(fehler) < 0.03, np.median(fehler)
    # Sonne wird als gesättigt markiert
    assert maske[gt.max(axis=2) > 100].mean() > 0.5
    # Nadir ohne Aufnahme bleibt ein erkennbares Loch
    assert (abdeckung[-3:, :] == 0).all()


def test_exr_hin_und_zurueck(tmp_path):
    bild = np.random.default_rng(1).random((8, 16, 3)).astype(np.float32) * 100
    maske = np.zeros((8, 16), np.float32)
    maske[2, 3] = 1
    exr.schreiben(tmp_path / "x.exr", bild, {"clip": maske}, {"stage_ingest": "test"})
    rgb, rest = exr.lesen(tmp_path / "x.exr")
    assert np.array_equal(rgb, bild)
    assert np.array_equal(rest["clip.Y"], maske)
    assert not (tmp_path / "x.exr.teil").exists()


def matrix_zu_quaternion(r: np.ndarray) -> list[float]:
    w = np.sqrt(max(0.0, 1 + r[0, 0] + r[1, 1] + r[2, 2])) / 2
    x = np.copysign(np.sqrt(max(0.0, 1 + r[0, 0] - r[1, 1] - r[2, 2])) / 2, r[2, 1] - r[1, 2])
    y = np.copysign(np.sqrt(max(0.0, 1 - r[0, 0] + r[1, 1] - r[2, 2])) / 2, r[0, 2] - r[2, 0])
    z = np.copysign(np.sqrt(max(0.0, 1 - r[0, 0] - r[1, 1] + r[2, 2])) / 2, r[1, 0] - r[0, 1])
    return [w, x, y, z]


def test_aufnahme_von_ordner_bis_exr(tmp_path):
    """Wie vom Plate Assistant: metadata.json + Bilder je Position und EV; Befehl `verarbeiten` schreibt EXR + JPG."""
    import json

    from hdri_dienst.__main__ import main

    gt = szene(128)
    kamera = Kamera(breite=120, hoehe=160, hfov_grad=80, vfov_grad=100)  # Hochformat
    frames = []
    position = 0
    for pitch, schritte in ((0, 6), (45, 4), (-45, 4), (90, 1)):
        for i in range(schritte):
            r = lage(360 / schritte * i, pitch)
            assert np.allclose(quaternion_zu_matrix(matrix_zu_quaternion(r)), r)
            for ev, f in ((-4, 0.002), (0, 0.05), (4, 0.8)):
                name = f"p{position}e{ev}"
                np.save(tmp_path / f"{name}.npy", aufnehmen(gt, kamera, r, f))
                frames.append(
                    {"id": name, "position": position, "ev": ev, "lage_quaternion": matrix_zu_quaternion(r),
                     "belichtung_s": f, "iso": 100, "pfad": f"h1/{name}.npy"}
                )
            position += 1
    meta = {"format_version": 1, "hdri": {"id": "h1", "format": "dng", "hfov_grad": 80, "vfov_grad": 100}, "frames": frames}
    (tmp_path / "metadata.json").write_text(json.dumps(meta))
    assert main(["verarbeiten", str(tmp_path), "--hoehe", "128"]) == 0
    bild, rest = exr.lesen(tmp_path / "h1_gemessen.exr")
    assert bild.shape == (128, 256, 3) and "clip.Y" in rest and "abdeckung.Y" in rest
    assert (tmp_path / "h1_gemessen.jpg").exists()
    bewertbar = (rest["abdeckung.Y"] > 0.5) & (gt.max(axis=2) < 100)
    fehler = np.abs(bild - gt)[bewertbar] / np.maximum(gt[bewertbar], 0.05)
    assert np.median(fehler) < 0.03


def test_heic_wird_abgelehnt(tmp_path):
    import json

    from hdri_dienst.aufnahme import AufnahmeFehler, laden

    (tmp_path / "metadata.json").write_text(json.dumps({"format_version": 1, "hdri": {"format": "heic"}, "frames": [{}]}))
    with pytest.raises(AufnahmeFehler, match="nur DNG"):
        laden(tmp_path)


def test_waechter_erkennt_unreal():
    from hdri_dienst.waechter import stage_aktiv

    assert stage_aktiv(["explorer.exe", "UnrealEditor.exe".lower()]) == "unrealeditor.exe"
    assert stage_aktiv(["explorer.exe", "chrome.exe"]) is None


def test_server_schreibt_nur_jobs():
    from hdri_dienst.server import Server, ServerFehler

    s = Server("https://beispiel.invalid", "k", "e", "p")
    with pytest.raises(ServerFehler, match="schreibt hdri.zustand nicht"):
        s._anwenden([{"tabelle": "hdri", "feld": "zustand"}])
    with pytest.raises(ServerFehler, match="schreibt hdri_job.geloescht nicht"):
        s._anwenden([{"tabelle": "hdri_job", "feld": "geloescht"}])


# OpcodeList3 eines echten DNG (iPhone 13 Pro, Ultraweitwinkel), nur die Korrekturwerte des Objektivs.
IPHONE_OPCODES = bytes.fromhex("000000020000000e0103000000000000000000b0000000013ff08efb400000000000000000000000bfe1b81b3c3590a1000000000000000040052ed68c6d956c0000000000000000c018aaef1285fb93000000000000000040212c92441fff6a0000000000000000c01aa8c7fd844b52000000000000000040053866a869e05d0000000000000000bfdb00937be8308b0000000000000000000000000000000000000000000000003ff00000000000003fe00000000000003fe00000000000000000000100000003010300000000000000000038400b7fad60000000bfff2837600000004022978d20000000c0338f62e0000000402b3fb9800000003fdfebcfeb8272f63fe01325d7b5460e")


def test_opcodes_des_iphone():
    from hdri_dienst import opcodes

    verz, vign = opcodes.lesen(IPHONE_OPCODES)
    assert verz.kehrwert and abs(verz.kr[0] - 1.034908) < 1e-5 and verz.cx == 0.5
    assert abs(vign.k[0] - 3.437342) < 1e-5 and abs(vign.cx - 0.498768) < 1e-5
    # Randabdunklung: Mitte unverändert, Ecke deutlich aufgehellt
    bild = np.ones((30, 40, 3), np.float32)
    v = opcodes.vignette_anwenden(bild, vign)
    assert abs(v[15, 20, 0] - 1) < 0.05 and v[0, 0, 0] > 4
    # Entzerrung: Mitte bleibt (Faktor ≈ 0,97 nahe der Mitte), Bildmitte zeigt weiter die Mitte
    gitter = np.zeros((101, 101, 3), np.float32)
    gitter[50, 50] = 1
    e = opcodes.verzerrung_anwenden(gitter, verz)
    assert e[50, 50, 0] > 0.5


def test_zweite_ausloesung_wird_ausgerichtet():
    """Ab Build 12 hat jede Auslösung ihre Lage: ein um 1,5° verdrehtes Bild wird auf die erste Lage gedreht."""
    from hdri_dienst.aufnahme import ausrichten

    gt = szene(512)
    kamera = Kamera(breite=300, hoehe=400, hfov_grad=80, vfov_grad=100)
    r1, r2 = lage(10, 20), lage(11.2, 20.9)
    erstes = aufnehmen(gt, kamera, r1, 0.3)
    zweites = aufnehmen(gt, kamera, r2, 0.3)
    gedreht = ausrichten(zweites, kamera, r2, r1)
    innen = (slice(60, 340), slice(60, 240))
    vorher = np.abs(zweites[innen] - erstes[innen]).mean()
    nachher = np.abs(gedreht[innen] - erstes[innen]).mean()
    assert nachher < vorher * 0.3, (vorher, nachher)


def test_multiband_wie_panorama():
    """Multiband-Nähte geben dieselbe Strahldichte wie das einfache Mischen (künstliche Szene, ohne Parallaxe)."""
    from hdri_dienst.projektion import panorama_multiband

    hoehe = 128
    gt = szene(hoehe)
    kamera = Kamera(breite=160, hoehe=120, hfov_grad=100, vfov_grad=80)
    positionen = []
    for pitch, schritte in ((0, 6), (45, 4), (-45, 4), (90, 1)):
        for i in range(schritte):
            r = lage(360 / schritte * i, pitch)
            reihe = [Belichtung(aufnehmen(gt, kamera, r, f), zeit_s=f / 4, iso=400) for f in (0.0005, 0.02, 0.5, 4.0)]
            hdr, clip = zusammenfuehren(reihe)
            positionen.append(Position(hdr, r, kamera, clip))
    bild, maske, abdeckung = panorama_multiband(positionen, hoehe, stufen=4)
    bewertbar = (abdeckung > 0.5) & (gt.max(axis=2) < 100)
    fehler = np.abs(bild - gt)[bewertbar] / np.maximum(gt[bewertbar], 0.05)
    assert np.median(fehler) < 0.03, np.median(fehler)
    assert maske[gt.max(axis=2) > 100].mean() > 0.5


def test_ergebnis_hochladen_nur_eigene_datei(tmp_path):
    """Der Dienst lädt nur `<hdri_id>/ergebnis.jpg` hoch, verkleinert auf 2048 Pixel Breite."""
    import cv2

    from hdri_dienst.__main__ import klein_jpg
    from hdri_dienst.server import SPEICHER_DARF

    assert SPEICHER_DARF.match("01M4DDQMJE3PJBD8R4PJRAS32W/ergebnis.jpg")
    assert not SPEICHER_DARF.match("01M4DDQMJE3PJBD8R4PJRAS32W/vorschau.jpg")  # gehört dem iPhone
    assert not SPEICHER_DARF.match("../ergebnis.jpg")
    pfad = tmp_path / "p.jpg"
    cv2.imwrite(str(pfad), np.full((2048, 4096, 3), 128, np.uint8))
    klein = cv2.imdecode(np.frombuffer(klein_jpg(pfad), np.uint8), cv2.IMREAD_COLOR)
    assert klein.shape[:2] == (1024, 2048)


def test_ki_lichter_nur_im_clip_und_nie_dunkler():
    """Die KI-Schätzung ersetzt nur geclippte Pixel und nur nach oben; alles andere bleibt die Messung."""
    from hdri_dienst import ki

    h, w = 512, 1024
    hdr = np.full((h, w, 3), 0.2, np.float32)
    hdr[100:140, 300:360] = 1.5  # Fenster, gesättigt gemessen (Untergrenze)
    clip = np.zeros((h, w), np.float32)
    clip[100:140, 300:360] = 1
    rgb8, k, h_klein, c_klein = ki.eingabe(hdr, clip)
    assert rgb8.shape == (ki.HOEHE, ki.BREITE, 3) and rgb8[c_klein].min() >= 250  # Fenster weiss
    # Schätzung: halbe Belichtung von h_klein, im Fenster 6× heller als gemessen
    schaetzung = h_klein * 0.5
    schaetzung[c_klein] *= 6
    aus, erfunden, b = ki.zusammensetzen(hdr, clip, schaetzung, h_klein, c_klein)
    assert abs(b["massstab"] - 2.0) < 1e-3
    assert np.allclose(aus[clip == 0], hdr[clip == 0])
    assert aus[120, 330, 0] > hdr[120, 330, 0] * 4
    assert erfunden[clip == 0].max() == 0 and erfunden[120, 330] == 1
    assert b["licht_im_clip_faktor"] > 4


def test_ki_einstellungen(tmp_path):
    from hdri_dienst import ki

    assert ki.einstellungen(tmp_path / "fehlt.env") is None
    (tmp_path / "ki.env").write_text("DIFFHDR_ORDNER=a\nDIFFHDR_PYTHON=b\nMODEL_BASE=c\n", encoding="utf-8")
    assert ki.einstellungen(tmp_path / "ki.env") is None  # unvollständig
    (tmp_path / "ki.env").write_text("# KI\nDIFFHDR_ORDNER=a\nDIFFHDR_PYTHON=b\nMODEL_BASE=c\nDIFFHDR_LORA=d\n", encoding="utf-8")
    assert ki.einstellungen(tmp_path / "ki.env")["DIFFHDR_LORA"] == "d"
