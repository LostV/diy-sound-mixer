//! Session volume and mute, and the microphone's, applied on a thread of
//! their own.
//!
//! On some devices (seen with an Astro A50) Windows takes 70–130 ms per
//! `SetMasterVolume` call, and an app like Chrome has many sessions. Done on
//! the mixer thread that froze the meters and made a fader drag play back
//! for seconds. Here only the newest value per session is kept, so however
//! slow Windows is, it ends on the value the user let go at.
//!
//! Each write can also cost ~60 MB of kernel memory (ETW notification data,
//! freed over minutes; seen on a Windows 11 machine with an Astro A50), so
//! values Windows already has are never written again.
//!
//! The microphone (an endpoint volume) is slower still: ~0.4 s per write on
//! the A50 Mic, so turning the mic knob froze the meters and the screen for
//! seconds.

use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

use windows::Win32::{
    Media::Audio::{Endpoints::IAudioEndpointVolume, ISimpleAudioVolume},
    System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
};

/// A session's or the microphone's volume control, sent to the writer thread.
enum Control {
    Session(ISimpleAudioVolume),
    Endpoint(IAudioEndpointVolume),
}

impl Control {
    fn volume(&self) -> windows::core::Result<f32> {
        match self {
            Self::Session(c) => unsafe { c.GetMasterVolume() },
            Self::Endpoint(c) => unsafe { c.GetMasterVolumeLevelScalar() },
        }
    }

    fn set_volume(&self, volume: f32) -> windows::core::Result<()> {
        match self {
            Self::Session(c) => unsafe { c.SetMasterVolume(volume, std::ptr::null()) },
            Self::Endpoint(c) => unsafe { c.SetMasterVolumeLevelScalar(volume, std::ptr::null()) },
        }
    }

    fn muted(&self) -> windows::core::Result<bool> {
        match self {
            Self::Session(c) => unsafe { c.GetMute() }.map(|m| m.as_bool()),
            Self::Endpoint(c) => unsafe { c.GetMute() }.map(|m| m.as_bool()),
        }
    }

    fn set_muted(&self, muted: bool) -> windows::core::Result<()> {
        match self {
            Self::Session(c) => unsafe { c.SetMute(muted, std::ptr::null()) },
            Self::Endpoint(c) => unsafe { c.SetMute(muted, std::ptr::null()) },
        }
    }
}

// SAFETY: windows-rs doesn't mark audio interfaces Send, but both the mixer
// thread and the writer thread are in the multithreaded apartment, where COM
// interface pointers may be used from any thread.
unsafe impl Send for Control {}

struct Pending {
    control: Control,
    volume: Option<f32>,
    muted: Option<bool>,
}

#[derive(Default)]
struct State {
    pending: HashMap<String, Pending>,
    stop: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

pub(crate) struct SessionWriter {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl SessionWriter {
    pub(crate) fn new() -> Self {
        let shared = Arc::<Shared>::default();
        let worker = shared.clone();
        let thread = std::thread::Builder::new()
            .name("session-writer".into())
            .spawn(move || run(&worker))
            .map_err(|e| log::error!("starting session writer: {e}"))
            .ok();
        Self { shared, thread }
    }

    pub(crate) fn set_volume(&self, session_id: &str, control: &ISimpleAudioVolume, volume: f32) {
        self.queue(
            session_id,
            || Control::Session(control.clone()),
            |p| p.volume = Some(volume),
        );
    }

    pub(crate) fn set_mute(&self, session_id: &str, control: &ISimpleAudioVolume, muted: bool) {
        self.queue(
            session_id,
            || Control::Session(control.clone()),
            |p| p.muted = Some(muted),
        );
    }

    /// The microphone; `device_id` keeps it apart from the sessions.
    pub(crate) fn set_endpoint_volume(
        &self,
        device_id: &str,
        control: &IAudioEndpointVolume,
        volume: f32,
    ) {
        self.queue(
            device_id,
            || Control::Endpoint(control.clone()),
            |p| p.volume = Some(volume),
        );
    }

    pub(crate) fn set_endpoint_mute(
        &self,
        device_id: &str,
        control: &IAudioEndpointVolume,
        muted: bool,
    ) {
        self.queue(
            device_id,
            || Control::Endpoint(control.clone()),
            |p| p.muted = Some(muted),
        );
    }

    fn queue(
        &self,
        id: &str,
        control: impl FnOnce() -> Control,
        update: impl FnOnce(&mut Pending),
    ) {
        let mut state = self.shared.state.lock().expect("writer lock");
        let pending = state
            .pending
            .entry(id.to_owned())
            .or_insert_with(|| Pending {
                control: control(),
                volume: None,
                muted: None,
            });
        update(pending);
        self.shared.wake.notify_one();
    }
}

impl Drop for SessionWriter {
    /// Writes what is still pending (the volumes restored on exit) first.
    fn drop(&mut self) {
        self.shared.state.lock().expect("writer lock").stop = true;
        self.shared.wake.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(shared: &Shared) {
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    loop {
        let batch = {
            let mut state = shared.state.lock().expect("writer lock");
            while state.pending.is_empty() && !state.stop {
                state = shared.wake.wait(state).expect("writer lock");
            }
            if state.pending.is_empty() {
                break;
            }
            std::mem::take(&mut state.pending)
        };
        for (id, pending) in batch {
            let control = &pending.control;
            // Reading is free; writing is the expensive part, so skip it when
            // Windows already has the value (e.g. after a restart).
            if let Some(volume) = pending.volume
                && control
                    .volume()
                    .ok()
                    .is_none_or(|v| (v - volume).abs() > 0.001)
                && let Err(e) = control.set_volume(volume)
            {
                log::debug!("set volume on {id}: {e}");
            }
            if let Some(muted) = pending.muted
                && control.muted().ok().is_none_or(|m| m != muted)
                && let Err(e) = control.set_muted(muted)
            {
                log::debug!("set mute on {id}: {e}");
            }
        }
    }
    if com {
        unsafe { CoUninitialize() };
    }
}
