//! 糸車 ひとつに縒りて 秋の声

use std::process::Command;

use anyhow::{Context, Result, anyhow};

pub const SYSTEM_SINK: &str = "locosaonduh_system";
pub const MIC_SINK: &str = "locosaonduh_mic";
pub const MIC_SOURCE: &str = "locosaonduh_mic_input";
const PREFIX: &str = "locosaonduh_";

pub fn is_ours(device_id: &str) -> bool {
    device_id.contains(PREFIX)
}

pub fn device_id(name: &str) -> String {
    cpal::DeviceId::new(cpal::HostId::PulseAudio, name).to_string()
}

/// 秋の灯に 訛りを抜きて 文字を読む
pub fn pactl(args: &[&str]) -> Result<String> {
    let output = Command::new("pactl")
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .context("pactl is not available")?;
    if !output.status.success() {
        return Err(anyhow!(
            "pactl {}: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn default_sink() -> Result<String> {
    pactl(&["info"])?
        .lines()
        .find_map(|line| line.strip_prefix("Default Sink: "))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("no default sink"))
}

pub struct Module {
    pub index: String,
    pub args: String,
}

pub fn modules() -> Vec<Module> {
    let Ok(list) = pactl(&["list", "short", "modules"]) else {
        return Vec::new();
    };
    list.lines()
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let index = columns.next()?.to_owned();
            columns.next()?;
            let args = columns.next().unwrap_or_default().to_owned();
            Some(Module { index, args })
        })
        .collect()
}

pub fn unload(index: &str) {
    let _ = pactl(&["unload-module", index]);
}

/// 番号と 名を対にして 札を読む
pub fn sinks() -> Result<Vec<(String, String)>> {
    Ok(pactl(&["list", "short", "sinks"])?
        .lines()
        .filter_map(|line| {
            let mut columns = line.split('\t');
            Some((columns.next()?.to_owned(), columns.next()?.to_owned()))
        })
        .collect())
}

pub struct SinkInput {
    pub index: String,
    pub sink: String,
    pub owner_module: Option<String>,
    pub ours: bool,
}

pub fn sink_inputs() -> Result<Vec<SinkInput>> {
    let listing = pactl(&["list", "sink-inputs"])?;
    Ok(parse_sink_inputs(&listing, &sinks()?, std::process::id()))
}

fn parse_sink_inputs(listing: &str, sinks: &[(String, String)], pid: u32) -> Vec<SinkInput> {
    let pid = format!("\"{pid}\"");
    let mut inputs = Vec::new();
    for block in listing.split("Sink Input #").skip(1) {
        let mut lines = block.lines();
        let Some(index) = lines.next().map(|line| line.trim().to_owned()) else {
            continue;
        };
        let mut sink = String::new();
        let mut owner_module = None;
        let mut ours = false;
        for line in lines {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("Sink: ") {
                sink = sinks
                    .iter()
                    .find(|(id, _)| id == value)
                    .map(|(_, name)| name.clone())
                    .unwrap_or_else(|| value.to_owned());
            } else if let Some(value) = line.strip_prefix("Owner Module: ") {
                owner_module = Some(value.to_owned());
            } else if let Some(value) = line.strip_prefix("application.process.id = ") {
                ours = value == pid;
            }
        }
        inputs.push(SinkInput {
            index,
            sink,
            owner_module,
            ours,
        });
    }
    inputs
}

pub fn move_inputs(from: &str, to: &str) {
    if let Ok(inputs) = sink_inputs() {
        for input in inputs.iter().filter(|input| input.sink == from) {
            let _ = pactl(&["move-sink-input", &input.index, to]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = "Sink Input #42
\tDriver: protocol-native.c
\tOwner Module: 10
\tClient: 77
\tSink: 1
\tProperties:
\t\tapplication.name = \"cpal-pulseaudio-9999\"
\t\tapplication.process.id = \"1234\"

Sink Input #43
\tDriver: protocol-native.c
\tOwner Module: 10
\tClient: 78
\tSink: 1
\tProperties:
\t\tapplication.name = \"Firefox\"
\t\tapplication.process.id = \"12345\"

Sink Input #44
\tDriver: module-loopback.c
\tOwner Module: 25
\tClient: n/a
\tSink: 0
\tProperties:
\t\tmedia.name = \"Loopback\"
";

    #[test]
    fn sink_inputs_are_matched_by_process_id_alone() {
        let sinks = vec![
            ("0".to_owned(), "alsa_output.speakers".to_owned()),
            ("1".to_owned(), SYSTEM_SINK.to_owned()),
        ];
        let inputs = parse_sink_inputs(LISTING, &sinks, 1234);
        assert_eq!(inputs.len(), 3);
        assert!(inputs[0].ours, "the process id identifies this app");
        assert!(
            !inputs[1].ours,
            "a longer id with the same prefix is someone else"
        );
        assert!(!inputs[2].ours);
        assert_eq!(inputs[0].sink, SYSTEM_SINK);
        assert_eq!(inputs[2].sink, "alsa_output.speakers");
        assert_eq!(inputs[2].owner_module.as_deref(), Some("25"));
    }
}
