use std::{collections::HashMap, ffi::c_void, path::Path};

use windows::{
    Win32::{
        Foundation::CloseHandle,
        Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
        System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    },
    core::{HSTRING, PCWSTR, PWSTR},
};

#[derive(Debug, Clone, Default)]
pub struct ProcessInfo {
    /// e.g. `discord.exe`; empty for pid 0.
    pub name: String,
    pub path: Option<String>,
    /// FileDescription from the exe's version resource, e.g. `Discord`.
    pub description: Option<String>,
}

/// Looking up paths and version resources is slow-ish, so cache per pid.
#[derive(Default)]
pub struct ProcessCache {
    by_pid: HashMap<u32, ProcessInfo>,
}

impl ProcessCache {
    pub fn get(&mut self, pid: u32) -> &ProcessInfo {
        self.by_pid.entry(pid).or_insert_with(|| lookup(pid))
    }

    /// Forgets pids that no longer have sessions (pids get reused).
    pub fn retain(&mut self, live: impl Iterator<Item = u32>) {
        let live: std::collections::HashSet<u32> = live.collect();
        self.by_pid.retain(|pid, _| live.contains(pid));
    }
}

fn lookup(pid: u32) -> ProcessInfo {
    if pid == 0 {
        return ProcessInfo::default();
    }
    match image_path(pid) {
        Some(path) => ProcessInfo {
            name: Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            description: file_description(&path),
            path: Some(path),
        },
        None => ProcessInfo {
            name: format!("pid-{pid}"),
            ..Default::default()
        },
    }
}

fn image_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn file_description(path: &str) -> Option<String> {
    let path = HSTRING::from(path);
    unsafe {
        let size = GetFileVersionInfoSizeW(&path, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&path, None, size, data.as_mut_ptr() as *mut c_void).ok()?;

        // First language/codepage pair from the translation table.
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let translation = HSTRING::from(r"\VarFileInfo\Translation");
        if !VerQueryValueW(
            data.as_ptr() as *const c_void,
            PCWSTR(translation.as_ptr()),
            &mut ptr,
            &mut len,
        )
        .as_bool()
            || len < 4
        {
            return None;
        }
        let pair = std::slice::from_raw_parts(ptr as *const u16, 2);
        let key = HSTRING::from(format!(
            r"\StringFileInfo\{:04x}{:04x}\FileDescription",
            pair[0], pair[1]
        ));
        if !VerQueryValueW(
            data.as_ptr() as *const c_void,
            PCWSTR(key.as_ptr()),
            &mut ptr,
            &mut len,
        )
        .as_bool()
            || len == 0
        {
            return None;
        }
        let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let text = String::from_utf16_lossy(text)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        (!text.is_empty()).then_some(text)
    }
}
