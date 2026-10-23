//! 空蝉に 遠き声入れ 歌はしむ

use super::device::OutputDevice;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;

pub use platform::{LINK, PROVIDER, candidates, hides_input, install, prepare, release};

pub fn find(outputs: &[OutputDevice]) -> Option<OutputDevice> {
    candidates(outputs).into_iter().next()
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod cable {
    use anyhow::{Error, anyhow};

    use crate::audio::device::OutputDevice;

    pub const PROVIDER: &str = "VB-CABLE";
    pub const LINK: Option<&str> = Some("https://vb-audio.com/Cable/");

    pub fn outputs(outputs: &[OutputDevice], cable: impl Fn(&str) -> bool) -> Vec<OutputDevice> {
        let mut matches = outputs
            .iter()
            .filter(|device| cable(&device.name))
            .cloned()
            .collect::<Vec<_>>();
        matches.sort_by_key(|device| device.name.contains("16"));
        matches
    }

    pub fn failure(stderr: &[u8], cancelled: &[&str]) -> Error {
        let detail = String::from_utf8_lossy(stderr);
        if cancelled.iter().any(|marker| detail.contains(marker)) {
            anyhow!("已取消安装")
        } else if detail.contains("signature") {
            anyhow!("安装包签名无效")
        } else {
            anyhow!("VB-CABLE 安装失败")
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    use anyhow::{Context, Result};

    use super::cable;
    use crate::audio::device::OutputDevice;

    pub use super::cable::{LINK, PROVIDER};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$dir = Join-Path $env:TEMP 'locoSaonduh-vbcable'
if (Test-Path $dir) { Remove-Item $dir -Recurse -Force }
New-Item -ItemType Directory -Path $dir | Out-Null
$zip = Join-Path $dir 'cable.zip'
Invoke-WebRequest -Uri 'https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip' -OutFile $zip -UseBasicParsing
Expand-Archive -Path $zip -DestinationPath $dir -Force
$name = if ([Environment]::Is64BitOperatingSystem) { 'VBCABLE_Setup_x64.exe' } else { 'VBCABLE_Setup.exe' }
$setup = Join-Path $dir $name
$sig = Get-AuthenticodeSignature $setup
if ($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'BUREL VINCENT') { throw 'signature' }
try {
  Start-Process -FilePath $setup -ArgumentList '-i','-h' -Verb RunAs -Wait
} catch { throw 'cancelled' }
Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
"#;

    pub fn candidates(outputs: &[OutputDevice]) -> Vec<OutputDevice> {
        cable::outputs(outputs, |name| name.starts_with("CABLE In"))
    }

    pub fn hides_input(_id: &str, name: &str) -> bool {
        name.starts_with("CABLE Output")
    }

    pub fn install() -> Result<()> {
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                SCRIPT,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .context("PowerShell is not available")?;
        if output.status.success() {
            return Ok(());
        }
        Err(cable::failure(&output.stderr, &["cancelled"]))
    }

    pub fn prepare() -> Result<()> {
        Ok(())
    }

    pub fn release() {}
}

#[cfg(target_os = "macos")]
mod macos {
    use std::process::Command;

    use anyhow::{Context, Result};

    use super::cable;
    use crate::audio::device::OutputDevice;

    pub use super::cable::{LINK, PROVIDER};

    const SCRIPT: &str = r#"
set -e
dir=$(mktemp -d)
trap 'hdiutil detach "$dir/mnt" -quiet >/dev/null 2>&1 || true; rm -rf "$dir"' EXIT
curl -fsSL --retry 2 -o "$dir/cable.zip" 'https://download.vb-audio.com/Download_MAC/VBCable_MACDriver_Pack108.zip'
/usr/bin/ditto -x -k "$dir/cable.zip" "$dir/x"
dmg=$(find "$dir/x" -name '*Pack108*.dmg' | head -n 1)
mkdir "$dir/mnt"
hdiutil attach -nobrowse -readonly -quiet -mountpoint "$dir/mnt" "$dmg"
pkg=$(find "$dir/mnt" -maxdepth 2 -name '*.pkg' | head -n 1)
test -n "$pkg"
signature=$(pkgutil --check-signature "$pkg") || { echo signature >&2; exit 1; }
case "$signature" in
  *"Developer ID Installer: "*"(6K8JQXLBSY)"*) ;;
  *) echo signature >&2; exit 1 ;;
esac
cp "$pkg" "$dir/cable.pkg"
osascript -e "do shell script \"/usr/sbin/installer -pkg '$dir/cable.pkg' -target /\" with administrator privileges"
"#;

    pub fn candidates(outputs: &[OutputDevice]) -> Vec<OutputDevice> {
        cable::outputs(outputs, |name| name.contains("VB-Cable"))
    }

    pub fn hides_input(_id: &str, name: &str) -> bool {
        name.contains("VB-Cable")
    }

    pub fn install() -> Result<()> {
        let output = Command::new("/bin/sh")
            .args(["-c", SCRIPT])
            .output()
            .context("sh is not available")?;
        if output.status.success() {
            return Ok(());
        }
        Err(cable::failure(&output.stderr, &["-128", "canceled"]))
    }

    pub fn prepare() -> Result<()> {
        Ok(())
    }

    pub fn release() {}
}

#[cfg(target_os = "linux")]
mod linux {
    use anyhow::Result;

    use crate::audio::device::OutputDevice;
    use crate::audio::pulse::{self, MIC_SINK, MIC_SOURCE, pactl};

    pub const PROVIDER: &str = "PulseAudio";
    pub const LINK: Option<&str> = None;

    pub fn candidates(_outputs: &[OutputDevice]) -> Vec<OutputDevice> {
        let Ok(sinks) = pulse::sinks() else {
            return Vec::new();
        };
        if !sinks.iter().any(|(_, name)| name == MIC_SINK) {
            return Vec::new();
        }
        vec![OutputDevice {
            id: pulse::device_id(MIC_SINK),
            name: "locoSaonduh-Mic".to_owned(),
            is_default: false,
        }]
    }

    pub fn hides_input(id: &str, _name: &str) -> bool {
        pulse::is_ours(id)
    }

    pub fn install() -> Result<()> {
        prepare()
    }

    pub fn prepare() -> Result<()> {
        if !candidates(&[]).is_empty() {
            return Ok(());
        }
        pactl(&[
            "load-module",
            "module-null-sink",
            &format!("sink_name={MIC_SINK}"),
            "sink_properties=device.description=locoSaonduh-Sink",
        ])?;
        pactl(&[
            "load-module",
            "module-remap-source",
            &format!("master={MIC_SINK}.monitor"),
            &format!("source_name={MIC_SOURCE}"),
            "source_properties=device.description=locoSaonduh-Mic",
        ])?;
        Ok(())
    }

    pub fn release() {
        for module in pulse::modules() {
            if module.args.contains(&format!("source_name={MIC_SOURCE}"))
                || module.args.contains(&format!("sink_name={MIC_SINK}"))
            {
                pulse::unload(&module.index);
            }
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    fn device(name: &str) -> OutputDevice {
        OutputDevice {
            id: name.to_owned(),
            name: name.to_owned(),
            is_default: false,
        }
    }

    #[test]
    fn the_two_channel_cable_is_preferred_over_the_16_channel_one() {
        let outputs = [
            device("CABLE In 16ch (VB-Audio Virtual Cable)"),
            device("Speakers"),
            device("CABLE Input (VB-Audio Virtual Cable)"),
        ];
        let names = candidates(&outputs)
            .into_iter()
            .map(|device| device.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "CABLE Input (VB-Audio Virtual Cable)",
                "CABLE In 16ch (VB-Audio Virtual Cable)"
            ]
        );
        assert!(hides_input("", "CABLE Output (VB-Audio Virtual Cable)"));
        assert!(!hides_input("", "Microphone"));
    }
}
