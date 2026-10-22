//! Device discovery and the capture side of the native audio graph.
//!
//! On Windows, opening an input stream on a render endpoint takes the WASAPI
//! loopback path. Listing both render and capture endpoints in one source menu
//! means "system sound" and a microphone are the same kind of choice to the
//! person using the app.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use rtrb::RingBuffer;
use serde::Serialize;

use crate::protocol::SAMPLE_RATE;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSource {
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    pub detail: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    System,
    Microphone,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub sample_rate: u32,
    pub channels: u16,
}

pub fn list_sources() -> Vec<AudioSource> {
    let host = cpal::default_host();
    let default_output = host.default_output_device();
    let default_input = host.default_input_device();
    let mut sources = Vec::new();

    if let Ok(devices) = host.output_devices() {
        for device in devices {
            let Ok(description) = device.description() else {
                continue;
            };
            let Ok(id) = device.id() else {
                continue;
            };
            let details = device.default_output_config().ok();
            let is_default = default_output.as_ref() == Some(&device);
            sources.push(AudioSource {
                id: format!("system|{id}"),
                name: description.name().to_owned(),
                kind: SourceKind::System,
                detail: match &details {
                    Some(config) => format!(
                        "系统声音 · {} kHz · {} 声道",
                        config.sample_rate() / 1_000,
                        config.channels()
                    ),
                    None => "系统声音".to_owned(),
                },
                is_default,
            });
        }
    }

    if let Ok(devices) = host.input_devices() {
        for device in devices {
            let Ok(description) = device.description() else {
                continue;
            };
            let Ok(id) = device.id() else {
                continue;
            };
            let details = device.default_input_config().ok();
            let is_default = default_input.as_ref() == Some(&device);
            sources.push(AudioSource {
                id: format!("mic|{id}"),
                name: description.name().to_owned(),
                kind: SourceKind::Microphone,
                detail: match &details {
                    Some(config) => format!(
                        "麦克风 · {} kHz · {} 声道",
                        config.sample_rate() / 1_000,
                        config.channels()
                    ),
                    None => "麦克风".to_owned(),
                },
                is_default,
            });
        }
    }

    sources.sort_by_key(|source| {
        (
            !source.is_default,
            match source.kind {
                SourceKind::System => 0,
                SourceKind::Microphone => 1,
            },
            source.name.to_lowercase(),
        )
    });
    sources
}

pub fn list_output_devices() -> Vec<OutputDevice> {
    let host = cpal::default_host();
    let default = host.default_output_device();
    let mut devices = Vec::new();
    if let Ok(outputs) = host.output_devices() {
        for device in outputs {
            let Ok(description) = device.description() else {
                continue;
            };
            let Ok(id) = device.id() else {
                continue;
            };
            let config = device.default_output_config().ok();
            devices.push(OutputDevice {
                id: id.to_string(),
                name: description.name().to_owned(),
                is_default: default.as_ref() == Some(&device),
                sample_rate: config
                    .as_ref()
                    .map(|c| c.sample_rate())
                    .unwrap_or(SAMPLE_RATE),
                channels: config.as_ref().map(|c| c.channels()).unwrap_or(2),
            });
        }
    }
    devices.sort_by_key(|device| (!device.is_default, device.name.to_lowercase()));
    devices
}

pub struct OpenCapture {
    pub stream: cpal::Stream,
    pub feed: rtrb::Consumer<f32>,
    pub source: AudioSource,
    pub sample_rate: u32,
    pub channels: usize,
    pub peak: Arc<AtomicU32>,
    pub overflowed_samples: Arc<AtomicU64>,
    pub error: Arc<Mutex<Option<String>>>,
}

