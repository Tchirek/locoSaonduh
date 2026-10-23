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
    let pid = std::process::id().to_string();
    let marker = format!("cpal-pulseaudio-{pid}");
    let sinks = sinks()?;
    let mut inputs = Vec::new();
    for block in pactl(&["list", "sink-inputs"])?
        .split("Sink Input #")
        .skip(1)
    {
        let mut lines = block.lines();
        let Some(index) = lines.next().map(|line| line.trim().to_owned()) else {
            continue;
        };
        let mut sink = String::new();
        let mut owner_module = None;
        let mut ours = block.contains(&marker);
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
                ours |= value.trim_matches('"') == pid;
            }
        }
        inputs.push(SinkInput {
            index,
            sink,
            owner_module,
            ours,
        });
    }
    Ok(inputs)
}

pub fn move_inputs(from: &str, to: &str) {
    if let Ok(inputs) = sink_inputs() {
        for input in inputs.iter().filter(|input| input.sink == from) {
            let _ = pactl(&["move-sink-input", &input.index, to]);
        }
    }
}
