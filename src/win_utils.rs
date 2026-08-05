use std::ffi::c_void;
use std::mem;

use windows::core::{Interface, PCWSTR};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{IShellLinkW, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetIconInfo, HICON, ICONINFO, LoadImageW, IMAGE_ICON, IMAGE_FLAGS,
};
use windui::prelude::Image;

/// 字符串转 UTF-16 含结尾 NUL（Win32 API 输入用）。
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_to_string(buf: &[u16], len: usize) -> String {
    let end = buf.iter().take(len).position(|&c| c == 0).unwrap_or(len);
    String::from_utf16_lossy(&buf[..end]).trim_end().to_string()
}

/// 从文件/目录提取图标（SHGetFileInfoW → HICON → DIB → RGBA）。
/// 失败返回 None，调用方回退占位。size 为期望尺寸（提取 32px 后由上层缩放绘制）。
pub fn extract_icon(path: &str, size: u32) -> Option<Image> {
    extract_icon_rgba(path, size).and_then(|(w, h, buf)| Image::from_rgba(w, h, &buf).ok())
}

/// 后台线程版图标提取：返回 RGBA 像素（纯数据，可跨线程 Send），不阻塞 UI 线程。
pub fn extract_icon_rgba(path: &str, _size: u32) -> Option<(u32, u32, Vec<u8>)> {
    let wide = to_wide(path);
    let mut info: SHFILEINFOW = unsafe { mem::zeroed() };
    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut info as *mut SHFILEINFOW),
            mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON,
        )
    };
    if ok == 0 || info.hIcon.is_invalid() {
        return None;
    }
    let icon = info.hIcon;
    let bytes = hicon_to_bytes(icon);
    unsafe {
        let _ = DestroyIcon(icon);
    }
    bytes
}

/// HICON → RGBA 像素（GetIconInfo → 优先彩色位图，无彩色时退化掩码位图）。
fn hicon_to_bytes(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut iconinfo = ICONINFO::default();
    if unsafe { GetIconInfo(icon, &mut iconinfo) }.is_ok() {
        let has_color = !iconinfo.hbmColor.is_invalid();
        let hbm = if has_color {
            iconinfo.hbmColor
        } else {
            iconinfo.hbmMask
        };
        let out = dib_rgba_bytes(hbm);
        unsafe {
            let _ = DeleteObject(HGDIOBJ(iconinfo.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(iconinfo.hbmMask.0));
        }
        out
    } else {
        None
    }
}

fn dib_rgba_bytes(hbm: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let mut bmp: BITMAP = unsafe { mem::zeroed() };
    let got = unsafe {
        GetObjectW(
            HGDIOBJ(hbm.0),
            mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut c_void),
        )
    };
    if got == 0 {
        return None;
    }
    let w = bmp.bmWidth as u32;
    let h = bmp.bmHeight as u32;
    if w == 0 || h == 0 || w > 1024 || h > 1024 {
        return None;
    }
    let hdc = unsafe { CreateCompatibleDC(None) };
    if hdc.is_invalid() {
        return None;
    }
    let mut bmi: BITMAPINFO = unsafe { mem::zeroed() };
    bmi.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w as i32;
    bmi.bmiHeader.biHeight = -(h as i32);
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB.0;
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let lines = unsafe {
        GetDIBits(
            hdc,
            hbm,
            0,
            h,
            Some(buf.as_mut_ptr() as *mut c_void),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    unsafe {
        let _ = DeleteDC(hdc);
    }
    if lines == 0 {
        return None;
    }
    // BGRA → RGBA；纯色位图 alpha 可能全零，此时视作不透明。
    let mut any_alpha = false;
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2);
        if px[3] != 0 {
            any_alpha = true;
        }
    }
    if !any_alpha {
        for px in buf.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }
    Some((w, h, buf))
}