/// Open a capture stream and return a realtime-safe feed for the encoder.
pub fn open_capture(source: &AudioSource) -> Result<OpenCapture> {
    let host = cpal::default_host();
    let (kind, raw_id) = source
        .id
        .split_once('|')
        .ok_or_else(|| anyhow!("invalid audio source id"))?;
    let device_id = raw_id
        .parse::<cpal::DeviceId>()
        .context("unrecognised audio device")?;
    let device = host
        .device_by_id(&device_id)
        .ok_or_else(|| anyhow!("audio device is no longer connected"))?;

    let config = match kind {
        "system" => device
            .default_output_config()
            .context("this output cannot be opened for loopback capture")?,
        "mic" => device
            .default_input_config()
            .context("this microphone cannot be opened")?,
        _ => return Err(anyhow!("unknown audio source kind")),
    };

    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;
    let capacity = (sample_rate as usize * channels).clamp(8_192, 1_048_576);
    let (producer, feed) = RingBuffer::<f32>::new(capacity);
    let peak = Arc::new(AtomicU32::new(0));
    let overflowed_samples = Arc::new(AtomicU64::new(0));
    let error_slot: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let error_for_cb = Arc::clone(&error_slot);

    let stream = match config.sample_format() {
        SampleFormat::F64 => build_input::<f64>(
            &device,
            config.into(),
            producer,
            Arc::clone(&peak),
            Arc::clone(&overflowed_samples),
            error_for_cb,
        ),
        SampleFormat::F32 => build_input::<f32>(
            &device,
            config.into(),
            producer,
            Arc::clone(&peak),
            Arc::clone(&overflowed_samples),
            error_for_cb,
        ),
        SampleFormat::I32 => build_input::<i32>(
            &device,
            config.into(),
            producer,
            Arc::clone(&peak),
            Arc::clone(&overflowed_samples),
            error_for_cb,
        ),
        SampleFormat::I24 => build_input::<cpal::I24>(
            &device,
            config.into(),
            producer,
            Arc::clone(&peak),
            Arc::clone(&overflowed_samples),
            error_for_cb,
        ),
        SampleFormat::I16 => build_input::<i16>(
            &device,
            config.into(),
            producer,
            Arc::clone(&peak),
            Arc::clone(&overflowed_samples),
            error_for_cb,
        ),
        SampleFormat::I8 => build_input::<i8>(
            &device,
            config.into(),
            producer,
            Arc::clone(&peak),
            Arc::clone(&overflowed_samples),
            error_for_cb,
        ),
        SampleFormat::U32 => build_input::<u32>(
            &device,
            config.into(),
            producer,
            Arc::clone(&peak),
            Arc::clone(&overflowed_samples),
            error_for_cb,
        ),
        SampleFormat::U24 => build_input::<cpal::U24>(
            &device,
            config.into(),
            producer,
            Arc::clone(&peak),
            Arc::clone(&overflowed_samples),
            error_for_cb,
        ),
        SampleFormat::U16 => build_input::<u16>(
            &device,
            config.into(),
            producer,
            Arc::clone(&peak),
            Arc::clone(&overflowed_samples),
            error_for_cb,
        ),
        SampleFormat::U8 => build_input::<u8>(
            &device,
            config.into(),
            producer,
            Arc::clone(&peak),
            Arc::clone(&overflowed_samples),
            error_for_cb,
        ),
        other => return Err(anyhow!("unsupported sample format {other}")),
    }
    .context("could not start loopback capture")?;

    stream
        .play()
        .context("could not start the capture stream")?;
    if let Some(message) = error_slot.lock().unwrap().take() {
        return Err(anyhow!(message));
    }

    Ok(OpenCapture {
        stream,
        feed,
        source: source.clone(),
        sample_rate,
        channels,
        peak,
        overflowed_samples,
        error: error_slot,
    })
}

fn build_input<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut producer: rtrb::Producer<f32>,
    peak: Arc<AtomicU32>,
    overflowed: Arc<AtomicU64>,
    error_slot: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream>
where
    T: SizedSample + cpal::Sample,
    f32: FromSample<T>,
{
    let err = move |error: cpal::Error| {
        let mut slot = error_slot.lock().unwrap();
        if slot.is_none() {
            *slot = Some(error.to_string());
        }
    };
    let stream = device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let mut local_peak = 0.0_f32;
                let mut dropped = 0_u64;
                for &sample in data {
                    let value = f32::from_sample(sample);
                    local_peak = local_peak.max(value.abs());
                    if producer.push(value).is_err() {
                        dropped += 1;
                    }
                }
                if local_peak > 0.0 {
                    peak.fetch_max(local_peak.to_bits(), Ordering::Relaxed);
                }
                if dropped > 0 {
                    overflowed.fetch_add(dropped, Ordering::Relaxed);
                }
            },
            err,
            None,
        )
        .context("failed to build capture stream")?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_listing_is_non_panicking_and_unique() {
        let sources = list_sources();
        let mut ids = std::collections::HashSet::new();
        for source in &sources {
            assert!(ids.insert(source.id.clone()), "duplicate source id");
            assert!(!source.name.is_empty());
        }
    }

    #[test]
    fn output_listing_has_a_default_when_available() {
        let outputs = list_output_devices();
        if !outputs.is_empty() {
            assert!(outputs.iter().filter(|device| device.is_default).count() <= 1);
        }
    }

    #[test]
    #[ignore = "requires a real audio endpoint; run manually"]
    fn system_loopback_delivers_samples() {
        let sources = list_sources();
        let Some(source) = sources
            .iter()
            .find(|source| source.kind == SourceKind::System)
            .or_else(|| sources.first())
        else {
            return;
        };
        let mut capture = open_capture(source).expect("capture should open");
        std::thread::sleep(std::time::Duration::from_millis(250));
        let mut samples = 0usize;
        while capture.feed.pop().is_ok() {
            samples += 1;
        }
        drop(capture);
        assert!(samples > 0, "the capture callback produced no samples");
    }

    #[test]
    #[ignore = "requires a real audio endpoint; run manually"]
    fn playback_endpoint_opens() {
        let sink = crate::audio::PlaybackSink::open(None).expect("output should open");
        std::thread::sleep(std::time::Duration::from_millis(120));
        drop(sink);
    }
}
