//! Welches Gerät steckt hinter einem Ziel?
//!
//! Zwei Ziele auf derselben Platte zählen als **eine** Kopie (`docs/KONZEPT.md`, Kapitel 4).
//! Grundsatz (Recherche, nach Sluice): Für „verschiedene Platten“ zählt die aktuelle Identität
//! der physischen Platte; die Seriennummer steht im Bericht. Ist die Identität nicht bewiesen,
//! ist [`Kennung::sicher`] `false`, und die Freigabe zählt alle unbewiesenen Ziele zusammen als eins.
//! - macOS: `diskutil info` bis zur ganzen physischen Platte (auch hinter einem APFS-Container).
//! - Windows: Plattennummer über `IOCTL_STORAGE_GET_DEVICE_NUMBER`, Seriennummer über `IOCTL_STORAGE_QUERY_PROPERTY`.
//! - Netz (SMB/NFS): Server; verschiedene Freigaben auf demselben Server sind dasselbe Gerät.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Art {
    Platte,
    Netz,
    /// Nur das Volume ist bekannt (z. B. Linux, oder die Abfrage ist gescheitert).
    Volume,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kennung {
    /// Vergleichswert: gleiche Werte = dasselbe Gerät.
    pub wert: String,
    /// `true`, wenn `wert` das physische Gerät (bzw. den Server) sicher bezeichnet.
    pub sicher: bool,
    pub art: Art,
    pub seriennummer: Option<String>,
    /// Modell oder Server, für Menschen und den Bericht.
    pub beschreibung: String,
}

/// Bestimmt die Kennung des Geräts, auf dem `pfad` liegt. `pfad` muss existieren.
pub fn kennung(pfad: &Path) -> std::io::Result<Kennung> {
    plattform::kennung(pfad)
}

