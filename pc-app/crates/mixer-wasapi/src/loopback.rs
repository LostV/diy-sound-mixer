//! Per-app level meters from process loopback capture (Windows 10 2004+).
//!
//! The session meters Windows hands out (`IAudioMeterInformation` on a
//! session) report the whole device's level on some setups, e.g. the Astro
//! A50, so every active app showed the same meter. Capturing an app's own
//! audio and measuring it ourselves is correct on every driver.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        Media::Audio::{
            AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            AUDCLNT_STREAMFLAGS_LOOPBACK, AUDIOCLIENT_ACTIVATION_PARAMS,
            AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, ActivateAudioInterfaceAsync,
            IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
            IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient,
            PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
        },
        System::{
            Com::{
                BLOB, COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize, IAgileObject,
                IAgileObject_Impl, StructuredStorage::PROPVARIANT,
            },
            Threading::{CreateEventW, WaitForSingleObject},
            Variant::VT_BLOB,
        },
    },
    core::{Error, HRESULT, IUnknown, Interface, Ref, Result},
};
use windows_core::implement;

/// How long one published peak covers; matches the ~30 Hz UI update.
const WINDOW: Duration = Duration::from_millis(30);
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
/// 200 ms shared-mode buffer, in 100 ns units.
const BUFFER_DURATION: i64 = 2_000_000;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

/// Captures one process tree's audio on a background thread and keeps its
/// latest peak level. Dropping it stops the capture.
pub struct ProcessMeter {
    peak: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ProcessMeter {
    /// Starts capturing `pid`. Fails if Windows refuses the loopback stream,
    /// e.g. on Windows versions without process loopback.
    pub fn start(pid: u32) -> Result<Self> {
        let peak = Arc::new(AtomicU32::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();

        let thread = {
            let (peak, stop) = (peak.clone(), stop.clone());
            std::thread::Builder::new()
                .name(format!("meter-{pid}"))
                .spawn(move || capture_thread(pid, &peak, &stop, ready_tx))
                .map_err(|e| Error::new(HRESULT(-1), e.to_string()))?
        };

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                peak,
                stop,
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(Error::new(HRESULT(-1), "meter thread exited")),
        }
    }

    /// Highest absolute sample in the latest ~30 ms window, 0.0..=1.0.
    pub fn peak(&self) -> f32 {
        f32::from_bits(self.peak.load(Ordering::Relaxed))
    }
}

impl Drop for ProcessMeter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn capture_thread(pid: u32, peak: &AtomicU32, stop: &AtomicBool, ready: mpsc::Sender<Result<()>>) {
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let stream = match open_stream(pid) {
        Ok(stream) => {
            let _ = ready.send(Ok(()));
            Some(stream)
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            None
        }
    };
    if let Some(stream) = stream {
        if let Err(e) = stream.run(peak, stop) {
            log::debug!("meter for pid {pid} stopped: {e}");
        }
        peak.store(0f32.to_bits(), Ordering::Relaxed);
    }
    if com {
        unsafe { CoUninitialize() };
    }
}

struct Stream {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: HANDLE,
}

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
            let _ = CloseHandle(self.event);
        }
    }
}

impl Stream {
    fn run(&self, peak: &AtomicU32, stop: &AtomicBool) -> Result<()> {
        let mut window_start = Instant::now();
        let mut window_peak = 0f32;
        while !stop.load(Ordering::Relaxed) {
            // Wakes on new data; the timeout keeps stop responsive when the app is idle.
            let woke = unsafe { WaitForSingleObject(self.event, 50) } == WAIT_OBJECT_0;
            if woke {
                window_peak = window_peak.max(self.drain()?);
            }
            if window_start.elapsed() >= WINDOW {
                peak.store(window_peak.min(1.0).to_bits(), Ordering::Relaxed);
                window_peak = 0.0;
                window_start = Instant::now();
            }
        }
        Ok(())
    }

    /// Reads every pending packet and returns the highest absolute sample.
    fn drain(&self) -> Result<f32> {
        let mut highest = 0f32;
        loop {
            let frames_ready = unsafe { self.capture.GetNextPacketSize()? };
            if frames_ready == 0 {
                return Ok(highest);
            }
            let mut data = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            unsafe {
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?
            };
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 == 0 && !data.is_null() {
                let count = frames as usize * usize::from(CHANNELS);
                let samples = unsafe { std::slice::from_raw_parts(data as *const f32, count) };
                highest = samples.iter().fold(highest, |m, s| m.max(s.abs()));
            }
            unsafe { self.capture.ReleaseBuffer(frames)? };
        }
    }
}

fn open_stream(pid: u32) -> Result<Stream> {
    let client = activate_loopback_client(pid)?;

    let block_align = CHANNELS * 4;
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
        nChannels: CHANNELS,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * u32::from(block_align),
        nBlockAlign: block_align,
        wBitsPerSample: 32,
        cbSize: 0,
    };
    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK
                | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
            BUFFER_DURATION,
            0,
            &format,
            None,
        )?;
        let event = CreateEventW(None, false, false, None)?;
        let stream = Stream {
            capture: client.GetService()?,
            client,
            event,
        };
        stream.client.SetEventHandle(stream.event)?;
        stream.client.Start()?;
        Ok(stream)
    }
}

/// `PROPVARIANT` holding a `VT_BLOB` that points at the activation params.
/// Laid out like the real thing; we own the memory, so it is never cleared.
#[repr(C)]
struct BlobVariant {
    vt: u16,
    reserved: [u16; 3],
    blob: BLOB,
}

fn activate_loopback_client(pid: u32) -> Result<IAudioClient> {
    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    let variant = BlobVariant {
        vt: VT_BLOB.0,
        reserved: [0; 3],
        blob: BLOB {
            cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
            pBlobData: &mut params as *mut _ as *mut u8,
        },
    };

    let (done_tx, done_rx) = mpsc::channel();
    let handler: IActivateAudioInterfaceCompletionHandler = Completion(done_tx).into();
    let operation = unsafe {
        ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&variant as *const BlobVariant as *const PROPVARIANT),
            &handler,
        )?
    };
    done_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| Error::new(HRESULT(-1), "loopback activation timed out"))?;

    let mut result = HRESULT(0);
    let mut unknown: Option<IUnknown> = None;
    unsafe { operation.GetActivateResult(&mut result, &mut unknown)? };
    result.ok()?;
    unknown
        .ok_or_else(|| Error::new(HRESULT(-1), "no audio client returned"))?
        .cast()
}

/// Signals when Windows finishes activating the loopback client. Agile, as
/// `ActivateAudioInterfaceAsync` requires.
#[implement(IActivateAudioInterfaceCompletionHandler, IAgileObject)]
struct Completion(mpsc::Sender<()>);

impl IAgileObject_Impl for Completion_Impl {}

impl IActivateAudioInterfaceCompletionHandler_Impl for Completion_Impl {
    fn ActivateCompleted(
        &self,
        _operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> Result<()> {
        let _ = self.0.send(());
        Ok(())
    }
}
