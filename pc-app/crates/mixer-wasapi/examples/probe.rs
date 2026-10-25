//! Stress test for the Windows Audio service (Audiosrv): repeats one Core
//! Audio call many times so its memory can be watched from outside.
//!
//!     cargo run -p mixer-wasapi --example probe -- scan <count>
//!     cargo run -p mixer-wasapi --example probe -- volume <process> <count>
//!     cargo run -p mixer-wasapi --example probe -- read <process> <count>
//!     cargo run -p mixer-wasapi --example probe -- drag <process,process> <steps>
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
