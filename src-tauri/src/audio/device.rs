//! 耳ひとつ 己が声のみ 聞かぬ秋

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use rtrb::RingBuffer;
use serde::Serialize;

use super::system;
use crate::protocol::FRAME_SAMPLES;

pub const SYSTEM_SOURCE_ID: &str = "system|*";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSource {
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    System,
    Microphone,
}

impl SourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Microphone => "microphone",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "microphone" => Some(Self::Microphone),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn system_source() -> AudioSource {
    AudioSource {
        id: SYSTEM_SOURCE_ID.to_owned(),
        name: "系统声音".to_owned(),
        kind: SourceKind::System,
        is_default: true,
    }
}

pub fn list_microphones(hide: impl Fn(&str, &str) -> bool) -> Vec<AudioSource> {
    let host = cpal::default_host();
    let default_input = host.default_input_device();
    let mut sources = Vec::new();

    if let Ok(devices) = host.input_devices() {
        for device in devices {
            let Ok(description) = device.description() else {
                continue;
            };
            let Ok(id) = device.id() else {
                continue;
            };
            let id = id.to_string();
            let name = description.name().to_owned();
            if system::is_monitor(&id) || hide(&id, &name) {
                continue;
            }
            sources.push(AudioSource {
                id: format!("mic|{id}"),
                name,
                kind: SourceKind::Microphone,
                is_default: default_input.as_ref() == Some(&device),
            });
        }
    }

    sources.sort_by_key(|source| (!source.is_default, source.name.to_lowercase()));
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
            let id = id.to_string();
            if system::is_private_sink(&id) {
                continue;
            }
            devices.push(OutputDevice {
                id,
                name: description.name().to_owned(),
                is_default: default.as_ref() == Some(&device),
            });
        }
    }
    devices.sort_by_key(|device| (!device.is_default, device.name.to_lowercase()));
    devices
}

pub enum CaptureGuard {
    Cpal(cpal::Stream),
    #[cfg(target_os = "windows")]
    Excluding(system::ExcludingCapture),
    #[cfg(target_os = "macos")]
    Tap(system::ExcludingTap),
    #[cfg(target_os = "linux")]
    Takeover(system::Takeover),
}

impl CaptureGuard {
    pub fn playback_override(&self) -> Option<String> {
        #[cfg(target_os = "linux")]
        if let Self::Takeover(takeover) = self {
            return Some(takeover.original_output());
        }
        None
    }
}

pub struct OpenCapture {
    pub guard: CaptureGuard,
    pub feed: rtrb::Consumer<f32>,
    pub source: AudioSource,
    pub sample_rate: u32,
    pub channels: usize,
    pub taps: CaptureTaps,
    pub excludes_self: bool,
}

impl OpenCapture {
    pub fn from_device(
        source: &AudioSource,
        capture: DeviceCapture,
        guard: impl FnOnce(cpal::Stream) -> CaptureGuard,
        excludes_self: bool,
    ) -> Self {
        let DeviceCapture {
            stream,
            feed,
            sample_rate,
            channels,
            taps,
        } = capture;
        Self {
            guard: guard(stream),
            feed,
            source: source.clone(),
            sample_rate,
            channels,
            taps,
            excludes_self,
        }
    }
}

pub struct DeviceCapture {
    pub stream: cpal::Stream,
    pub feed: rtrb::Consumer<f32>,
    pub sample_rate: u32,
    pub channels: usize,
    pub taps: CaptureTaps,
}

#[derive(Clone, Default)]
pub struct CaptureTaps {
    pub peak: Arc<AtomicU32>,
    pub error: Arc<Mutex<Option<String>>>,
}

impl CaptureTaps {
    pub fn fail(&self, message: impl Into<String>) {
        let mut slot = self.error.lock().unwrap();
        if slot.is_none() {
            *slot = Some(message.into());
        }
    }

    pub fn push(&self, producer: &mut rtrb::Producer<f32>, samples: impl Iterator<Item = f32>) {
        let mut local_peak = 0.0_f32;
        for value in samples {
            local_peak = local_peak.max(value.abs());
            let _ = producer.push(value);
        }
        if local_peak > 0.0 {
            self.peak.fetch_max(local_peak.to_bits(), Ordering::Relaxed);
        }
    }
}

pub fn ring(sample_rate: u32, channels: usize) -> (rtrb::Producer<f32>, rtrb::Consumer<f32>) {
    let capacity = (sample_rate as usize * channels).clamp(8_192, 1_048_576);
    RingBuffer::<f32>::new(capacity)
}

/// Open a capture stream and return a realtime-safe feed for the encoder.
pub fn open_capture(source: &AudioSource, local_playback: bool) -> Result<OpenCapture> {
    if source.id == SYSTEM_SOURCE_ID {
        return system::open(source, local_playback);
    }
    let device_id = source
        .id
        .strip_prefix("mic|")
        .ok_or_else(|| anyhow!("unknown audio source"))?
        .parse::<cpal::DeviceId>()
        .context("unrecognised audio device")?;
    let device = cpal::default_host()
        .device_by_id(&device_id)
        .ok_or_else(|| anyhow!("audio device is no longer connected"))?;
    let capture = open_device_capture(&device, false)?;
    Ok(OpenCapture::from_device(
        source,
        capture,
        CaptureGuard::Cpal,
        false,
    ))
}

