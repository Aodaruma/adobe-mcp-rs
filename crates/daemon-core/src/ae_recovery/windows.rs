//! Read-only Windows process/window gates and the native AE launcher.
use super::*;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::{ffi::OsStrExt, process::CommandExt};
use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_NO_MORE_FILES, FILETIME, HANDLE, HWND, INVALID_HANDLE_VALUE,
    WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetProcessTimes, OpenProcess, QueryFullProcessImageNameW,
    WaitForSingleObject, CREATE_NO_WINDOW, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible, SendMessageTimeoutW,
    SMTO_ABORTIFHUNG, WM_NULL,
};

pub(super) struct WindowsHost;
impl NativeHost for WindowsHost {
    fn process(&mut self) -> Result<Option<AeProcess>> {
        sole_ae_process()
    }
    fn windows_ready(&mut self, pid: u32) -> bool {
        window_accepts_scripts(pid)
    }
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
    fn launch(&mut self, executable: &Path, script: &Path) -> Result<()> {
        Command::new(executable)
            .arg("-r")
            .arg(script)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to launch AE scheduler recovery")?;
        Ok(())
    }
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

fn sole_ae_process() -> Result<Option<AeProcess>> {
    Ok(sole_ae_process_with_handle()?.map(|(process, _handle)| process))
}

pub(crate) fn track_host_exit(instance: &HostInstance) -> Option<Box<dyn Fn() -> bool + Send>> {
    if !supports_process_lifetime_tracking(instance) {
        return None;
    }
    let (process, handle) = sole_ae_process_with_handle().ok()??;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    if !instance_belongs_to_process(instance, &process, now_ms) {
        return None;
    }
    // Keeping the exact process object open prevents PID reuse from changing
    // the target. WAIT_FAILED and WAIT_TIMEOUT must both retain the request.
    Some(Box::new(move || unsafe {
        WaitForSingleObject(handle.as_raw_handle(), 0) == WAIT_OBJECT_0
    }))
}

fn sole_ae_process_with_handle() -> Result<Option<(AeProcess, OwnedHandle)>> {
    unsafe {
        let snapshot = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0));
        if snapshot.0 == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut pids = Vec::new();
        let mut found = Process32FirstW(snapshot.0, &mut entry);
        while found != 0 {
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case("AfterFX.exe")
            {
                pids.push(entry.th32ProcessID);
            }
            found = Process32NextW(snapshot.0, &mut entry);
        }
        if GetLastError() != ERROR_NO_MORE_FILES {
            return Ok(None);
        }
        if pids.len() != 1 {
            return Ok(None);
        }
        let pid = pids[0];
        let (mut own_session, mut ae_session) = (0, 0);
        if ProcessIdToSessionId(GetCurrentProcessId(), &mut own_session) == 0
            || ProcessIdToSessionId(pid, &mut ae_session) == 0
            || own_session != ae_session
        {
            return Ok(None);
        }
        let process_handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        );
        if process_handle.is_null() {
            return Ok(None);
        }
        let process = OwnedHandle::from_raw_handle(process_handle);
        let mut path = vec![0u16; 32768];
        let mut size = path.len() as u32;
        if QueryFullProcessImageNameW(process.as_raw_handle(), 0, path.as_mut_ptr(), &mut size) == 0
        {
            return Ok(None);
        }
        let executable = PathBuf::from(String::from_utf16_lossy(&path[..size as usize]));
        // Only the major version verified on a real host is enabled.
        if executable_major(&executable)? != 25 {
            return Ok(None);
        }
        let (mut created, mut exited, mut kernel, mut user): (
            FILETIME,
            FILETIME,
            FILETIME,
            FILETIME,
        ) = std::mem::zeroed();
        if GetProcessTimes(
            process.as_raw_handle(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        ) == 0
        {
            return Ok(None);
        }
        let ticks = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
        let started_ms = (ticks / 10_000).saturating_sub(11_644_473_600_000);
        Ok(Some((
            AeProcess {
                pid,
                started_ms,
                executable,
            },
            process,
        )))
    }
}

fn executable_major(path: &Path) -> Result<u32> {
    let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let len = GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut());
        if len == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // Align the buffer for VS_FIXEDFILEINFO.
        let mut data = vec![0u32; (len as usize).div_ceil(4)];
        if GetFileVersionInfoW(wide.as_ptr(), 0, len, data.as_mut_ptr().cast()) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let (mut value, mut size) = (std::ptr::null_mut(), 0u32);
        if VerQueryValueW(
            data.as_ptr().cast(),
            [92u16, 0].as_ptr(),
            &mut value,
            &mut size,
        ) == 0
            || size < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
        {
            anyhow::bail!("missing AE file version");
        }
        Ok((*(value as *const VS_FIXEDFILEINFO)).dwFileVersionMS >> 16)
    }
}

#[derive(Default)]
struct WindowCheck {
    pid: u32,
    visible: bool,
    blocked: bool,
}

unsafe extern "system" fn inspect_window(window: HWND, param: isize) -> i32 {
    let state = &mut *(param as *mut WindowCheck);
    let mut pid = 0;
    GetWindowThreadProcessId(window, &mut pid);
    if pid != state.pid || IsWindowVisible(window) == 0 {
        return 1;
    }
    state.visible = true;
    let mut class = [0u16; 128];
    let len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
    let is_dialog = len > 0 && String::from_utf16_lossy(&class[..len as usize]) == "#32770";
    // Disabled main windows reveal native and ScriptUI modal dialogs. Common file
    // dialogs are also blocked explicitly. Never dismiss or interact with a dialog.
    if len <= 0
        || IsWindowEnabled(window) == 0
        || is_dialog
        || SendMessageTimeoutW(
            window,
            WM_NULL,
            0,
            0,
            SMTO_ABORTIFHUNG,
            200,
            std::ptr::null_mut(),
        ) == 0
    {
        state.blocked = true;
        return 0;
    }
    1
}

fn window_accepts_scripts(pid: u32) -> bool {
    let mut state = WindowCheck {
        pid,
        ..Default::default()
    };
    let ok = unsafe {
        EnumWindows(
            Some(inspect_window),
            (&mut state as *mut WindowCheck) as isize,
        )
    };
    ok != 0 && state.visible && !state.blocked
}
