//! The output side. The mixer produces 48 kHz stereo; this module owns the
//! device callback, rate conversion and channel mapping, and hands the mixer a
//! realtime-safe producer.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use rtrb::RingBuffer;
use serde::Serialize;

use super::convert::PlaybackAdapter;
use crate::protocol::SAMPLE_RATE;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackInfo {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct PlaybackSink {
    pub stream: cpal::Stream,
    pub feed: rtrb::Producer<[f32; 2]>,
    pub info: PlaybackInfo,
    pub taps: OutputTaps,
}

// 欠けし数 乞ひし量をも 書き残す
#[derive(Clone, Default)]
pub struct OutputTaps {
    pub underruns: Arc<AtomicU64>,
    pub quantum: Arc<AtomicU32>,
    pub error: Arc<Mutex<Option<String>>>,
}

impl PlaybackSink {
    pub fn open(device_id: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let device = match device_id {
            Some(id) => {
                let parsed = id
                    .parse::<cpal::DeviceId>()
                    .context("invalid output device id")?;
                host.device_by_id(&parsed)
                    .ok_or_else(|| anyhow!("output device is no longer connected"))?
            }
            None => host
                .default_output_device()
                .ok_or_else(|| anyhow!("no audio output device"))?,
        };
        let description = device.description().context("output device disappeared")?;
        let config = device
            .default_output_config()
            .context("this output cannot be opened")?;
        let sample_rate = config.sample_rate();
        let channels = config.channels();
        let info = PlaybackInfo {
            device_name: description.name().to_owned(),
            sample_rate,
            channels,
        };

        let capacity = (sample_rate as usize * 2).clamp(16_384, 524_288);
        let (feed, consumer) = RingBuffer::<[f32; 2]>::new(capacity);
        let taps = OutputTaps::default();

        let format = config.sample_format();
        let frames = SAMPLE_RATE / 100;
        let config: cpal::StreamConfig = config.into();
        let mut consumer = Some(consumer);
        let mut stream = None;
        for size in [Some(frames), None] {
            match build_output_for_format(
                &device,
                format,
                config.clone(),
                consumer.take().expect("each attempt consumes the consumer"),
                taps.clone(),
                size,
            ) {
                Ok(built) => {
                    stream = Some(built);
                    break;
                }
                Err(error) if size.is_some() => {
                    // 浅き江の 深き溜めを 端に置く
                    log::warn!(
                        "a {frames}-frame output chunk was refused ({error:#}); \
                         retrying with the host's own buffer size"
                    );
                }
                Err(error) => return Err(error.context("could not start playback")),
            }
        }
        let stream = stream.expect("the host default buffer is always attempted");

        stream.play().context("could not start the output stream")?;
        Ok(Self {
            stream,
            feed,
            info,
            taps,
        })
    }
}

// 還る音 浅き瀬にこそ 速やかに
fn build_output_for_format(
    device: &cpal::Device,
    format: SampleFormat,
    mut config: cpal::StreamConfig,
    consumer: rtrb::Consumer<[f32; 2]>,
    taps: OutputTaps,
    frames: Option<u32>,
) -> Result<cpal::Stream> {
    if let Some(frames) = frames {
        // An unset size lets the server prebuffer its own default target
        // (two full seconds on pipewire-pulse) before anything is heard.
        config.buffer_size = cpal::BufferSize::Fixed(frames);
    }
    match format {
        SampleFormat::F64 => build_output::<f64>(device, config, consumer, taps),
        SampleFormat::F32 => build_output::<f32>(device, config, consumer, taps),
        SampleFormat::I32 => build_output::<i32>(device, config, consumer, taps),
        SampleFormat::I24 => build_output::<cpal::I24>(device, config, consumer, taps),
        SampleFormat::I16 => build_output::<i16>(device, config, consumer, taps),
        SampleFormat::I8 => build_output::<i8>(device, config, consumer, taps),
        SampleFormat::U32 => build_output::<u32>(device, config, consumer, taps),
        SampleFormat::U24 => build_output::<cpal::U24>(device, config, consumer, taps),
        SampleFormat::U16 => build_output::<u16>(device, config, consumer, taps),
        SampleFormat::U8 => build_output::<u8>(device, config, consumer, taps),
        other => Err(anyhow!("unsupported sample format {other}")),
    }
}

fn build_output<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut consumer: rtrb::Consumer<[f32; 2]>,
    taps: OutputTaps,
) -> Result<cpal::Stream>
where
    T: SizedSample + cpal::Sample,
    T: FromSample<f32>,
{
    let channels = config.channels as usize;
    let rate = config.sample_rate as u64;
    let mut adapter = PlaybackAdapter::new(config.sample_rate);
    let OutputTaps {
        underruns,
        quantum,
        error: error_slot,
    } = taps;
    let err = move |error: cpal::Error| {
        let mut slot = error_slot.lock().unwrap();
        if slot.is_none() {
            *slot = Some(error.to_string());
        }
    };
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let wanted = (data.len() / channels) as u64 * SAMPLE_RATE as u64 / rate.max(1);
            quantum.fetch_max(wanted as u32, Ordering::Relaxed);
            let mut missing = 0_u64;
            for frame in data.chunks_mut(channels) {
                let [left, right] = adapter.next(|| match consumer.pop() {
                    Ok(frame) => frame,
                    Err(_) => {
                        missing += 1;
                        [0.0, 0.0]
                    }
                });
                if channels == 1 {
                    frame[0] = T::from_sample((left + right) * 0.5);
                } else {
                    frame[0] = T::from_sample(left);
                    frame[1] = T::from_sample(right);
                    for channel in frame.iter_mut().skip(2) {
                        *channel = T::from_sample(0.0);
                    }
                }
            }
            if missing > 0 {
                underruns.fetch_add(missing, Ordering::Relaxed);
            }
        },
        err,
        None,
    )?;
    Ok(stream)
}
