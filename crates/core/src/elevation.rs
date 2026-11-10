//! Administrator rights: do we have them, and asking for them.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::util::wide;
use crate::{Error, Result};

/// True when this process is running elevated (as administrator).
pub fn is_elevated() -> bool {
    let mut token = HANDLE::default();
    // SAFETY: GetCurrentProcess returns a pseudo handle that needs no closing.
    // OpenProcessToken writes a real handle into `token`, which we close below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    // SAFETY: the buffer is a TOKEN_ELEVATION and we pass its exact size, so
    // Windows cannot write past it.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    }
    .is_ok();
    // SAFETY: `token` came from OpenProcessToken above and is closed once.
    let _ = unsafe { CloseHandle(token) };
    ok && elevation.TokenIsElevated != 0
}

/// Start a second copy of this program elevated, through the normal Windows
/// UAC prompt. Returns Ok once the new copy has started: the caller should
/// then exit. `Error::Cancelled` if the user says no to the prompt.
pub fn relaunch_elevated() -> Result<()> {
    let exe = std::env::current_exe()?;
    let exe_w = wide(&exe.to_string_lossy());
    let verb = wide("runas");
    // SAFETY: both strings are NUL-terminated buffers that outlive the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a fake HINSTANCE: anything above 32 is success.
    if result.0 as usize > 32 {
        return Ok(());
    }
    // SAFETY: reads the calling thread's last error, no pointers involved.
    let err = unsafe { GetLastError() };
    if err == ERROR_CANCELLED {
        return Err(Error::Cancelled);
    }
    Err(Error::msg(format!(
        "Windows could not restart Quadrant as administrator (error {}).",
        err.0
    )))
}

#[cfg(test)]
mod tests {
    // relaunch_elevated is never called here: it would raise a UAC prompt.
    #[test]
    fn elevation_check_does_not_panic() {
        let _ = super::is_elevated();
    }
}
