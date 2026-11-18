use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{PostMessageW, WM_PAINT},
};

/// Wake the native event loop even while Windows suppresses normal redraws of
/// a hidden window. A queued paint is handled after the tray menu closes.
#[derive(Clone, Copy)]
pub struct NativeWake(usize);
impl NativeWake {
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        match cc.window_handle()?.as_raw() {
            RawWindowHandle::Win32(handle) => Ok(Self(handle.hwnd.get() as usize)),
            _ => anyhow::bail!("Janela Windows indisponível"),
        }
    }
    pub fn wake(self, ctx: &eframe::egui::Context) {
        ctx.request_repaint();
        // Posting avoids reentrant App::update calls inside a native menu callback.
        unsafe {
            PostMessageW(self.0 as HWND, WM_PAINT, 0, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    #[test]
    fn wake_reaches_hidden_native_window() {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                10,
                10,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!hwnd.is_null());
            assert_eq!(IsWindowVisible(hwnd), 0);
            NativeWake(hwnd as usize).wake(&eframe::egui::Context::default());
            let mut msg: MSG = std::mem::zeroed();
            assert_ne!(
                PeekMessageW(&mut msg, hwnd, WM_PAINT, WM_PAINT, PM_REMOVE),
                0
            );
            assert_eq!(msg.hwnd, hwnd);
            assert_eq!(msg.message, WM_PAINT);
            assert_eq!(IsWindowVisible(hwnd), 0);
            DestroyWindow(hwnd);
        }
    }
}
