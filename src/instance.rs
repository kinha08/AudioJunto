//! Kernel objects make startup atomic, including simultaneous launches.
use std::{
    io, ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0},
    System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject},
    UI::WindowsAndMessaging::{AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId},
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub struct Instance {
    _mutex: Handle,
    activate: Handle,
    stop: Arc<AtomicBool>,
    watcher: Option<thread::JoinHandle<()>>,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

impl Instance {
    pub fn acquire() -> io::Result<Option<Self>> {
        Self::acquire_named(
            "Local\\AudioJunto.529C9BD8.Instance",
            "Local\\AudioJunto.529C9BD8.Activate",
        )
    }
    fn acquire_named(mutex_name: &str, event_name: &str) -> io::Result<Option<Self>> {
        unsafe {
            // Both processes can create/open this auto-reset event. An activation
            // sent while the first window is still initializing remains pending.
            let event = CreateEventW(ptr::null(), 0, 0, wide(event_name).as_ptr());
            if event.is_null() {
                return Err(io::Error::last_os_error());
            }
            let activate = Handle(event);
            // Hold the event before acquiring the mutex so early activation
            // cannot disappear between the duplicate exiting and GUI startup.
            let mutex = CreateMutexW(ptr::null(), 0, wide(mutex_name).as_ptr());
            if mutex.is_null() {
                return Err(io::Error::last_os_error());
            }
            let exists = GetLastError() == ERROR_ALREADY_EXISTS;
            let mutex = Handle(mutex);
            if exists {
                let window = FindWindowW(ptr::null(), wide("AudioJunto").as_ptr());
                if !window.is_null() {
                    let mut pid = 0;
                    GetWindowThreadProcessId(window, &mut pid);
                    if pid != 0 {
                        AllowSetForegroundWindow(pid);
                    }
                }
                if SetEvent(activate.0) == 0 {
                    return Err(io::Error::last_os_error());
                }
                return Ok(None);
            }
            Ok(Some(Self {
                _mutex: mutex,
                activate,
                stop: Arc::new(AtomicBool::new(false)),
                watcher: None,
            }))
        }
    }
    #[cfg(test)]
    fn take_activation(&self) -> bool {
        unsafe { WaitForSingleObject(self.activate.0, 0) == WAIT_OBJECT_0 }
    }
    pub fn watch(&mut self, ctx: eframe::egui::Context, wake: crate::wake::NativeWake) {
        let stop = self.stop.clone();
        // Synchronization handles may be waited on from another thread. Drop
        // joins this worker before closing the handle, so it cannot outlive it.
        let event = self.activate.0 as usize;
        self.watcher = Some(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if unsafe { WaitForSingleObject(event as HANDLE, 100) } == WAIT_OBJECT_0 {
                    use eframe::egui::ViewportCommand;
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                    wake.wake(&ctx);
                }
            }
        }));
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simultaneous_launches_only_admit_one_primary() {
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let mut workers = Vec::new();
        for _ in 0..8 {
            let barrier = barrier.clone();
            workers.push(thread::spawn(move || {
                let name = format!("Local\\AudioJunto.Race.{}", std::process::id());
                barrier.wait();
                let instance = Instance::acquire_named(&name, &format!("{name}.Activate")).unwrap();
                let primary = instance.is_some();
                barrier.wait();
                drop(instance);
                primary
            }));
        }
        assert_eq!(
            workers
                .into_iter()
                .map(|w| w.join().unwrap() as usize)
                .sum::<usize>(),
            1
        );
    }
    #[test]
    fn duplicate_exits_and_signals_primary_then_relaunch_works() {
        let name = format!("Local\\AudioJunto.Test.{}", std::process::id());
        let event = format!("{name}.Activate");
        let first = Instance::acquire_named(&name, &event).unwrap().unwrap();
        assert!(!first.take_activation());
        assert!(Instance::acquire_named(&name, &event).unwrap().is_none());
        assert!(first.take_activation());
        assert!(!first.take_activation());
        drop(first);
        assert!(Instance::acquire_named(&name, &event).unwrap().is_some());
    }
}
