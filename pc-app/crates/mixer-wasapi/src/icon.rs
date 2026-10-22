//! App icons for the UI: an executable's main icon as PNG bytes.

use std::ffi::c_void;

use windows::{
    Win32::{
        Graphics::Gdi::{
            BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS,
            DeleteDC, DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC,
        },
        UI::{
            Shell::SHDefExtractIconW,
            WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO},
        },
    },
    core::HSTRING,
};

/// The first icon in `exe_path`, rendered at `size`×`size`, encoded as PNG.
/// `None` if the file has no icon.
pub fn icon_png(exe_path: &str, size: u32) -> Option<Vec<u8>> {
    let mut icon = HICON::default();
    // LOWORD = large icon size, HIWORD = small icon size (unused).
    let sizes = size | (16 << 16);
    unsafe { SHDefExtractIconW(&HSTRING::from(exe_path), 0, 0, Some(&mut icon), None, sizes) }
        .ok()
        .ok()?;
    let rgba = icon_rgba(icon);
    unsafe {
        let _ = DestroyIcon(icon);
    }
    let (width, height, pixels) = rgba?;
    encode_png(width, height, &pixels)
}

fn icon_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut info = ICONINFO::default();
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    let dc = unsafe { CreateCompatibleDC(None) };
    let result = (|| {
        let (width, height) = bitmap_size(info.hbmColor)?;
        let mut pixels = read_bgra(dc, info.hbmColor, width, height)?;
        // Old icons have no alpha channel; their transparency is in the mask.
        if pixels.chunks_exact(4).all(|p| p[3] == 0) {
            let mask = read_bgra(dc, info.hbmMask, width, height)?;
            for (pixel, m) in pixels.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
                pixel[3] = if m[0] == 0 { 255 } else { 0 };
            }
        }
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2); // BGRA → RGBA
        }
        Some((width, height, pixels))
    })();
    unsafe {
        let _ = DeleteDC(dc);
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
    }
    result
}

fn bitmap_size(bitmap: HBITMAP) -> Option<(u32, u32)> {
    if bitmap.is_invalid() {
        return None;
    }
    let mut bm = BITMAP::default();
    let read = unsafe {
        GetObjectW(
            bitmap.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut c_void),
        )
    };
    (read != 0 && bm.bmWidth > 0 && bm.bmHeight > 0)
        .then_some((bm.bmWidth as u32, bm.bmHeight as u32))
}

/// Top-down 32-bit BGRA pixels of `bitmap`.
fn read_bgra(dc: HDC, bitmap: HBITMAP, width: u32, height: u32) -> Option<Vec<u8>> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let lines = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut c_void),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    (lines == height as i32).then_some(pixels)
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}
