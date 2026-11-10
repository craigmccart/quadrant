//! What this PC is: hardware, Windows version, memory, uptime.

use winreg::enums::{KEY_READ, KEY_WOW64_64KEY};
use winreg::{RegKey, HKLM};

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{
    GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows::Win32::System::Threading::GetSystemTimes;

use crate::model::{LiveStats, SystemOverview};

const GPU_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
const BASIC_DISPLAY: &str = "Microsoft Basic Display Adapter";

/// A snapshot of the machine. Never fails: anything Windows will not tell us
/// comes back as an empty string or zero. Drives come from
/// `crate::diskmap::drives()`.
pub fn overview() -> SystemOverview {
    let cv = open_hklm(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let product = cv.as_ref().map(|k| reg_str(k, "ProductName")).unwrap_or_default();
    let build = cv.as_ref().map(|k| reg_str(k, "CurrentBuildNumber")).unwrap_or_default();
    let ubr = cv.as_ref().and_then(|k| k.get_value::<u32, _>("UBR").ok());
    let display_version = cv.as_ref().map(|k| reg_str(k, "DisplayVersion")).unwrap_or_default();

    let bios = open_hklm(r"HARDWARE\DESCRIPTION\System\BIOS");
    let cpu = open_hklm(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0");

    let (ram_total, ram_used) = memory();
    let uptime_secs = uptime_secs();

    SystemOverview {
        host_name: std::env::var("COMPUTERNAME").unwrap_or_default(),
        user_name: std::env::var("USERNAME").unwrap_or_default(),
        os_name: os_name(&product, &build),
        os_version: display_version,
        os_build: os_build(&build, ubr),
        manufacturer: bios.as_ref().map(|k| reg_str(k, "SystemManufacturer")).unwrap_or_default(),
        model: bios.as_ref().map(|k| reg_str(k, "SystemProductName")).unwrap_or_default(),
        cpu_name: cpu.as_ref().map(|k| reg_str(k, "ProcessorNameString")).unwrap_or_default(),
        cpu_cores_physical: sysinfo::System::physical_core_count().map(|n| n as u32).unwrap_or(0),
        cpu_cores_logical: std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(0),
        gpus: gpus(),
        ram_total_bytes: ram_total,
        ram_used_bytes: ram_used,
        uptime_secs,
        boot_time: crate::util::now_unix() - uptime_secs as i64,
        is_elevated: crate::elevation::is_elevated(),
        drives: crate::diskmap::drives(),
    }
}

fn open_hklm(path: &str) -> Option<RegKey> {
    HKLM.open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY).ok()
}

fn reg_str(key: &RegKey, name: &str) -> String {
    key.get_value::<String, _>(name).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// `ProductName` still says "Windows 10" on Windows 11, so the build number
/// decides which one it really is.
fn os_name(product: &str, build: &str) -> String {
    let build_no: u32 = build.trim().parse().unwrap_or(0);
    if build_no >= 22000 {
        product.replacen("Windows 10", "Windows 11", 1)
    } else {
        product.to_string()
    }
}

fn os_build(build: &str, ubr: Option<u32>) -> String {
    match (build.is_empty(), ubr) {
        (true, _) => String::new(),
        (false, Some(u)) => format!("{build}.{u}"),
        (false, None) => build.to_string(),
    }
}

fn gpus() -> Vec<String> {
    let Some(class) = open_hklm(GPU_CLASS) else { return Vec::new() };
    let names: Vec<String> = class
        .enum_keys()
        .filter_map(|k| k.ok())
        .filter(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|k| class.open_subkey_with_flags(&k, KEY_READ).ok())
        .map(|k| reg_str(&k, "DriverDesc"))
        .collect();
    tidy_gpus(names)
}

/// Drop blanks and repeats, and hide the basic display driver when a real
/// adapter is present (it is only the Windows fallback).
fn tidy_gpus(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in names {
        if !n.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&n)) {
            out.push(n);
        }
    }
    if out.iter().any(|n| n != BASIC_DISPLAY) {
        out.retain(|n| n != BASIC_DISPLAY);
    }
    out
}

/// (total, used) physical memory in bytes. Zeros if Windows will not say.
fn memory() -> (u64, u64) {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: dwLength is set to the struct size as the API requires, and the
    // pointer is to a live local.
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_err() {
        return (0, 0);
    }
    (status.ullTotalPhys, status.ullTotalPhys.saturating_sub(status.ullAvailPhys))
}

fn uptime_secs() -> u64 {
    // SAFETY: takes no arguments and cannot fail.
    let ms = unsafe { GetTickCount64() };
    ms / 1000
}

fn filetime_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

/// (idle, kernel + user) in 100 ns units. Kernel time already includes idle.
fn system_times() -> Option<(u64, u64)> {
    let (mut idle, mut kernel, mut user) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: three pointers to live locals of the right type.
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    Some((filetime_u64(idle), filetime_u64(kernel) + filetime_u64(user)))
}

/// CPU busy share between two readings of `system_times`, 0 to 100.
fn cpu_percent(prev: (u64, u64), now: (u64, u64)) -> f32 {
    let idle = now.0.saturating_sub(prev.0);
    let total = now.1.saturating_sub(prev.1);
    if total == 0 {
        return 0.0;
    }
    let busy = total.saturating_sub(idle);
    ((busy as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32
}

/// Keeps the state needed to measure CPU use between two samples. The app
/// holds one of these and calls `sample` every couple of seconds.
pub struct LiveSampler {
    last: Option<(u64, u64)>,
}

impl LiveSampler {
    pub fn new() -> Self {
        // Prime with a first reading so the first sample measures something.
        Self { last: system_times() }
    }

    /// CPU use since the previous call, and memory right now.
    pub fn sample(&mut self) -> LiveStats {
        let now = system_times();
        let cpu = match (self.last, now) {
            (Some(prev), Some(cur)) => cpu_percent(prev, cur),
            _ => 0.0,
        };
        if now.is_some() {
            self.last = now;
        }
        let (total, used) = memory();
        LiveStats { cpu_percent: cpu, ram_used_bytes: used, ram_total_bytes: total }
    }
}

impl Default for LiveSampler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_windows_11_by_build() {
        assert_eq!(os_name("Windows 10 Pro", "26100"), "Windows 11 Pro");
        assert_eq!(os_name("Windows 10 Pro", "19045"), "Windows 10 Pro");
        assert_eq!(os_name("", ""), "");
        assert_eq!(os_build("26100", Some(2314)), "26100.2314");
        assert_eq!(os_build("26100", None), "26100");
        assert_eq!(os_build("", Some(1)), "");
    }

    #[test]
    fn tidies_gpu_list() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            tidy_gpus(v(&["NVIDIA RTX", BASIC_DISPLAY, "nvidia rtx", ""])),
            v(&["NVIDIA RTX"])
        );
        assert_eq!(tidy_gpus(v(&[BASIC_DISPLAY])), v(&[BASIC_DISPLAY]));
    }

    #[test]
    fn cpu_percent_from_deltas() {
        assert_eq!(cpu_percent((0, 0), (0, 0)), 0.0);
        assert_eq!(cpu_percent((100, 200), (150, 300)), 50.0);
        // A reading that goes backwards must not produce garbage.
        assert_eq!(cpu_percent((500, 900), (100, 100)), 0.0);
    }

    #[test]
    fn live_sampler_is_sane() {
        let mut s = LiveSampler::new();
        let a = s.sample();
        assert!((0.0..=100.0).contains(&a.cpu_percent));
        assert!(a.ram_total_bytes >= a.ram_used_bytes);
    }

    #[test]
    fn overview_reads_this_machine() {
        let o = overview();
        assert!(o.ram_total_bytes > 0);
        assert!(o.cpu_cores_logical > 0);
        assert!(o.uptime_secs > 0);
        eprintln!(
            "OVERVIEW os={:?} ver={:?} build={:?} make={:?} model={:?} cpu={:?} cores={}/{} gpus={:?} ram={}/{} up={} elevated={}",
            o.os_name, o.os_version, o.os_build, o.manufacturer, o.model, o.cpu_name,
            o.cpu_cores_physical, o.cpu_cores_logical, o.gpus, o.ram_used_bytes,
            o.ram_total_bytes, o.uptime_secs, o.is_elevated
        );
    }
}