/// Frames per capture fragment. One codec frame keeps the send cadence even;
/// `LOCOSAONDUH_CAPTURE_FRAG` overrides it for hosts whose audio server stalls
/// on very small fragments.
fn capture_frames() -> u32 {
    std::env::var("LOCOSAONDUH_CAPTURE_FRAG")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|frames| (32..=48_000).contains(frames))
        .unwrap_or(FRAME_SAMPLES as u32)
}

pub fn open_device_capture(device: &cpal::Device, loopback: bool) -> Result<DeviceCapture> {
    let config = if loopback {
        device
            .default_output_config()
            .context("this output cannot be opened for loopback capture")?
    } else {
        device
            .default_input_config()
            .context("this microphone cannot be opened")?
    };

    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;
    let (producer, feed) = ring(sample_rate, channels);
    let taps = CaptureTaps::default();

    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let frames = capture_frames();
    let mut producer = Some(producer);
    let mut stream = None;
    for size in [Some(frames), None] {
        match build_input_for_format(
            device,
            format,
            config.clone(),
            producer.take().expect("each attempt consumes the producer"),
            taps.clone(),
            size,
        ) {
            Ok(built) => {
                stream = Some(built);
                break;
            }
            Err(error) if size.is_some() => {
                // 溜めざるを まずは器の 縁に問ふ
                log::warn!(
                    "a {frames}-frame capture chunk was refused ({error:#}); \
                     retrying with the host's own buffer size"
                );
            }
            Err(error) => return Err(error.context("could not start capture")),
        }
    }
    let stream = stream.expect("the host default buffer is always attempted");

    stream
        .play()
        .context("could not start the capture stream")?;
    Ok(DeviceCapture {
        stream,
        feed,
        sample_rate,
        channels,
        taps,
    })
}

// 汲む水を 溜めず運ぶ 竿の先
fn build_input_for_format(
    device: &cpal::Device,
    format: SampleFormat,
    mut config: cpal::StreamConfig,
    producer: rtrb::Producer<f32>,
    taps: CaptureTaps,
    frames: Option<u32>,
) -> Result<cpal::Stream> {
    if let Some(frames) = frames {
        // An unset size lets the server fill in its own default capture
        // fragment — two full seconds on pipewire-pulse — and the sender
        // sits on that much audio before the first packet leaves.
        config.buffer_size = cpal::BufferSize::Fixed(frames);
    }
    match format {
        SampleFormat::F64 => build_input::<f64>(device, config, producer, taps),
        SampleFormat::F32 => build_input::<f32>(device, config, producer, taps),
        SampleFormat::I32 => build_input::<i32>(device, config, producer, taps),
        SampleFormat::I24 => build_input::<cpal::I24>(device, config, producer, taps),
        SampleFormat::I16 => build_input::<i16>(device, config, producer, taps),
        SampleFormat::I8 => build_input::<i8>(device, config, producer, taps),
        SampleFormat::U32 => build_input::<u32>(device, config, producer, taps),
        SampleFormat::U24 => build_input::<cpal::U24>(device, config, producer, taps),
        SampleFormat::U16 => build_input::<u16>(device, config, producer, taps),
        SampleFormat::U8 => build_input::<u8>(device, config, producer, taps),
        other => Err(anyhow!("unsupported sample format {other}")),
    }
}

fn build_input<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut producer: rtrb::Producer<f32>,
    taps: CaptureTaps,
) -> Result<cpal::Stream>
where
    T: SizedSample + cpal::Sample,
    f32: FromSample<T>,
{
    let error_taps = taps.clone();
    let err = move |error: cpal::Error| error_taps.fail(error.to_string());
    let stream = device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                taps.push(
                    &mut producer,
                    data.iter().map(|&sample| f32::from_sample(sample)),
                );
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
        let sources = list_microphones(|_, _| false);
        let mut ids = std::collections::HashSet::new();
        for source in &sources {
            assert!(ids.insert(source.id.clone()), "duplicate source id");
            assert!(!source.name.is_empty());
            assert_eq!(source.kind, SourceKind::Microphone);
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
    fn kinds_round_trip_through_their_wire_labels() {
        for kind in [SourceKind::System, SourceKind::Microphone] {
            assert_eq!(SourceKind::parse(kind.label()), Some(kind));
        }
        assert_eq!(SourceKind::parse("speaker"), None);
    }

    #[test]
    #[ignore = "requires a real audio endpoint; run manually"]
    fn system_capture_delivers_samples() {
        let mut capture = open_capture(&system_source(), true).expect("capture should open");
        std::thread::sleep(std::time::Duration::from_millis(400));
        let mut samples = 0usize;
        while capture.feed.pop().is_ok() {
            samples += 1;
        }
        let excludes_self = capture.excludes_self;
        drop(capture);
        assert!(samples > 0, "the capture produced no samples");
        if cfg!(target_os = "windows") {
            assert!(
                excludes_self,
                "Windows should keep this app out of the capture"
            );
        }
    }

    #[test]
    #[ignore = "requires a real audio endpoint; run manually"]
    fn playback_endpoint_opens() {
        let sink = crate::audio::PlaybackSink::open(None).expect("output should open");
        std::thread::sleep(std::time::Duration::from_millis(120));
        drop(sink);
    }
}
