//! 鳴く虫の 己が音だけを 聞きもらす

#[cfg(target_os = "windows")]
pub use windows_impl::{ExcludingCapture, open};

#[cfg(target_os = "macos")]
pub use macos_impl::{ExcludingTap, open};

#[cfg(target_os = "linux")]
pub use linux_impl::{Takeover, cleanup_stale, open};

#[cfg(target_os = "windows")]
pub const DETAIL: &str = "WASAPI";
#[cfg(target_os = "macos")]
pub const DETAIL: &str = "Core Audio";
#[cfg(target_os = "linux")]
pub const DETAIL: &str = "PulseAudio";
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub const DETAIL: &str = "";

#[cfg(target_os = "linux")]
pub fn is_monitor(device_id: &str) -> bool {
    device_id.ends_with(".monitor")
}

#[cfg(not(target_os = "linux"))]
pub fn is_monitor(_device_id: &str) -> bool {
    false
}

#[cfg(target_os = "linux")]
pub fn is_private_sink(device_id: &str) -> bool {
    super::pulse::is_ours(device_id)
}

#[cfg(not(target_os = "linux"))]
pub fn is_private_sink(_device_id: &str) -> bool {
    false
}

/// 秋の舟 元の岸へと 声返す
pub fn default_playback_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        linux_impl::original_sink()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use anyhow::{Context, Result, anyhow};
    use cpal::traits::HostTrait;
    use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};

    use crate::audio::device::{
        AudioSource, CaptureGuard, CaptureTaps, OpenCapture, open_device_capture, ring,
    };
    use crate::protocol::{CHANNELS, SAMPLE_RATE};

    pub struct ExcludingCapture {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Drop for ExcludingCapture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    /// 古き戸は 己が声をも 通しけり
    pub fn open(source: &AudioSource) -> Result<OpenCapture> {
        open_excluding(source).or_else(|error| {
            log::warn!("process loopback unavailable, capturing the endpoint: {error:#}");
            open_endpoint(source)
        })
    }

    fn open_excluding(source: &AudioSource) -> Result<OpenCapture> {
        let taps = CaptureTaps::default();
        let (producer, feed) = ring(SAMPLE_RATE, CHANNELS);
        let capture = start(producer, taps.clone())?;
        Ok(OpenCapture {
            guard: CaptureGuard::Excluding(capture),
            feed,
            source: source.clone(),
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
            peak: taps.peak,
            overflowed_samples: taps.overflowed,
            error: taps.error,
            excludes_self: true,
        })
    }

    fn open_endpoint(source: &AudioSource) -> Result<OpenCapture> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| anyhow!("no audio output to capture"))?;
        let (stream, feed, sample_rate, channels, taps) = open_device_capture(&device, true)?;
        Ok(OpenCapture {
            guard: CaptureGuard::Cpal(stream),
            feed,
            source: source.clone(),
            sample_rate,
            channels,
            peak: taps.peak,
            overflowed_samples: taps.overflowed,
            error: taps.error,
            excludes_self: false,
        })
    }

    fn start(mut producer: rtrb::Producer<f32>, taps: CaptureTaps) -> Result<ExcludingCapture> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let worker_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("loco-loopback".to_owned())
            .spawn(move || {
                let _ = wasapi::initialize_mta();
                let opened = (|| {
                    let format = WaveFormat::new(
                        32,
                        32,
                        &SampleType::Float,
                        SAMPLE_RATE as usize,
                        CHANNELS,
                        None,
                    );
                    let mut client =
                        AudioClient::new_application_loopback_client(std::process::id(), false)?;
                    client.initialize_client(
                        &format,
                        &Direction::Capture,
                        &StreamMode::EventsShared {
                            autoconvert: true,
                            buffer_duration_hns: 0,
                        },
                    )?;
                    let event = client.set_get_eventhandle()?;
                    let capture = client.get_audiocaptureclient()?;
                    client.start_stream()?;
                    Ok::<_, wasapi::WasapiError>((client, event, capture))
                })();
                let (client, event, capture) = match opened {
                    Ok(parts) => {
                        let _ = ready_tx.send(Ok(()));
                        parts
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error.to_string()));
                        return;
                    }
                };
                let frame_bytes = CHANNELS * 4;
                let mut bytes = vec![0u8; SAMPLE_RATE as usize * frame_bytes];
                while !worker_stop.load(Ordering::Acquire) {
                    let _ = event.wait_for_event(50);
                    loop {
                        match capture.get_next_packet_size() {
                            Ok(Some(frames)) if frames > 0 => {}
                            Ok(_) => break,
                            Err(error) => {
                                taps.fail(error.to_string());
                                return;
                            }
                        }
                        let (frames, info) = match capture.read_from_device(&mut bytes) {
                            Ok(read) => read,
                            Err(error) => {
                                taps.fail(error.to_string());
                                return;
                            }
                        };
                        if frames == 0 {
                            break;
                        }
                        let block = &bytes[..frames as usize * frame_bytes];
                        if info.flags.silent {
                            taps.push(
                                &mut producer,
                                std::iter::repeat_n(0.0, frames as usize * CHANNELS),
                            );
                        } else {
                            taps.push(
                                &mut producer,
                                block
                                    .as_chunks::<4>()
                                    .0
                                    .iter()
                                    .map(|b| f32::from_le_bytes(*b)),
                            );
                        }
                    }
                }
                let _ = client.stop_stream();
            })
            .context("could not start the loopback thread")?;

        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => Ok(ExcludingCapture {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(message)) => {
                let _ = thread.join();
                Err(anyhow!(message))
            }
            Err(_) => {
                stop.store(true, Ordering::Release);
                Err(anyhow!("process loopback did not start in time"))
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod macos_impl {
    use std::ffi::{CStr, c_void};
    use std::ptr::NonNull;
    use std::time::Duration;

    use anyhow::{Result, anyhow};
    use cpal::traits::HostTrait;
    use objc2::AnyThread;
    use objc2_core_audio::{
        AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap,
        AudioHardwareDestroyAggregateDevice, AudioHardwareDestroyProcessTap,
        AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress, CATapDescription,
        CATapMuteBehavior, kAudioAggregateDeviceNameKey, kAudioAggregateDeviceTapAutoStartKey,
        kAudioAggregateDeviceTapListKey, kAudioAggregateDeviceUIDKey,
        kAudioEndPointDeviceIsPrivateKey, kAudioHardwarePropertyTranslatePIDToProcessObject,
        kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
        kAudioObjectUnknown, kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey,
    };
    use objc2_core_foundation::{
        CFArray, CFDictionary, CFMutableDictionary, CFRetained, CFString, kCFAllocatorDefault,
        kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks,
    };
    use objc2_foundation::{NSArray, NSNumber, NSString};

    use crate::audio::device::{AudioSource, CaptureGuard, OpenCapture, open_device_capture};

    pub struct ExcludingTap {
        tap_id: AudioObjectID,
        aggregate_id: AudioObjectID,
        uid: String,
    }

    impl Drop for ExcludingTap {
        fn drop(&mut self) {
            unsafe {
                let _ = AudioHardwareDestroyAggregateDevice(self.aggregate_id);
                let _ = AudioHardwareDestroyProcessTap(self.tap_id);
            }
        }
    }

    pub fn open(source: &AudioSource) -> Result<OpenCapture> {
        let tap = ExcludingTap::create()?;
        let host = cpal::default_host();
        let id = cpal::DeviceId::new(cpal::HostId::CoreAudio, &tap.uid);
        let mut device = None;
        for _ in 0..20 {
            device = host.device_by_id(&id);
            if device.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let device = device.ok_or_else(|| anyhow!("the system tap did not appear"))?;
        let (stream, feed, sample_rate, channels, taps) = open_device_capture(&device, false)?;
        Ok(OpenCapture {
            guard: CaptureGuard::Tap(stream, tap),
            feed,
            source: source.clone(),
            sample_rate,
            channels,
            peak: taps.peak,
            overflowed_samples: taps.overflowed,
            error: taps.error,
            excludes_self: true,
        })
    }

    impl ExcludingTap {
        fn create() -> Result<Self> {
            let pid = std::process::id();
            let own = own_process_object(pid as i32);
            let ids = own.into_iter().map(NSNumber::new_u32).collect::<Vec<_>>();
            let processes = NSArray::from_retained_slice(&ids);
            let description = unsafe {
                CATapDescription::initStereoGlobalTapButExcludeProcesses(
                    CATapDescription::alloc(),
                    &processes,
                )
            };
            unsafe {
                description.setMuteBehavior(CATapMuteBehavior::Unmuted);
                description.setPrivate(true);
                description.setName(&NSString::from_str("locoSaonduh"));
            }
            let mut tap_id: AudioObjectID = kAudioObjectUnknown;
            let status = unsafe { AudioHardwareCreateProcessTap(Some(&*description), &mut tap_id) };
            if status != 0 {
                return Err(anyhow!("process tap refused ({status})"));
            }
            let tap_uid = unsafe { description.UUID().UUIDString() };
            let uid = format!("com.tchirek.locosaonduh.system.{pid}");
            let properties = aggregate_properties(&tap_uid, &uid, "locoSaonduh");
            let mut aggregate_id: AudioObjectID = kAudioObjectUnknown;
            let status = unsafe {
                AudioHardwareCreateAggregateDevice(&properties, NonNull::from(&mut aggregate_id))
            };
            if status != 0 {
                unsafe {
                    let _ = AudioHardwareDestroyProcessTap(tap_id);
                }
                return Err(anyhow!("aggregate device refused ({status})"));
            }
            Ok(Self {
                tap_id,
                aggregate_id,
                uid,
            })
        }
    }

    fn own_process_object(pid: i32) -> Option<AudioObjectID> {
        let address = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyTranslatePIDToProcessObject,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut object: AudioObjectID = kAudioObjectUnknown;
        let mut size = size_of::<AudioObjectID>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&address),
                size_of::<i32>() as u32,
                (&pid as *const i32).cast(),
                NonNull::from(&mut size),
                NonNull::from(&mut object).cast(),
            )
        };
        (status == 0 && object != kAudioObjectUnknown).then_some(object)
    }

    fn key(name: &'static CStr) -> CFRetained<CFString> {
        CFString::from_str(name.to_str().unwrap_or_default())
    }

    fn aggregate_properties(tap_uid: &NSString, uid: &str, name: &str) -> CFRetained<CFDictionary> {
        unsafe {
            let sub_tap = CFMutableDictionary::new(
                kCFAllocatorDefault,
                2,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            )
            .unwrap();
            let yes = NSNumber::new_bool(true);
            CFMutableDictionary::set_value(
                Some(&*sub_tap),
                &*key(kAudioSubTapUIDKey) as *const _ as *const c_void,
                tap_uid as *const _ as *const c_void,
            );
            CFMutableDictionary::set_value(
                Some(&*sub_tap),
                &*key(kAudioSubTapDriftCompensationKey) as *const _ as *const c_void,
                &*yes as *const _ as *const c_void,
            );
            let list = [sub_tap];
            let taps = CFArray::new(
                kCFAllocatorDefault,
                list.as_ptr() as *mut *const c_void,
                list.len() as _,
                &kCFTypeArrayCallBacks,
            )
            .unwrap();
            let properties = CFMutableDictionary::new(
                kCFAllocatorDefault,
                5,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            )
            .unwrap();
            let name_value = CFString::from_str(name);
            let uid_value = CFString::from_str(uid);
            let entries: [(&'static CStr, *const c_void); 5] = [
                (
                    kAudioAggregateDeviceNameKey,
                    &*name_value as *const _ as *const c_void,
                ),
                (
                    kAudioAggregateDeviceUIDKey,
                    &*uid_value as *const _ as *const c_void,
                ),
                (
                    kAudioAggregateDeviceTapListKey,
                    &*taps as *const _ as *const c_void,
                ),
                (
                    kAudioAggregateDeviceTapAutoStartKey,
                    &*yes as *const _ as *const c_void,
                ),
                (
                    kAudioEndPointDeviceIsPrivateKey,
                    &*yes as *const _ as *const c_void,
                ),
            ];
            for (name, value) in entries {
                CFMutableDictionary::set_value(
                    Some(&*properties),
                    &*key(name) as *const _ as *const c_void,
                    value,
                );
            }
            CFRetained::cast_unchecked::<CFDictionary>(properties)
        }
    }
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::sync::Mutex;

    use anyhow::{Result, anyhow};
    use cpal::traits::HostTrait;

    use crate::audio::device::{AudioSource, CaptureGuard, OpenCapture, open_device_capture};
    use crate::audio::pulse::{self, SYSTEM_SINK, pactl};

    static ORIGINAL_SINK: Mutex<Option<String>> = Mutex::new(None);

    pub fn original_sink() -> Option<String> {
        ORIGINAL_SINK.lock().ok()?.as_deref().map(pulse::device_id)
    }

    /// 迷ひ子の 帯ほどきやる 夜寒かな
    pub fn cleanup_stale() {
        let mut restore = None;
        let mut stale = Vec::new();
        for module in pulse::modules() {
            if module
                .args
                .contains(&format!("source={SYSTEM_SINK}.monitor"))
            {
                restore = module
                    .args
                    .split_whitespace()
                    .find_map(|arg| arg.strip_prefix("sink="))
                    .map(str::to_owned);
                stale.push(module.index);
            } else if module.args.contains(&format!("sink_name={SYSTEM_SINK}")) {
                stale.push(module.index);
            }
        }
        if stale.is_empty() {
            return;
        }
        if let Some(original) = restore {
            let _ = pactl(&["set-default-sink", &original]);
            pulse::move_inputs(SYSTEM_SINK, &original);
        }
        for index in stale {
            pulse::unload(&index);
        }
    }

    pub struct Takeover {
        null_module: String,
        loopback_module: String,
        original: String,
    }

    impl Takeover {
        fn start() -> Result<Self> {
            cleanup_stale();
            let original = pulse::default_sink()?;
            let null_module = pactl(&[
                "load-module",
                "module-null-sink",
                &format!("sink_name={SYSTEM_SINK}"),
                "sink_properties=device.description=locoSaonduh",
            ])?;
            let loopback_module = match pactl(&[
                "load-module",
                "module-loopback",
                &format!("source={SYSTEM_SINK}.monitor"),
                &format!("sink={original}"),
                "latency_msec=20",
                "source_dont_move=true",
                "sink_dont_move=true",
            ]) {
                Ok(index) => index,
                Err(error) => {
                    pulse::unload(&null_module);
                    return Err(error);
                }
            };
            let takeover = Self {
                null_module,
                loopback_module,
                original: original.clone(),
            };
            pactl(&["set-default-sink", SYSTEM_SINK])?;
            for input in pulse::sink_inputs()? {
                let owned = input.owner_module.as_deref() == Some(&takeover.loopback_module);
                if input.ours || owned || input.sink != original {
                    continue;
                }
                let _ = pactl(&["move-sink-input", &input.index, SYSTEM_SINK]);
            }
            *ORIGINAL_SINK.lock().unwrap() = Some(original);
            Ok(takeover)
        }
    }

    impl Drop for Takeover {
        fn drop(&mut self) {
            *ORIGINAL_SINK.lock().unwrap() = None;
            let _ = pactl(&["set-default-sink", &self.original]);
            pulse::move_inputs(SYSTEM_SINK, &self.original);
            pulse::unload(&self.loopback_module);
            pulse::unload(&self.null_module);
        }
    }

    pub fn open(source: &AudioSource) -> Result<OpenCapture> {
        let takeover = Takeover::start()?;
        let host = cpal::default_host();
        let id = cpal::DeviceId::new(cpal::HostId::PulseAudio, format!("{SYSTEM_SINK}.monitor"));
        let device = host
            .device_by_id(&id)
            .ok_or_else(|| anyhow!("the takeover monitor is missing"))?;
        let (stream, feed, sample_rate, channels, taps) = open_device_capture(&device, false)?;
        Ok(OpenCapture {
            guard: CaptureGuard::Takeover(stream, takeover),
            feed,
            source: source.clone(),
            sample_rate,
            channels,
            peak: taps.peak,
            overflowed_samples: taps.overflowed,
            error: taps.error,
            excludes_self: true,
        })
    }
}