fn volume(pfad: &Path, grund: &str) -> std::io::Result<Kennung> {
    #[cfg(unix)]
    let wert = {
        use std::os::unix::fs::MetadataExt;
        format!("volume:{}", std::fs::metadata(pfad)?.dev())
    };
    #[cfg(windows)]
    let wert = format!(
        "volume:{}",
        std::fs::canonicalize(pfad)?
            .components()
            .next()
            .map(|k| k.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default()
    );
    Ok(Kennung { wert, sicher: false, art: Art::Volume, seriennummer: None, beschreibung: grund.into() })
}

#[cfg_attr(all(unix, not(target_os = "macos")), allow(dead_code))]
fn netz(server: &str, quelle: &str) -> Kennung {
    let server = server.trim_start_matches('/').trim_start_matches('\\');
    let server = server.rsplit('@').next().unwrap_or(server); // Benutzer weglassen
    let server = server.split(['/', '\\']).next().unwrap_or(server).to_lowercase();
    let server = server.split(':').next().unwrap_or(&server).to_owned(); // NFS „host:/pfad“
                                                                         // Derselbe NAS unter zwei Namen (Name, IP, Bonjour) darf nicht als zwei Geräte zählen: nach IP vergleichen.
    use std::net::ToSocketAddrs;
    let ip = (server.as_str(), 445).to_socket_addrs().ok().and_then(|mut a| a.next()).map(|a| a.ip());
    // Eine Freigabe auf den eigenen Rechner ist kein eigenes Gerät.
    let lokal = ip.is_some_and(|ip| ip.is_loopback()) || server == "localhost";
    Kennung {
        wert: format!("netz:{}", ip.map(|i| i.to_string()).unwrap_or(server.clone())),
        sicher: !lokal,
        art: Art::Netz,
        seriennummer: None,
        beschreibung: format!("Netzlaufwerk {quelle}"),
    }
}

/// Ist `pfad` die Wurzel eines Volumes (die ganze Karte) und nicht nur ein Ordner darauf?
pub fn ist_volume_wurzel(pfad: &Path) -> std::io::Result<bool> {
    let p = std::fs::canonicalize(pfad)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let Some(eltern) = p.parent() else { return Ok(true) };
        Ok(std::fs::metadata(&p)?.dev() != std::fs::metadata(eltern)?.dev())
    }
    #[cfg(windows)]
    {
        Ok(plattform::volume_wurzel(&p).is_some_and(|w| {
            w.trim_end_matches('\\')
                .eq_ignore_ascii_case(p.to_string_lossy().trim_start_matches(r"\\?\").trim_end_matches('\\'))
        }))
    }
}

/// Name der Karte für den Zielordner: Ordnername, bei einer Windows-Laufwerkswurzel der Volume-Name.
pub fn kartenname(pfad: &Path) -> String {
    let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !name.is_empty() && !name.contains(':') {
        return name;
    }
    #[cfg(windows)]
    if let Some(n) = plattform::volume_name(pfad) {
        return n;
    }
    "Karte".into()
}

/// Kennung des Volumes, auf dem `pfad` liegt (zum Vergleich mit der Karte).
pub fn volume_kennung(pfad: &Path) -> std::io::Result<String> {
    Ok(volume(pfad, "")?.wert)
}

#[cfg(target_os = "macos")]
mod plattform {
    use super::*;
    use std::ffi::CStr;
    use std::process::Command;

    pub fn kennung(pfad: &Path) -> std::io::Result<Kennung> {
        let (typ, von, auf) = statfs(pfad)?;
        if matches!(typ.as_str(), "smbfs" | "nfs" | "afpfs" | "webdav") {
            return Ok(netz(&von, &von));
        }
        match ganze_platte(&auf) {
            Some((platte, modell)) => Ok(Kennung {
                seriennummer: seriennummer(&platte),
                wert: format!("platte:{platte}"),
                sicher: true,
                art: Art::Platte,
                beschreibung: modell,
            }),
            None => volume(pfad, "Platte nicht bestimmbar"),
        }
    }

    fn statfs(pfad: &Path) -> std::io::Result<(String, String, String)> {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(pfad.as_os_str().as_bytes())?;
        let mut s: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: gültiger C-String und Zielstruktur.
        if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let text = |f: &[libc::c_char]| unsafe { CStr::from_ptr(f.as_ptr()) }.to_string_lossy().into_owned();
        Ok((text(&s.f_fstypename), text(&s.f_mntfromname), text(&s.f_mntonname)))
    }

    fn info(ziel: &str) -> Option<plist::Dictionary> {
        let aus = Command::new("/usr/sbin/diskutil").args(["info", "-plist", ziel]).output().ok()?;
        if !aus.status.success() {
            return None;
        }
        plist::from_bytes::<plist::Dictionary>(&aus.stdout).ok()
    }

    /// Seriennummer der Platte `diskN` aus `system_profiler` (USB, NVMe, SATA). Nur für Bericht und
    /// Wiedererkennen; über „verschiedene Platten“ entscheidet der BSD-Name.
    fn seriennummer(platte: &str) -> Option<String> {
        let aus = Command::new("/usr/sbin/system_profiler")
            .args(["-json", "SPUSBDataType", "SPUSBHostDataType", "SPNVMeDataType", "SPSerialATADataType"])
            .output()
            .ok()?;
        let wurzel: serde_json::Value = serde_json::from_slice(&aus.stdout).ok()?;
        suchen(&wurzel, platte, None)
    }

    /// Sucht den Knoten mit `bsd_name == platte` und nimmt die nächste Seriennummer auf dem Weg dorthin.
    pub(super) fn suchen(knoten: &serde_json::Value, platte: &str, oben: Option<String>) -> Option<String> {
        use serde_json::Value;
        match knoten {
            Value::Object(m) => {
                let eigene = ["serial_num", "device_serial", "USBDeviceKeySerialNumber"]
                    .iter()
                    .filter_map(|k| m.get(*k).and_then(Value::as_str))
                    .map(str::trim)
                    .find(|s| brauchbar(s))
                    .map(str::to_owned)
                    .or(oben);
                if m.get("bsd_name").and_then(Value::as_str) == Some(platte) {
                    return eigene;
                }
                m.values().find_map(|v| suchen(v, platte, eigene.clone()))
            }
            Value::Array(a) => a.iter().find_map(|v| suchen(v, platte, oben.clone())),
            _ => None,
        }
    }

    /// Leere und Fantasie-Seriennummern billiger Gehäuse (`000000…`, `0123456789…`) verwerfen.
    fn brauchbar(s: &str) -> bool {
        let ziffern: Vec<char> = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        ziffern.len() >= 4 && !ziffern.iter().all(|&c| c == '0') && !s.starts_with("0123456789")
    }

    /// Ganze physische Platte (BSD-Name) und Modell zu einem Einhängepunkt.
    fn ganze_platte(einhaengepunkt: &str) -> Option<(String, String)> {
        let v = info(einhaengepunkt)?;
        // APFS: das Volume liegt in einem synthetischen Container; die physische Ablage steht in APFSPhysicalStores.
        let speicher = v
            .get("APFSPhysicalStores")
            .and_then(|s| s.as_array())
            .and_then(|a| a.first())
            .and_then(|e| e.as_dictionary().and_then(|d| d.get("APFSPhysicalStore")).or(Some(e)))
            .and_then(|s| s.as_string())
            .map(str::to_owned);
        let teil =
            speicher.unwrap_or_else(|| v.get("DeviceIdentifier").and_then(|s| s.as_string()).unwrap_or("").to_owned());
        let p = info(&teil)?;
        let platte = p.get("ParentWholeDisk").and_then(|s| s.as_string())?.to_owned();
        let ganz = info(&platte)?;
        // Disk-Images (DMG, Sparsebundle) liegen auf einer anderen Platte: nie als eigenes Gerät zählen.
        let virtuell = ganz.get("VirtualOrPhysical").and_then(|s| s.as_string()) == Some("Virtual")
            || ganz.get("BusProtocol").and_then(|s| s.as_string()) == Some("Disk Image");
        if virtuell {
            return None;
        }
        let modell = ganz.get("MediaName").and_then(|s| s.as_string()).unwrap_or_default().to_owned();
        Some((platte, modell))
    }
}

#[cfg(windows)]
mod plattform {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::NetworkManagement::WNet::WNetGetConnectionW;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetDriveTypeW, GetVolumePathNameW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    const DRIVE_REMOTE: u32 = 4;
    const IOCTL_STORAGE_GET_DEVICE_NUMBER: u32 = 0x002D_1080;
    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;

    fn breit(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn aus_breit(b: &[u16]) -> String {
        String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())])
    }

    struct Griff(HANDLE);
    impl Drop for Griff {
        fn drop(&mut self) {
            // SAFETY: gültiger, von CreateFileW geöffneter Griff.
            unsafe { CloseHandle(self.0) };
        }
    }

    fn oeffnen(geraet: &str) -> Option<Griff> {
        let name = breit(std::ffi::OsStr::new(geraet));
        // SAFETY: Zugriff 0 genügt für die Abfragen, keine Adminrechte nötig.
        let h = unsafe {
            CreateFileW(
                name.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        (h != INVALID_HANDLE_VALUE).then_some(Griff(h))
    }

    fn ioctl(g: &Griff, code: u32, ein: &[u8], aus: &mut [u8]) -> Option<usize> {
        let mut n = 0u32;
        // SAFETY: Puffer gültig für die angegebenen Längen.
        let ok = unsafe {
            DeviceIoControl(
                g.0,
                code,
                ein.as_ptr().cast(),
                ein.len() as u32,
                aus.as_mut_ptr().cast(),
                aus.len() as u32,
                &mut n,
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(n as usize)
    }

    /// Wurzel des Volumes (z. B. `E:\`) zu einem Pfad.
    pub fn volume_wurzel(pfad: &Path) -> Option<String> {
        let mut wurzel = [0u16; 261];
        let p = breit(pfad.as_os_str());
        // SAFETY: Puffer mit angegebener Länge.
        (unsafe { GetVolumePathNameW(p.as_ptr(), wurzel.as_mut_ptr(), wurzel.len() as u32) } != 0)
            .then(|| aus_breit(&wurzel).trim_start_matches(r"\\?\").to_owned())
    }

    /// Volume-Name (Bezeichnung) einer Laufwerkswurzel, z. B. „A001R132“.
    pub fn volume_name(pfad: &Path) -> Option<String> {
        use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
        let wurzel = volume_wurzel(&std::fs::canonicalize(pfad).ok()?)?;
        let w = breit(std::ffi::OsStr::new(&wurzel));
        let mut name = [0u16; 261];
        // SAFETY: Puffer mit angegebener Länge, übrige Ausgaben nicht gebraucht.
        let ok = unsafe {
            GetVolumeInformationW(
                w.as_ptr(),
                name.as_mut_ptr(),
                name.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        let n = aus_breit(&name);
        (ok != 0 && !n.trim().is_empty()).then_some(n)
    }

    pub fn kennung(pfad: &Path) -> std::io::Result<Kennung> {
        let voll = std::fs::canonicalize(pfad)?;
        let text = voll.to_string_lossy().trim_start_matches(r"\\?\").to_owned();
        if let Some(unc) = text.strip_prefix(r"UNC\") {
            return Ok(netz(unc, &format!(r"\\{unc}")));
        }
        let mut wurzel = [0u16; 261];
        let p = breit(voll.as_os_str());
        // SAFETY: Puffer mit angegebener Länge.
        if unsafe { GetVolumePathNameW(p.as_ptr(), wurzel.as_mut_ptr(), wurzel.len() as u32) } == 0 {
            return volume(pfad, "Volume nicht bestimmbar");
        }
        let wurzel = aus_breit(&wurzel);
        let wurzel = wurzel.trim_start_matches(r"\\?\").to_owned();
        let w = breit(std::ffi::OsStr::new(&wurzel));
        // SAFETY: nullterminierter Pfad.
        if unsafe { GetDriveTypeW(w.as_ptr()) } == DRIVE_REMOTE {
            let laufwerk = breit(std::ffi::OsStr::new(wurzel.trim_end_matches('\\')));
            let mut ziel = [0u16; 512];
            let mut laenge = ziel.len() as u32;
            // SAFETY: Puffer und Längenangabe passen zusammen.
            if unsafe { WNetGetConnectionW(laufwerk.as_ptr(), ziel.as_mut_ptr(), &mut laenge) } == 0 {
                let unc = aus_breit(&ziel);
                return Ok(netz(&unc, &unc));
            }
            return volume(pfad, "Netzlaufwerk ohne Server");
        }

        let Some(vol) = oeffnen(&format!(r"\\.\{}", wurzel.trim_end_matches('\\'))) else {
            return volume(pfad, "Volume nicht zu öffnen");
        };
        let mut nummer = [0u8; 12]; // STORAGE_DEVICE_NUMBER: DeviceType, DeviceNumber, PartitionNumber
        if ioctl(&vol, IOCTL_STORAGE_GET_DEVICE_NUMBER, &[], &mut nummer).is_none() {
            return volume(pfad, "Plattennummer nicht lesbar (Volume über mehrere Platten?)");
        }
        let platte = u32::from_le_bytes(nummer[4..8].try_into().expect("4 Bytes"));

        let (seriennummer, modell, bus) = oeffnen(&format!(r"\\.\PhysicalDrive{platte}"))
            .and_then(|g| {
                // STORAGE_PROPERTY_QUERY { PropertyId = StorageDeviceProperty (0), QueryType = Standard (0) }
                let anfrage = [0u8; 12];
                let mut aus = vec![0u8; 1024];
                let n = ioctl(&g, IOCTL_STORAGE_QUERY_PROPERTY, &anfrage, &mut aus)?;
                let feld = |off: usize| -> Option<String> {
                    let o = u32::from_le_bytes(aus.get(off..off + 4)?.try_into().ok()?) as usize;
                    if o == 0 || o >= n {
                        return None;
                    }
                    let ende = aus[o..n].iter().position(|&b| b == 0).map(|e| o + e).unwrap_or(n);
                    let s = String::from_utf8_lossy(&aus[o..ende]).trim().to_owned();
                    (!s.is_empty()).then_some(s)
                };
                // STORAGE_DEVICE_DESCRIPTOR: VendorIdOffset 12, ProductIdOffset 16, SerialNumberOffset 24, BusType 28
                let modell = [feld(12), feld(16)].into_iter().flatten().collect::<Vec<_>>().join(" ");
                let bus = u32::from_le_bytes(aus.get(28..32)?.try_into().ok()?);
                Some((feld(24), modell, bus))
            })
            .unwrap_or((None, String::new(), 0));
        // BusTypeVirtual (14), BusTypeFileBackedVirtual (15): VHD/VHDX liegen auf einer anderen Platte.
        let virtuell = bus == 14 || bus == 15;

        Ok(Kennung {
            wert: format!("platte:{platte}"),
            sicher: !virtuell,
            art: Art::Platte,
            seriennummer,
            beschreibung: modell,
        })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod plattform {
    use super::*;

    /// Linux ist keine Zielplattform; nur das Volume.
    pub fn kennung(pfad: &Path) -> std::io::Result<Kennung> {
        volume(pfad, "Linux: nur Volume")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netz_kennung_ohne_benutzer_und_freigabe() {
        assert_eq!(netz("//marlon@NAS.local/Footage", "x").wert, "netz:nas.local");
        assert_eq!(netz(r"\\NAS.local\Footage", "x").wert, "netz:nas.local");
        assert_eq!(netz(r"NAS.local\Andere", "x").wert, "netz:nas.local");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn seriennummer_aus_system_profiler() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"SPUSBDataType":[{"_items":[{"_name":"T7","serial_num":"S6XNNF0W123456",
                "Media":[{"bsd_name":"disk4","volumes":[{"bsd_name":"disk4s1"}]}]},
               {"_name":"Billig","serial_num":"000000000000","Media":[{"bsd_name":"disk5"}]}]}]}"#,
        )
        .unwrap();
        assert_eq!(plattform::suchen(&json, "disk4", None).as_deref(), Some("S6XNNF0W123456"));
        assert_eq!(plattform::suchen(&json, "disk5", None), None);
        assert_eq!(plattform::suchen(&json, "disk9", None), None);
    }

    #[test]
    fn eigener_ordner_hat_eine_kennung() {
        let k = kennung(&std::env::temp_dir()).unwrap();
        assert!(!k.wert.is_empty());
        // Auf Mac und Windows muss die physische Platte bestimmbar sein.
        if cfg!(any(target_os = "macos", windows)) {
            assert!(k.sicher, "{k:?}");
        }
    }
}
