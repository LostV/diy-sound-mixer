//! Stress test for the Windows Audio service (Audiosrv): repeats one Core
//! Audio call many times so its memory can be watched from outside.
//!
//!     cargo run -p mixer-wasapi --example probe -- scan <count>
//!     cargo run -p mixer-wasapi --example probe -- volume <process> <count>
//!     cargo run -p mixer-wasapi --example probe -- read <process> <count>
//!     cargo run -p mixer-wasapi --example probe -- drag <process,process> <steps>
//!     cargo run -p mixer-wasapi --example probe -- backend <sessions|devices|capture|peaks|all> <count>
//!     cargo run -p mixer-wasapi --example probe -- hold <seconds> [scp]
//!     cargo run -p mixer-wasapi --example probe -- call <name> <seconds> [fresh]
//!
//! `backend` and `hold` drive the mixer's own backend, flat out or at the
//! app's pace; `call` repeats one Core Audio call on every session. Watch the
//! probe's own private memory as well as Audiosrv's.
//!
//! `volume` nudges every session of <process> by 1 % and back, then restores
//! the volume it found. `drag` does the same through the mixer's own backend,
//! like a fader drag on a channel holding those processes, so idle sessions
//! are deferred as in the app. It ends on the volume of the first process.

use windows::{
    Win32::{
        Foundation::CloseHandle,
        Media::Audio::{
            DEVICE_STATE_ACTIVE, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
            ISimpleAudioVolume, MMDeviceEnumerator, eRender,
        },
        System::{
            Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    },
    core::{Interface, PWSTR, Result},
};

use mixer_core::AudioBackend;
use mixer_wasapi::WasapiBackend;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let managers = managers(&enumerator)?;

    match args.first().map(String::as_str) {
        Some("scan") => {
            let count: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
            for _ in 0..count {
                for m in &managers {
                    let list = unsafe { m.GetSessionEnumerator()? };
                    for i in 0..unsafe { list.GetCount()? } {
                        let _ = unsafe { list.GetSession(i)? };
                    }
                }
            }
            println!("scanned {count} times");
        }
        Some("volume") => {
            let wanted = args.get(1).expect("process name").to_lowercase();
            let count: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);
            let volumes = sessions_of(&managers, &wanted)?;
            println!("{} session(s) of {wanted}", volumes.len());
            for (volume, original) in &volumes {
                let other = if *original > 0.5 {
                    original - 0.01
                } else {
                    original + 0.01
                };
                for i in 0..count {
                    let v = if i % 2 == 0 { other } else { *original };
                    unsafe { volume.SetMasterVolume(v, std::ptr::null())? };
                }
                unsafe { volume.SetMasterVolume(*original, std::ptr::null())? };
            }
            println!("set volume {count} times per session");
        }
        Some("drag") => {
            let wanted: Vec<String> = args
                .get(1)
                .expect("process names")
                .to_lowercase()
                .split(',')
                .map(str::to_owned)
                .collect();
            let steps: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
            let Some((_, original)) = sessions_of(&managers, &wanted[0])?.into_iter().next() else {
                println!("{} has no session", wanted[0]);
                return Ok(());
            };
            let other = if original > 0.5 {
                original - 0.01
            } else {
                original + 0.01
            };
            let mut backend = WasapiBackend::new()?;
            let ids: Vec<String> = backend
                .scan_sessions()
                .into_iter()
                .filter(|s| wanted.contains(&s.process_name.to_lowercase()))
                .map(|s| s.id)
                .collect();
            println!("{} session(s), {steps} steps", ids.len());
            for step in 0..steps {
                let v = if step % 2 == 0 { other } else { original };
                for id in &ids {
                    backend.set_session_volume(id, v);
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            for id in &ids {
                backend.set_session_volume(id, original);
            }
            // Dropping the backend waits for the writer and flushes deferred writes.
            drop(backend);
            println!("done");
        }
        Some("backend") => {
            let what = args.get(1).map(String::as_str).unwrap_or("all");
            let count: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);
            let mut backend = WasapiBackend::new()?;
            let ids: Vec<String> = backend.scan_sessions().into_iter().map(|s| s.id).collect();
            for _ in 0..count {
                if matches!(what, "sessions" | "all") {
                    backend.scan_sessions();
                }
                if matches!(what, "devices" | "all") {
                    backend.output_devices();
                }
                if matches!(what, "capture" | "all") {
                    backend.capture_devices();
                    backend.scan_capture(None);
                }
                if matches!(what, "peaks" | "all") {
                    for id in &ids {
                        backend.session_peak(id);
                    }
                    backend.capture_peak();
                }
            }
            println!("backend {what} {count} times");
        }
        Some("call") => {
            // Repeats one call scan_sessions makes, on every session or device.
            let which = args.get(1).expect("call name").clone();
            let count: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10000);
            let mut sessions = Vec::new();
            for m in &managers {
                let list = unsafe { m.GetSessionEnumerator()? };
                for i in 0..unsafe { list.GetCount()? } {
                    sessions.push(unsafe { list.GetSession(i)? });
                }
            }
            // <count> is seconds here, so fast calls run as long as slow ones.
            let start = std::time::Instant::now();
            // fresh: enumerate again each round after letting go of the old
            // sessions; held: enumerate while still holding them.
            let mode = args.get(3).cloned().unwrap_or_default();
            while start.elapsed().as_secs() < u64::from(count) {
                if mode == "fresh" || mode == "held" {
                    if mode == "fresh" {
                        sessions.clear();
                    }
                    let mut next = Vec::new();
                    for m in &managers {
                        let list = unsafe { m.GetSessionEnumerator()? };
                        for i in 0..unsafe { list.GetCount()? } {
                            next.push(unsafe { list.GetSession(i)? });
                        }
                    }
                    sessions = next;
                }
                if which == "endpoints" {
                    let c = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)? };
                    for i in 0..unsafe { c.GetCount()? } {
                        let d = unsafe { c.Item(i)? };
                        let id = unsafe { d.GetId()? };
                        unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as _)) };
                    }
                    continue;
                }
                for s in &sessions {
                    let c2: IAudioSessionControl2 = s.cast()?;
                    match which.as_str() {
                        "state" => drop(unsafe { s.GetState() }),
                        "cast" => drop(c2),
                        "ident" => {
                            let p = unsafe { c2.GetSessionInstanceIdentifier()? };
                            unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as _)) };
                        }
                        "pid" => drop(unsafe { c2.GetProcessId() }),
                        "system" => drop(unsafe { c2.IsSystemSoundsSession() }),
                        "name" => {
                            if let Ok(p) = unsafe { c2.GetDisplayName() } {
                                unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as _)) };
                            }
                        }
                        "volume" => drop(c2.cast::<ISimpleAudioVolume>()?),
                        "meteri" => drop(
                            c2.cast::<windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation>()?,
                        ),
                        _ => {}
                    }
                }
            }
            println!("{which} x{count}");
        }
        Some("hold") => {
            // Runs the backend at the app's pace (refresh every 500 ms, meters
            // every 33 ms) for <seconds>.
            // <parts> picks what repeats: s(essions), c(apture), p(eaks); default all.
            let seconds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(180);
            let parts = args.get(2).cloned().unwrap_or_else(|| "scp".into());
            let mut backend = WasapiBackend::new()?;
            let start = std::time::Instant::now();
            let mut ids: Vec<String> = backend.scan_sessions().into_iter().map(|s| s.id).collect();
            let mut last_refresh = std::time::Instant::now();
            while start.elapsed().as_secs() < seconds {
                if last_refresh.elapsed().as_millis() >= 500 {
                    if parts.contains('s') {
                        ids = backend.scan_sessions().into_iter().map(|s| s.id).collect();
                        backend.output_devices();
                    }
                    if parts.contains('c') {
                        backend.capture_devices();
                        backend.scan_capture(None);
                    }
                    last_refresh = std::time::Instant::now();
                }
                if parts.contains('p') {
                    for id in &ids {
                        backend.session_peak(id);
                    }
                    backend.capture_peak();
                }
                std::thread::sleep(std::time::Duration::from_millis(33));
            }
            println!("held {seconds} s, {} session(s)", ids.len());
        }
        Some("read") => {
            let wanted = args.get(1).expect("process name").to_lowercase();
            let count: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);
            let volumes = sessions_of(&managers, &wanted)?;
            for (volume, _) in &volumes {
                for _ in 0..count {
                    let _ = unsafe { volume.GetMasterVolume()? };
                    let _ = unsafe { volume.GetMute()? };
                }
            }
            println!("read volume {count} times per session");
        }
        _ => eprintln!(
            "usage: probe scan <count> | probe volume <process> <count> | probe read <process> <count>"
        ),
    }
    Ok(())
}

fn managers(enumerator: &IMMDeviceEnumerator) -> Result<Vec<IAudioSessionManager2>> {
    let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)? };
    (0..unsafe { collection.GetCount()? })
        .map(|i| unsafe { collection.Item(i)?.Activate(CLSCTX_ALL, None) })
        .collect()
}

fn sessions_of(
    managers: &[IAudioSessionManager2],
    process: &str,
) -> Result<Vec<(ISimpleAudioVolume, f32)>> {
    let mut out = Vec::new();
    for m in managers {
        let list = unsafe { m.GetSessionEnumerator()? };
        for i in 0..unsafe { list.GetCount()? } {
            let control: IAudioSessionControl2 = unsafe { list.GetSession(i)? }.cast()?;
            let pid = unsafe { control.GetProcessId() }.unwrap_or(0);
            if image_name(pid).to_lowercase().ends_with(process) {
                let volume: ISimpleAudioVolume = control.cast()?;
                let original = unsafe { volume.GetMasterVolume()? };
                out.push((volume, original));
            }
        }
    }
    Ok(out)
}

fn image_name(pid: u32) -> String {
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return String::new();
    };
    let mut buf = [0u16; 512];
    let mut len = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    if ok.is_err() {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}
