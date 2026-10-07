# Recherche: Kamerakarten geprüft kopieren

Stand 07.10.2026. Grundlage für den Rust-Kern von Stage Ingest (Tauri 2, Mac und Windows). Quellcode der genannten Projekte wurde gelesen, nicht nur ihre READMEs. Unsichere Punkte stehen jeweils unter **Offen**.

Feste Anforderungen: Karte einmal lesen, XXH3-128 beim Lesen (optional MD5), gleichzeitig an mehrere Ziele schreiben, jedes Ziel danach ganz zurücklesen, ohne dass der OS-Cache antwortet, ASC MHL auf jedem Ziel, PDF-Bericht, „sicher zum Formatieren“ erst ab N unabhängigen Kopien auf verschiedenen physischen Platten.

---

## 1. ASC MHL v2

Quellen: [Spezifikation v1.0 (PDF)](https://github.com/ascmitc/mhl-specification/tree/main/Specification), [Implementation Guidelines](https://github.com/ascmitc/mhl-specification/tree/main/ImplementationGuidelines), [Referenz-Implementierung `ascmitc/mhl` (Python, MIT)](https://github.com/ascmitc/mhl), [XSD](https://github.com/ascmitc/mhl/tree/master/xsd).
Hinweis zur Benennung: Die Spezifikation heisst „v1.0“, das XML-Format darin trägt aber `version="2.0"` und den Namespace `urn:ASC:MHL:v2.0`.

### Ablage

```
<Reel>/                         ← der Ordner, den das MHL abdeckt (bei uns 01_KAMERA/<Reel>/)
  Clips/…
  ascmhl/
    0001_<Reel>_2026-10-07_095203Z.mhl     ← eine Datei pro Generation
    0002_<Reel>_2026-10-08_141500Z.mhl
    ascmhl_chain.xml                        ← Verzeichnis aller Generationen
```

- **Dateiname** (Spez. 6.3): `<Nummer>_<Ordnername>_<YYYY-MM-DD>_<HHMMSS>Z.mhl`. Nummer 4-stellig, fortlaufend ab 1, mehr Stellen erst ab 10000. Datum und Uhrzeit in UTC, **einmal beim Start des Vorgangs** festgelegt und für alle Dateien dieses Vorgangs gleich. Die Referenz macht `f"{index:04d}_{folder}_{utc:%Y-%m-%d_%H%M%SZ}.mhl"`.
- **Chain-Datei** `ascmhl_chain.xml` (Namespace `urn:ASC:MHL:DIRECTORY:v2.0`): pro Generation ein `<hashlist sequencenr="n">` mit `<path>` (Dateiname) und `<c4>` (C4-Hash **der .mhl-Datei selbst**). Damit fällt eine nachträglich geänderte .mhl-Datei auf.
- Verschachtelte Historien (ein Unterordner mit eigenem `ascmhl/`) werden über `<references><hashlistreference>` eingebunden. Das brauchen wir zunächst nicht.

### XML einer .mhl-Datei

Pflicht sind nur `creatorinfo` (mit `creationdate`, `hostname`, `tool`) und `processinfo` (mit `process`). `hashes`, `roothash`, `ignore`, `author`, `location`, `comment`, `metadata` sind optional, sollten aber gefüllt werden.

- `process`: `in-place` (vorhandene Daten versiegeln), **`transfer` (Kopie, für uns)**, `flatten`.
- Jeder Hash ist ein Element mit dem Namen des Verfahrens. Erlaubt: **`xxh128`**, `xxh3` (64 Bit), `xxh64`, **`md5`**, `sha1`, `c4`. **Ein `<hash>` darf mehrere Verfahren enthalten**, zum Beispiel xxh128 und md5 nebeneinander.
- Attribute am Hash: `action` = `original` (erster Hash dieser Datei in der Historie) · `verified` (neu berechnet und gleich wie in einer früheren Generation) · `failed` (neu berechnet und **anders**). Dazu `hashdate` (Zeitpunkt der Berechnung).
- `<path>` ist relativ zum Ordner, mit `/` als Trenner. Attribute `size`, optional `creationdate` und `lastmodificationdate`.
- `<directoryhash>` je Unterordner, `<roothash>` in `processinfo`, beide mit `<content>` und `<structure>`.

### Hash-Darstellung (Spez. Anhang D)

- Seed 0, **Big Endian**, Hex in Kleinbuchstaben. xxh128 = 32 Hex-Zeichen. Beispiel aus der Spezifikation: `00fd03cd9996ee8cf8be6a756bf82a42`.
- Geprüft: Python `xxhash.xxh3_128(b"abc").hexdigest()` ist `06b05ab6733a618578af5f94892f3950`, also dasselbe wie die Zahl als 32-stelliges Hex. In Rust heisst das `format!("{:032x}", hasher.digest128())`. Dieser Wert eignet sich als Unit-Test.
- **C4** = SHA-512, als Zahl Base58 kodiert (Alphabet ohne 0, O, I, l), **links mit `1` auf 88 Zeichen aufgefüllt**, davor `c4` (zusammen 90 Zeichen). Achtung: Ein normales Base58 (z. B. Crate `bs58`) füllt nicht auf. Gegen die Referenz nachgerechnet, liefert das bei **rund 19 % aller Hashes einen falschen C4**. Die Rust-Implementierung MASH (siehe unten) hat genau diesen Fehler. Die Referenz-Implementierung steht in `ascmhl/hasher.py`, Klasse `C4`.

### Ordner-Hashes (Spez. Anhang G, `hasher.py` → `DirectoryHashContext`)

Für jeden Ordner, von unten nach oben:
- **content** = Hash über die Liste der Inhalts-Hashes aller Kinder (Dateien: ihr Hash, Unterordner: ihr content-Hash).
- **structure** = Hash über die Liste von `hash(name_utf8 ‖ kind_bytes)` aller Kinder. Bei einer Datei ist `kind_bytes` ihr Inhalts-Hash, bei einem Unterordner sein **structure**-Hash. `name` ist nur der Basisname.
- „Hash über eine Liste“ heisst: die Hex-Strings **lexikographisch sortieren**, jeden wieder in Bytes umwandeln, alles in **einen neuen Hasher desselben Verfahrens** schreiben und das Ergebnis wieder als Hex ausgeben. Eine leere Liste ergibt den Hash von nichts.
- Bei jedem Verfahren getrennt (md5 nur aus md5-Kindern). Die Ignore-Muster (Standard: `.DS_Store`, `ascmhl`, `ascmhl/`) stehen in `processinfo/ignore` und dürfen nicht mitgehasht werden.

### Minimales gültiges Beispiel

Mit `ascmhl create -h xxh128 -h md5` aus der Referenz erzeugt und gegen das XSD geprüft. Gekürzt auf eine Datei, `roothash`/`directoryhash` weggelassen (optional):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<hashlist version="2.0" xmlns="urn:ASC:MHL:v2.0">
  <creatorinfo>
    <creationdate>2026-10-07T09:52:03+00:00</creationdate>
    <hostname>ingest-mac</hostname>
    <tool version="0.1.0">Stage Ingest</tool>
  </creatorinfo>
  <processinfo>
    <process>transfer</process>
    <ignore><pattern>.DS_Store</pattern><pattern>ascmhl</pattern><pattern>ascmhl/</pattern></ignore>
  </processinfo>
  <hashes>
    <hash>
      <path size="3" lastmodificationdate="2026-10-01T10:00:00+00:00">Sidecar.txt</path>
      <md5 action="original" hashdate="2026-10-07T09:52:03+00:00">900150983cd24fb0d6963f7d28e17f72</md5>
      <xxh128 action="original" hashdate="2026-10-07T09:52:03+00:00">06b05ab6733a618578af5f94892f3950</xxh128>
    </hash>
  </hashes>
</hashlist>
```

```xml
<?xml version="1.0" encoding="UTF-8"?>
<ascmhldirectory xmlns="urn:ASC:MHL:DIRECTORY:v2.0">
  <hashlist sequencenr="1">
    <path>0001_A001R1AB_2026-10-07_095203Z.mhl</path>
    <c4>c4…(90 Zeichen, C4 der .mhl-Datei)</c4>
  </hashlist>
</ascmhldirectory>
```

So ordnen wir die Generationen beim Kopieren ein. Die Karte selbst wird nie beschrieben.
- Die Karte hat noch kein `ascmhl/` (der Normalfall bei ARRI): Auf jedem Ziel entsteht Generation 0001 mit `process=transfer` und `action=original`, Hash aus dem Lesen der Karte. Geschrieben wird sie **erst nach bestandener Rückleseprüfung**.
- Die Karte hat schon ein `ascmhl/`: Den Ordner 1:1 mitkopieren und eine neue Generation anhängen, mit `verified` oder `failed` gegen die Historie (so macht es die Referenz, Szenario 2).

### Rust-Implementierung?

Auf crates.io gibt es **keinen** gepflegten ASC-MHL-Crate. Gefunden wurden:
- [sakerk/MASH](https://github.com/sakerk/MASH) (Rust, Apache-2.0): ein Hash-Werkzeug mit MHL-v2-Writer. **Nicht kompatibel mit der Referenz:** C4 ohne Auffüllen, structure-Hash nur über Namen, content-Hash über Hex-Text statt Bytes. Nur als Anschauung brauchbar.
- [OffloadKit / check-file](https://github.com/tranvietthang94-jpg/check-file) (Tauri 2 + Rust): hat einen eigenen MHL-Teil mit korrekt aufgefülltem C4 und Tests mit den offiziellen C4-Testwerten. **Ohne Lizenz**, also nur lesen.

→ **Selbst schreiben** (etwa 400–600 Zeilen mit `quick-xml`) und gegen die Referenz testen.

### Validieren

```
pip install ascmhl                                   # bringt ascmhl und ascmhl-debug mit
ascmhl-debug verify <Reel>/                          # Exit 0 = ok, getestet: 11 bei Hash-Abweichung
ascmhl-debug xsd-schema-check <Reel>/ascmhl/0001_….mhl        # nur aus einem Ordner mit xsd/-Unterordner (z. B. im mhl-Repo)
ascmhl-debug xsd-schema-check -df <Reel>/ascmhl/ascmhl_chain.xml
ascmhl info -v <Reel>/ascmhl/
```

`verify` gehört zu **`ascmhl-debug`**, nicht zu `ascmhl` (die README ist an der Stelle missverständlich). Zusätzlich lohnt ein Kreuztest: `ascmhl create` auf eine Kopie desselben Baums laufen lassen. Root- und Ordner-Hashes müssen dann Byte für Byte mit unseren übereinstimmen. Das gehört als Schritt in die CI (Python in GitHub Actions).

---

## 2. Sluice ([AndAy224/sluice](https://github.com/AndAy224/sluice), MIT, Rust)

- **Nur Windows** (egui statt Tauri). Hash ist **xxh64** (`xxhash-rust` mit Feature `xxh64`). Weitere Crates: `crossbeam-channel`, `quick-xml`, `walkdir`, `filetime`, `chrono`, `windows-sys`.
- **Kopieren** (`src/engine/copy.rs`): ein Lese-Thread liest die Karte in 4-MiB-Stücken ohne Puffer. Jedes Stück geht als `Arc<Chunk>` an je einen Schreib-Thread pro Ziel, über einen **begrenzten Kanal (4 Stücke)**. Das langsamste Ziel bremst den Leser, der Speicher bleibt klein, und die Oberfläche zeigt an, welches Ziel gerade bremst. Gehasht wird nebenbei beim Lesen.
- **Flush:** `FlushFileBuffers` pro Datei direkt beim Schreiben halbiert den Durchsatz (gemessen 44 → 81 MB/s). Deshalb übernimmt ein eigener Sync-Thread pro Ziel den Flush **nach** dem Schreiben. Das mtime wird erst gesetzt, wenn alles auf der Platte ist, weil „Fortsetzen“ dem mtime vertraut.
- **Prüfen** (`verify.rs`, `unbuffered.rs`): Jedes Ziel **und die Karte noch einmal** werden mit `FILE_FLAG_NO_BUFFERING` gelesen, mit 4096-Byte-ausgerichtetem Puffer (`AlignedBuf` über `std::alloc`). Ein kurzer Lesevorgang mitten in der Datei gilt als harter Fehler. `diagnose()` ordnet jede Kombination abweichender Hashes einer Ursache zu (Karte, Leser, Ziel, systematisch).
- **Urteil** (`verdict.rs`): fünf Stufen. SAFE TO FORMAT · VERIFIED-ONE SOURCE · VERIFIED-ONE COPY · VERIFIED-DO NOT FORMAT · FAILED. **Nur eine Funktion** (`authorises_erase`) darf „formatieren“ erlauben. Die Stufe ist zugleich der Exit-Code.
- **Physische Platte** (`win.rs`): Volume → `IOCTL_STORAGE_GET_DEVICE_NUMBER` → Plattennummer. Ohne Nummer kann **nur Gleichheit** bewiesen werden, keine Verschiedenheit, also gilt dann „unbewiesen = gleich“. Die Erkennung läuft über ein Trait `DeviceProbe`, das ausschliesslich im Test ersetzt werden kann (kein Schalter in der ausgelieferten App). Netzlaufwerke (`DRIVE_REMOTE`) erreichen nie SAFE TO FORMAT, weil `NO_BUFFERING` über SMB nur ein Hinweis ist.
- **MHL:** schreibt **MHL v1**, nicht ASC MHL v2. Liest beide. Das MHL wird erst nach bestandener Prüfung geschrieben, sein Vorhandensein ist also selbst das Erfolgszeichen.
- Weitere gute Vorab-Prüfungen: Zielordner gehört schon einem anderen Lauf, Dateinamen, die nur in Gross/Klein abweichen, OneDrive-Platzhalter, Schreibtest, gleiche Karte zweimal, Messung der Schreibrate (32 MiB).

**Was wir unter MIT übernehmen dürfen:** alles, solange der MIT-Hinweis („Copyright (c) 2026 the sluice authors“) in unseren Lizenzhinweisen steht. Sinnvoll: `unbuffered.rs` (AlignedBuf/ChunkReader), die Windows-Helfer in `win.rs` (Plattennummer, Laufwerkstyp, Cloud-Platzhalter, Schreibtest), das Muster aus `verdict.rs` und `distinctness()`. Den Pipeline-Aufbau aus `copy.rs` nehmen wir als Vorlage. Nicht brauchbar: das MHL (v1, xxh64) und alles für macOS (dort fehlt die Cache-Umgehung ganz).

---

## 3. o/COPY ([ottomatic-io/ocopy](https://github.com/ottomatic-io/ocopy), LGPL-3.0, Python). Nur Ideen, kein Code

- Pro Datei: ein Leser, ein Schreib-Thread pro Ziel über eine begrenzte Queue (10). Stirbt ein Schreiber, wird der Fehler weitergereicht statt zu hängen (Abfrage alle 0,5 s, Abbruch-Event).
- Geschrieben wird in `*.copy_in_progress` und erst nach der Prüfung umbenannt. So bleibt nie eine Datei mit richtigem Namen und falschem Inhalt liegen.
- Prüfen: Quelle und alle Ziele parallel neu hashen, alles muss gleich sein. Bei Abweichung **einmal reparieren** (neu kopieren), beim zweiten Fehlschlag abbrechen. **Keine Cache-Umgehung**, das Zurücklesen kommt also meist aus dem RAM.
- Liegen auf der Quelle schon Hashes (ASC MHL), werden sie mit geprüft.
- Checkpoint-Datei pro Ziel zum Fortsetzen. ASC MHL über die Referenzbibliothek mit `process=transfer` und bereits berechneten Hashes.

---

## 4. Weitere Vorbilder (kurz)

- **Offloader** ([owenpkent/offloader](https://github.com/owenpkent/offloader), MIT, Python): Bericht im Layout von ShotPut Pro als PDF (reportlab), ASC MHL. Cache-Umgehung „billig“: Datei kurz mit `NO_BUFFERING` öffnen, das wirft sie unter Windows aus dem Cache, danach normal lesen. Auf POSIX `posix_fadvise(DONTNEED)`. In `docs/data-safety.md` steht ehrlich, was ein „verified“ beweist und was nicht.
- **BitMatch** ([BitmatchApp/Bitmatch](https://github.com/BitmatchApp/Bitmatch), MIT, Swift, nur Mac): schreibt **und** liest mit `F_NOCACHE`. Ein Test prüft mit `mmap` + `mincore`, dass nach dem Kopieren weniger als 2 % der Datei im RAM liegen (das Muster für unseren eigenen Test). Unabhängige Kopien zählt es über DiskArbitration und das IORegistry bis zum physischen Speicher, denn ein APFS-Container ist eine virtuelle Platte (`BackupIndependencePolicy.swift`). Achtung: SHA-256 steht nicht im ASC-MHL-XSD, BitMatch schreibt deshalb MD5 ins MHL.
- **OffloadKit** ([tranvietthang94-jpg/check-file](https://github.com/tranvietthang94-jpg/check-file), **keine Lizenz**, Tauri 2 + React + Rust): unser engster Verwandter beim Stack (`xxhash-rust` xxh3, `md-5`, `quick-xml`, `sysinfo`). Nützlich als Ideen: Kaskaden-Modus, Reparatur-Planer (ersetzt eine Datei nur bei vollem Hash-Treffer gegen das MHL, die alte bleibt als Beweis), Schreibtest, Auto-Eject erst nach sauberem Abschluss. Keine Cache-Umgehung. Code nicht übernehmen.

---

## 5. Rust-Technik: Cache umgehen und Platten erkennen

### Zurücklesen am Cache vorbei

**macOS**
- `fcntl(fd, F_NOCACHE, 1)` (Crate `libc`) gilt nur pro File-Deskriptor. **Wichtig:** Liegen Seiten der Datei schon im Unified Buffer Cache (weil wir sie eben gecacht geschrieben haben), kann ein Lesen mit `F_NOCACHE` trotzdem aus dem Cache kommen ([Diskussion PostgreSQL](https://www.postgresql.org/message-id/CA+hUKG+ADiyyHe0cun2wfT+SVnFVqNYPxoO6J9zcZkVO7+NGig@mail.gmail.com), [XNU vfs_cluster.c](https://opensource.apple.com/source/xnu/xnu-7195.60.75/bsd/vfs/vfs_cluster.c.auto.html)).
- Deshalb wie BitMatch: **schon beim Schreiben `F_NOCACHE` setzen**, nach dem Schreiben `fcntl(fd, F_FULLFSYNC)` (nur `fsync` reicht auf dem Mac nicht, um den Platten-Cache zu leeren), danach mit neuem Deskriptor und `F_NOCACHE` zurücklesen. Ausgerichtete Puffer (4 KiB, Stücke von 1–8 MiB) halten den Lesevorgang auf dem direkten Weg.
- `F_GLOBAL_NOCACHE` setzt „kein Cache“ für die ganze Datei, über alle Deskriptoren. Nur als Ergänzung. `purge` braucht Root, scheidet also aus.
- Selbsttest in der CI und als Diagnose in der App: nach dem Kopieren `mmap` + `mincore` auf die Zieldatei. Bleibt die Residenz bei etwa 0 %, kommt das Zurücklesen nicht aus dem RAM.

**Windows**
- `CreateFileW` mit `FILE_FLAG_NO_BUFFERING` (in Rust über `OpenOptionsExt::custom_flags(0x2000_0000)`). Regeln laut [Microsoft „File Buffering“](https://learn.microsoft.com/en-us/windows/win32/fileio/file-buffering): Lese**länge** und **Offset** sind ein Vielfaches der Sektorgrösse, die **Pufferadresse** ist auf den Sektor ausgerichtet. Einfach immer 4096 nehmen (deckt 512e und 4Kn ab). Wer es genau will, fragt die physische Sektorgrösse über `IOCTL_STORAGE_QUERY_PROPERTY` → `STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR` ab. Das letzte Stück fordert eine volle Sektorlänge an und wird auf die Dateilänge gekürzt. `BufReader`/`read_exact` nicht verwenden, sie bringen ihren eigenen, falsch ausgerichteten Puffer mit.
- Das Öffnen mit `NO_BUFFERING` leert und verwirft auch gecachte Seiten dieser Datei. Schreiben: normal gepuffert, danach `FlushFileBuffers` (Sluice: gesammelt auf einem Sync-Thread). `FILE_FLAG_WRITE_THROUGH` ist nicht nötig, wenn danach geflusht wird, und bremst vermutlich ähnlich wie ein Flush pro Datei (Sluice hat gemessen, dass der Flush pro Datei den Durchsatz etwa halbiert; WRITE_THROUGH selbst ist nicht gemessen). Erklärung bei [The Old New Thing](https://devblogs.microsoft.com/oldnewthing/20210729-00/?p=105494).
- Crates: `windows-sys` oder `windows` (Features `Win32_Storage_FileSystem`, `Win32_System_Ioctl`, `Win32_System_IO`). Ausgerichteter Puffer selbst über `std::alloc::Layout` (wie Sluice) oder mit dem Crate `aligned-vec`.

**Grenze (gilt überall):** Einen flüchtigen Cache in der Platte oder im RAID-Controller kann keine App umgehen. Die Formulierung im Bericht heisst deshalb „vom Gerät gelesen, OS-Cache umgangen“, nicht „vom Platter“.

### Seriennummer und physische Platte zu einem Pfad

**Grundsatz (von Sluice):** Für „verschiedene Platten **jetzt**“ zählt die aktuelle physische Geräte-Identität (Windows: Plattennummer, Mac: die ganze physische Platte hinter dem Volume). Die **Seriennummer** dient dem Wiedererkennen über Sitzungen hinweg und dem Bericht. Billige USB-Gehäuse liefern oft keine oder dieselbe Fantasie-Seriennummer (`000000…`). Darum gilt: dieselbe Seriennummer heisst „nicht unabhängig“, eine fehlende Seriennummer heisst „nur über die Geräte-Identität entscheiden“, und ist beides unklar, gilt die Kopie als **nicht bewiesen**.

**macOS**
1. `DADiskCreateFromVolumePath` → `DADiskCopyWholeDisk` → BSD-Name (`diskN`). Crates: `objc2-disk-arbitration` und `objc2-io-kit` (oder `io-kit-sys`).
2. **APFS:** `diskN` ist oft ein synthetischer Container. Deshalb im IORegistry vom Medium über `IORegistryEntryGetParentEntry` nach oben gehen, bis zum physischen Speicher (so macht es BitMatch). Alternative per CLI: `diskutil info -plist /Volumes/X` → `APFSPhysicalStores` und `ParentWholeDisk`.
3. Seriennummer auf demselben Weg nach oben: bei NVMe/SATA in `Device Characteristics` → `Serial Number`, bei USB-Geräten `USB Serial Number` bzw. `kUSBSerialNumberString`. `diskutil info` liefert **keine** Seriennummer. Als Ausweg `system_profiler SPUSBDataType SPNVMeDataType SPSerialATADataType -json` (langsam, nur als Rückfall).
4. Zusätzlich speichern: `kDADiskDescriptionDeviceModelKey`, Grösse, `VolumeUUID`.

**Windows**
1. `GetVolumePathNameW` → `GetVolumeNameForVolumeMountPointW` → `\\.\C:` mit Zugriff 0 öffnen → `IOCTL_STORAGE_GET_DEVICE_NUMBER` (Plattennummer N). Bei Volumes über mehrere Platten besser `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS`. Kein Admin nötig.
2. `\\.\PhysicalDriveN` öffnen → `IOCTL_STORAGE_QUERY_PROPERTY` mit `StorageDeviceProperty` → `STORAGE_DEVICE_DESCRIPTOR` → `SerialNumberOffset`, `VendorIdOffset`, `ProductIdOffset`, `BusType`. Manche USB-Brücken liefern die Seriennummer als Hex-ASCII oder gar nicht.
3. `GetVolumeInformationW` liefert nur die **Volume**-Seriennummer, nicht die der Platte (zwei Partitionen einer Platte haben verschiedene).

**Netzlaufwerke (SMB-NAS)**
- Erkennen: Windows `GetDriveTypeW == DRIVE_REMOTE` oder UNC-Pfad. macOS `statfs`: `MNT_LOCAL` fehlt bzw. `f_fstypename` ist `smbfs`/`nfs`/`afpfs`, oder DiskArbitration `kDADiskDescriptionVolumeNetworkKey`.
- Identität = **Server + Freigabe** (macOS `f_mntfromname` `//user@server/share`, Windows `WNetGetUniversalNameW`). Verschiedene Freigaben auf demselben Server gelten als **dasselbe Gerät**. Server-Namen normalisieren (Hostname vs. IP), sonst ist nur Gleichheit beweisbar, keine Verschiedenheit.
- Zurücklesen über SMB umgeht höchstens den Client-Cache, nie den Cache des NAS (Windows: `NO_BUFFERING` über SMB ist nur ein Hinweis; macOS: `F_NOCACHE` auf `smbfs` nicht verlässlich belegt).
- **Vorschlag:** Ein NAS zählt als **eine** unabhängige Kopie (anderes Gerät, anderer Ort), im Bericht markiert mit „über Netzwerk geprüft, NAS-Cache nicht ausgeschlossen“. Ob das für „sicher zum Formatieren“ reicht, entscheidet die Einstellung N (siehe Empfehlung).

---

## 6. XXH3-128 in Rust

- **Empfehlung: `xxhash-rust`** (0.8.19, BSL-1.0, Feature `xxh3`). Pure Rust mit SIMD (SSE2/AVX2/NEON), wird von Sluice, MASH und OffloadKit genutzt. `twox-hash` 2.x kann XXH3-128 inzwischen auch, beide gehen.
- Streaming: `let mut h = xxhash_rust::xxh3::Xxh3Default::new(); h.update(&buf[..n]); let v: u128 = h.digest128();` (bzw. `Xxh3::new()`). Ausgabe: `format!("{:032x}", v)` ergibt Big Endian, Kleinbuchstaben, wie ASC MHL es verlangt. Testwert: `"abc"` → `06b05ab6733a618578af5f94892f3950`.
- MD5 parallel im selben Lese-Durchgang: Crate `md-5` (RustCrypto). MD5 ist der Flaschenhals (~0,6–0,8 GB/s pro Kern). Deshalb bleibt es abschaltbar, und xxh3 und md5 laufen auf getrennten Threads über dasselbe `Arc<Chunk>`.
- C4 für die Chain: `sha2` (SHA-512) plus eigene Base58-Kodierung mit Auffüllen auf 88 Zeichen (siehe Abschnitt 1, **nicht** einfach `bs58`).

---

## 7. PDF-Bericht aus Rust

| Crate | Einschätzung |
|---|---|
| **`typst` + `typst-pdf`** (0.15), bequem über **`typst-as-lib`** | **Empfehlung.** Der Bericht wird als Typst-Vorlage geschrieben (Tabellen, Seitenumbruch, Kopf/Fuss, Seitenzahlen, Schriften eingebettet), die Daten kommen als JSON hinein. Gut lesbare Vorlage, die ohne Rust-Änderung angepasst werden kann. Nachteil: grössere Binärdatei (mehrere MB) und Schriften müssen mitgeliefert werden. |
| `krilla` (0.8) | Die PDF-Schicht unter Typst, leistungsfähig, aber man setzt selbst (kein Layout). |
| `printpdf` (0.12) | Aktiv gepflegt, niedriges Niveau. Für einen mehrseitigen Tabellenbericht viel Handarbeit. |
| `genpdf` (0.2) | Seit 2021 nicht mehr gepflegt, nicht verwenden. |

Den Bericht über die Tauri-WebView drucken zu lassen, geht nicht ohne Benutzer-Dialog und nicht einheitlich auf Mac und Windows, ist also keine Option für einen Bericht, der automatisch auf jedes Ziel geschrieben wird.

---

## So bauen wir es

1. **Pipeline wie Sluice, plattformübergreifend:** ein Lese-Thread pro Karte (ausgerichtete 4-MiB-Puffer, ohne Cache), Stücke als `Arc<Chunk>` über begrenzte `crossbeam-channel`-Kanäle (Tiefe ~4) an einen Schreib-Thread pro Ziel. xxh3-128 (und optional md5) werden auf eigenen Threads aus denselben Stücken gerechnet. Die Oberfläche zeigt, welches Ziel bremst. Threads statt async.
2. **Schreiben sicher:** in `*.part` schreiben, auf dem Mac mit `F_NOCACHE`. Flush gesammelt auf einem Sync-Thread pro Ziel (`FlushFileBuffers` / `F_FULLFSYNC`). Erst nach bestandener Prüfung umbenennen und mtime setzen. Ein Abbruch hinterlässt so nie eine Datei mit richtigem Namen.
3. **Prüfen:** jedes Ziel **komplett** am OS-Cache vorbei zurücklesen (Windows `NO_BUFFERING` mit 4096-Ausrichtung, Mac `F_NOCACHE` plus Schreiben ohne Cache). Vergleich mit dem Hash von der Karte. Optional wie Sluice die Karte ein zweites Mal ungepuffert lesen, um einen unzuverlässigen Kartenleser zu erkennen (Einstellung, Standard aus). Ein `mincore`-Selbsttest belegt in der CI, dass der Mac nicht aus dem RAM liest.
4. **ASC MHL selbst schreiben** (`quick-xml`, `sha2`, eigenes C4): pro Ziel unter `01_KAMERA/<Reel>/ascmhl/` Generation `0001_<Reel>_<UTC>Z.mhl` mit `process=transfer`, xxh128 (+ md5), Ordner- und Root-Hashes und `ascmhl_chain.xml`. Erst nach bestandener Prüfung schreiben, atomar (temporäre Datei, dann umbenennen). Hat die Karte schon ein `ascmhl/`, wird es mitgenommen und eine Generation mit `verified`/`failed` angehängt.
5. **Kompatibilität automatisch beweisen:** CI-Job (GitHub Actions, Python) erzeugt einen Test-Baum, lässt unseren Kern kopieren, danach `ascmhl-debug verify` und `xsd-schema-check`, dazu `ascmhl create` auf eine Kopie. Root- und Ordner-Hashes müssen gleich sein. Unit-Tests mit festen Werten (xxh128 „abc“, offizielle C4-Testwerte).
6. **Geräte-Identität als eigenes Modul mit Trait** (wie Sluice `DeviceProbe`, nur im Test ersetzbar): Windows Plattennummer + `IOCTL_STORAGE_QUERY_PROPERTY`-Seriennummer, Mac DiskArbitration + IORegistry bis zum physischen Speicher + Seriennummer, Netzwerk = Server+Freigabe. Ergebnis pro Ziel: `Distinct` / `SameDevice` / `Unproven`. Unbewiesen zählt wie gleich.
7. **Freigabe-Urteil an einer einzigen Stelle** (`authorises_erase`), mit Tests über alle Zustände: „sicher zum Formatieren“ nur, wenn alle Dateien auf ≥ N Zielen bestätigt sind und diese N Ziele paarweise `Distinct` sind. Darunter abgestufte, ehrliche „Nein“-Stufen wie bei Sluice (eine Kopie / gleiche Platte / unbewiesen / fehlgeschlagen). Seriennummern und Plattenmodelle stehen im Bericht.
8. **Vorab-Prüfungen vor dem Start:** Schreibtest und gemessene Schreibrate pro Ziel, genug Platz, Ziel nicht auf der Karte, zwei Ziele auf derselben Platte (Warnung schon in der Auswahl), Dateinamen, die nur in Gross/Klein abweichen (wichtig für Mac → exFAT/NTFS), Zielordner gehört schon einem anderen Lauf.
9. **PDF-Bericht mit Typst** (`typst-as-lib`, Vorlage im Repo, Schrift eingebettet). Inhalt: Karte, Ziele mit Seriennummern, Datei-Liste mit xxh128, Urteil, Version und Commit der App. Er wird zusammen mit dem MHL auf jedes Ziel geschrieben.
10. **Code-Herkunft:** Teile aus Sluice (MIT) dürfen wir übernehmen, mit Hinweis in `THIRD_PARTY_NOTICES`. o/COPY (LGPL) und OffloadKit (ohne Lizenz) nur als Ideen, kein Code.

## Offen / unsicher

- **macOS-Cache:** Schliesst `F_NOCACHE` beim Schreiben **und** Lesen jeden Cache-Treffer aus? Bei BitMatch per `mincore` belegt, bei uns noch nicht. Muss auf echter Hardware (USB-SSD, exFAT und APFS) gemessen werden.
- **NAS:** Ob ein SMB-Ziel für die Freigabe als unabhängige Kopie zählt, ist eine **Entscheidung, keine Technikfrage**. Den Cache des NAS umgeht man nicht. Möglicher späterer Ausweg: den Hash auf dem NAS selbst berechnen lassen.
- **Seriennummern über USB:** wie zuverlässig mit den eigenen Gehäusen und Kartenlesern (manche Doppel-Leser zeigen zwei Karten als ein Gerät). Auf echter Hardware prüfen.
- **ASC-MHL-Feinheiten:** Ein Lauf mit einem fehlgeschlagenen Ziel: gar kein MHL schreiben (wie Sluice) oder eine Generation mit `failed`? Vorschlag: auf einem fehlerhaften Ziel kein MHL, der Bericht nennt den Fehler.
- **Typst:** Grösse der Binärdatei und Startzeit in Tauri noch nicht gemessen.