/// 从当前 exe 内嵌图标资源提取 RGBA 像素（托盘用）。
/// 资源不存在或加载失败返回 None，调用方回退占位。
pub fn load_res_icon_rgba(res_id: u16, size: u32) -> Option<Vec<u8>> {
    let hinst = unsafe { GetModuleHandleW(None) }.ok()?;
    let handle = unsafe {
        LoadImageW(
            Some(HINSTANCE(hinst.0)),
            PCWSTR(usize::from(res_id) as *const u16),
            IMAGE_ICON,
            size as i32,
            size as i32,
            IMAGE_FLAGS(0),
        )
        .ok()?
    };
    let icon = HICON(handle.0);
    let bytes = hicon_to_bytes(icon).map(|(_, _, b)| b);
    unsafe {
        let _ = DestroyIcon(icon);
    }
    bytes
}

/// 解析 .lnk 快捷方式目标，返回 (目标, 参数, 工作目录)。
pub fn parse_lnk(path: &str) -> Option<(String, String, String)> {
    // COM 必须在本线程初始化，否则 CoCreateInstance 返回未注册。
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }
    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let persist: IPersistFile = link.cast().ok()?;
    let text = to_wide(path);
    if unsafe { persist.Load(PCWSTR(text.as_ptr()), STGM_READ) }.is_err() {
        return None;
    }
    let mut path_buf = [0u16; 1024];
    let mut args_buf = [0u16; 1024];
    let mut dir_buf = [0u16; 1024];
    unsafe { link.GetPath(&mut path_buf, std::ptr::null_mut(), 0) }
        .ok()?;
    let _ = unsafe { link.GetArguments(&mut args_buf) }.ok();
    let _ = unsafe { link.GetWorkingDirectory(&mut dir_buf) }.ok();
    Some((
        wide_to_string(&path_buf, path_buf.len()),
        wide_to_string(&args_buf, args_buf.len()),
        wide_to_string(&dir_buf, dir_buf.len()),
    ))
}

/// 开机自启：写入/删除 HKCU 启动 Run 项。exe 为当前程序路径。
pub fn set_autostart(enabled: bool, exe: &str) -> Result<(), String> {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    let key = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let name = to_wide("LuckyLauncher");
    let mut hkey = HKEY::default();
    let rc = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            Some(0),
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut hkey,
            None,
        )
    };
    if rc.0 != 0 || hkey.is_invalid() {
        return Err("无法打开注册表 Run 项".to_string());
    }
    let result = if enabled {
        let value = to_wide(&format!("\"{exe}\" --minimized"));
        let bytes = value
            .iter()
            .flat_map(|c| c.to_le_bytes())
            .collect::<Vec<u8>>();
        unsafe {
            RegSetValueExW(
                hkey,
                PCWSTR(name.as_ptr()),
                Some(0),
                REG_SZ,
                Some(&bytes),
            )
        }
    } else {
        unsafe { RegDeleteValueW(hkey, PCWSTR(name.as_ptr())) }
    };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    // 关闭自启时删除一个不存在的值会返回 ERROR_FILE_NOT_FOUND，视为成功
    //（目标已达成，无需把它当成失败报错）。
    let ok = if enabled {
        result.0 == 0
    } else {
        result.0 == 0 || result == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND
    };
    if ok {
        Ok(())
    } else {
        Err("设置开机自启失败".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_wide_has_nul() {
        let w = to_wide("ab");
        assert_eq!(w, vec![97, 98, 0]);
    }

    #[test]
    fn wide_to_string_strips_nul() {
        let buf = [104u16, 105, 0, 0];
        assert_eq!(wide_to_string(&buf, buf.len()), "hi");
    }

    #[test]
    fn non_existing_icon_is_none() {
        assert!(extract_icon(r"C:\DefinitelyNotExist\__.exe", 32).is_none());
    }

    #[test]
    fn autostart_delete_missing_value_is_ok() {
        // 关闭自启时若 Run 键里本就没有该值，删除会返回 ERROR_FILE_NOT_FOUND，
        // 不应被当成失败。连续两次关闭都应为 Ok（第二次值已不存在）。
        assert!(set_autostart(false, "C:/x/app.exe").is_ok());
        assert!(set_autostart(false, "C:/x/app.exe").is_ok());
    }
}
