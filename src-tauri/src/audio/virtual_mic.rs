//! 空蝉に 遠き声入れ 歌はしむ

use anyhow::Result;

use super::device::OutputDevice;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub const PROVIDER: &str = "VB-CABLE";
#[cfg(target_os = "linux")]
pub const PROVIDER: &str = "PulseAudio";
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub const PROVIDER: &str = "";

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub const LINK: &str = "https://vb-audio.com/Cable/";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub const LINK: &str = "";

pub fn supported() -> bool {
    cfg!(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    ))
}

pub fn find(outputs: &[OutputDevice]) -> Option<OutputDevice> {
    candidates(outputs).into_iter().next()
}

pub fn candidates(outputs: &[OutputDevice]) -> Vec<OutputDevice> {
    #[cfg(target_os = "linux")]
    {
        let _ = outputs;
        linux::find().into_iter().collect()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut matches = outputs
            .iter()
            .filter(|device| is_cable_output(&device.name))
            .cloned()
            .collect::<Vec<_>>();
        matches.sort_by_key(|device| device.name.contains("16"));
        matches
    }
}

#[cfg(target_os = "linux")]
pub fn hides_input(id: &str, _name: &str) -> bool {
    super::pulse::is_ours(id)
}

#[cfg(not(target_os = "linux"))]
pub fn hides_input(_id: &str, name: &str) -> bool {
    if cfg!(target_os = "windows") {
        name.starts_with("CABLE Output")
    } else {
        name.contains("VB-Cable")
    }
}

#[cfg_attr(target_os = "linux", allow(dead_code))]
fn is_cable_output(name: &str) -> bool {
    if cfg!(target_os = "windows") {
        name.starts_with("CABLE In")
    } else {
        name.contains("VB-Cable")
    }
}

pub fn install() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        windows::install()
    }
    #[cfg(target_os = "macos")]
    {
        macos::install()
    }
    #[cfg(target_os = "linux")]
    {
        linux::prepare()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(anyhow::anyhow!("unsupported"))
    }
}

pub fn prepare() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::prepare()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(())
    }
}

pub fn release() {
    #[cfg(target_os = "linux")]
    linux::release();
}

#[cfg(target_os = "windows")]
mod windows {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    use anyhow::{Context, Result, anyhow};

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
  $p = Start-Process -FilePath $setup -ArgumentList '-i','-h' -Verb RunAs -Wait -PassThru
} catch { throw 'cancelled' }
Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
exit $p.ExitCode
"#;

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
        let detail = String::from_utf8_lossy(&output.stderr);
        Err(if detail.contains("cancelled") {
            anyhow!("已取消安装")
        } else if detail.contains("signature") {
            anyhow!("安装包签名无效")
        } else {
            anyhow!("VB-CABLE 安装失败")
        })
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::process::Command;

    use anyhow::{Context, Result, anyhow};

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

    pub fn install() -> Result<()> {
        let output = Command::new("/bin/sh")
            .args(["-c", SCRIPT])
            .output()
            .context("sh is not available")?;
        if output.status.success() {
            return Ok(());
        }
        let detail = String::from_utf8_lossy(&output.stderr);
        Err(if detail.contains("-128") || detail.contains("canceled") {
            anyhow!("已取消安装")
        } else if detail.contains("signature") {
            anyhow!("安装包签名无效")
        } else {
            anyhow!("VB-CABLE 安装失败")
        })
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use anyhow::Result;

    use crate::audio::device::OutputDevice;
    use crate::audio::pulse::{self, MIC_SINK, MIC_SOURCE, pactl};
    use crate::protocol::{CHANNELS, SAMPLE_RATE};

    pub fn find() -> Option<OutputDevice> {
        let sinks = pulse::sinks().ok()?;
        sinks
            .iter()
            .any(|(_, name)| name == MIC_SINK)
            .then(|| OutputDevice {
                id: pulse::device_id(MIC_SINK),
                name: "locoSaonduh-Mic".to_owned(),
                is_default: false,
                sample_rate: SAMPLE_RATE,
                channels: CHANNELS as u16,
            })
    }

    pub fn prepare() -> Result<()> {
        if find().is_some() {
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
