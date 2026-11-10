//! Read-only security posture checks. Nothing here changes a setting.
//!
//! Most checks are plain registry reads. Antivirus, third-party firewall and
//! drive encryption need WMI or the shell, so they come from one hidden
//! PowerShell call with a fixed script and a hard time limit.

use std::io::{self, Read};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::Value;
use winreg::enums::{KEY_READ, KEY_WOW64_64KEY};
use winreg::HKLM;

use crate::model::{SecurityCheck, Status};
use crate::util::hidden_command;

/// How long the PowerShell probe may run before we give up on it.
const PS_TIMEOUT: Duration = Duration::from_secs(15);

/// Fixed script: no input from anywhere is ever spliced into it. Prints one
/// JSON object. `avOk`/`fwOk` say whether SecurityCenter2 could be read at
/// all, so "no products" and "could not ask" are not confused.
const PS_SCRIPT: &str = r#"$ErrorActionPreference='Stop'
$r=@{}
try { $r.av=@(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct | ForEach-Object { @{name=[string]$_.displayName; state=[int64]$_.productState} }); $r.avOk=$true } catch { $r.avOk=$false }
try { $r.fw=@(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName FirewallProduct | ForEach-Object { @{name=[string]$_.displayName; state=[int64]$_.productState} }); $r.fwOk=$true } catch { $r.fwOk=$false }
try { $v=(New-Object -ComObject Shell.Application).NameSpace(17).ParseName($env:SystemDrive).ExtendedProperty('System.Volume.BitLockerProtection'); if ($null -ne $v) { $r.bitlocker=[int64]$v } } catch {}
$r | ConvertTo-Json -Compress -Depth 4"#;

const COULD_NOT_CHECK: &str = "Could not be checked";

/// A product registered in SecurityCenter2.
#[derive(Debug, Clone, PartialEq)]
pub struct Product {
    pub name: String,
    pub state: u32,
}

impl Product {
    /// Bit 0x1000 of productState: real-time protection or the firewall is on.
    pub fn is_on(&self) -> bool {
        self.state & 0x1000 != 0
    }
    /// Bit 0x10 of productState: definitions are out of date.
    pub fn is_out_of_date(&self) -> bool {
        self.state & 0x10 != 0
    }
}

/// What the PowerShell probe found. None means it could not be read.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Probe {
    pub antivirus: Option<Vec<Product>>,
    pub firewalls: Option<Vec<Product>>,
    pub bitlocker: Option<i64>,
}

fn products(v: &Value, list_key: &str, ok_key: &str) -> Option<Vec<Product>> {
    if v.get(ok_key).and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let one = |p: &Value| -> Option<Product> {
        Some(Product {
            name: p.get("name")?.as_str()?.trim().to_string(),
            state: u32::try_from(p.get("state")?.as_i64()?).ok()?,
        })
    };
    // PowerShell may print one product as an object instead of an array.
    Some(match v.get(list_key) {
        Some(Value::Array(items)) => items.iter().filter_map(one).collect(),
        Some(obj @ Value::Object(_)) => one(obj).into_iter().collect(),
        _ => Vec::new(),
    })
}

/// Decode the probe's JSON. Anything malformed comes back as "not read".
pub fn parse_probe(text: &str) -> Probe {
    let Ok(v) = serde_json::from_str::<Value>(text.trim()) else { return Probe::default() };
    Probe {
        antivirus: products(&v, "av", "avOk"),
        firewalls: products(&v, "fw", "fwOk"),
        bitlocker: v.get("bitlocker").and_then(Value::as_i64),
    }
}

/// Run the probe, killing it if it overruns.
fn run_probe() -> Probe {
    let child = hidden_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", PS_SCRIPT])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return Probe::default() };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        return Probe::default();
    };
    // Read on a thread so a stuck script cannot block us past the deadline.
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + PS_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Probe::default();
            }
        }
    }
    let out = reader.join().unwrap_or_default();
    parse_probe(&String::from_utf8_lossy(&out))
}

// ── Registry helpers ─────────────────────────────────────────────────────────

/// A DWORD, or None when the key or value is missing or unreadable.
fn dword(path: &str, name: &str) -> Option<u32> {
    HKLM.open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY)
        .ok()?
        .get_value::<u32, _>(name)
        .ok()
}

/// Some(true) if the key exists, Some(false) if it does not, None if Windows
/// would not say.
fn key_exists(path: &str) -> Option<bool> {
    match HKLM.open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY) {
        Ok(_) => Some(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

// ── Decoding into results ────────────────────────────────────────────────────

/// (status, summary) for each check. Pure, so every path can be tested.
pub fn antivirus_status(products: Option<&[Product]>) -> (Status, String) {
    let Some(products) = products else { return (Status::Unknown, COULD_NOT_CHECK.into()) };
    let on: Vec<&Product> = products.iter().filter(|p| p.is_on()).collect();
    if let Some(p) = on.iter().find(|p| !p.is_out_of_date()) {
        return (Status::Good, format!("{} is on and up to date", p.name));
    }
    if let Some(p) = on.first() {
        return (Status::Attention, format!("{} is on but its definitions are out of date", p.name));
    }
    if products.is_empty() {
        (Status::Problem, "No antivirus is registered with Windows".into())
    } else {
        (Status::Problem, "No antivirus has real-time protection switched on".into())
    }
}

pub fn firewall_status(
    domain: Option<u32>,
    private: Option<u32>,
    public: Option<u32>,
    third_party: Option<&[Product]>,
) -> (Status, String) {
    let managed_by = third_party.and_then(|ps| ps.iter().find(|p| p.is_on()));
    if domain.is_none() && private.is_none() && public.is_none() {
        return match managed_by {
            Some(p) => (Status::Good, format!("Managed by {}", p.name)),
            None => (Status::Unknown, COULD_NOT_CHECK.into()),
        };
    }
    let on = |v: Option<u32>| v == Some(1);
    if on(domain) && on(private) && on(public) {
        return (Status::Good, "On for all networks".into());
    }
    if let Some(p) = managed_by {
        return (Status::Good, format!("Managed by {}", p.name));
    }
    let mut off = Vec::new();
    if !on(private) {
        off.push("private");
    }
    if !on(public) {
        off.push("public");
    }
    if off.is_empty() {
        return (Status::Attention, "Off for work (domain) networks".into());
    }
    (Status::Problem, format!("Off for {} networks", off.join(" and ")))
}

pub fn uac_status(enable_lua: Option<u32>, consent_admin: Option<u32>) -> (Status, String) {
    match (enable_lua, consent_admin) {
        (None, _) => (Status::Unknown, COULD_NOT_CHECK.into()),
        (Some(0), _) => (Status::Problem, "Switched off".into()),
        (Some(_), Some(0)) => (Status::Attention, "Never asks before making changes".into()),
        (Some(_), _) => (Status::Good, "On".into()),
    }
}

pub fn secure_boot_status(value: Option<u32>) -> (Status, String) {
    match value {
        Some(1) => (Status::Good, "On".into()),
        Some(0) => (Status::Attention, "Off".into()),
        _ => (Status::Unknown, COULD_NOT_CHECK.into()),
    }
}

/// `System.Volume.BitLockerProtection`: 1, 3, 5 protected; 2 off; other
/// non-zero values suspended or part way through. 0 or missing: unknown.
pub fn encryption_status(code: Option<i64>) -> (Status, String) {
    match code {
        Some(1) | Some(3) | Some(5) => (Status::Good, "System drive is encrypted".into()),
        Some(2) => (Status::Attention, "System drive is not encrypted".into()),
        Some(c) if c > 0 => (Status::Attention, "Encryption is suspended or still in progress".into()),
        _ => (Status::Unknown, COULD_NOT_CHECK.into()),
    }
}

pub fn remote_desktop_status(deny: Option<u32>) -> (Status, String) {
    match deny {
        Some(1) => (Status::Good, "Off".into()),
        Some(0) => (Status::Attention, "On".into()),
        _ => (Status::Unknown, COULD_NOT_CHECK.into()),
    }
}

pub fn smb1_status(value: Option<u32>) -> (Status, String) {
    match value {
        Some(1) => (Status::Problem, "The old SMB1 file sharing protocol is on".into()),
        _ => (Status::Good, "SMB1 is off".into()),
    }
}

pub fn restart_status(update: Option<bool>, servicing: Option<bool>) -> (Status, String) {
    match (update, servicing) {
        (Some(true), _) | (_, Some(true)) => {
            (Status::Attention, "A restart is needed to finish updates".into())
        }
        (Some(false), Some(false)) => (Status::Good, "No restart waiting".into()),
        _ => (Status::Unknown, COULD_NOT_CHECK.into()),
    }
}

fn check(
    id: &str,
    label: &str,
    (status, summary): (Status, String),
    detail: &str,
    page: Option<&str>,
) -> SecurityCheck {
    // A fix page is offered only when there is something to look at.
    let fix_page = if status == Status::Good { None } else { page.map(str::to_string) };
    SecurityCheck {
        id: id.into(),
        label: label.into(),
        status,
        summary,
        detail: detail.into(),
        fix_page,
    }
}

/// Run every check. Never fails: a check that cannot be read comes back with
/// `Status::Unknown` and a summary that says so.
///
/// Checks, in this order: antivirus, firewall, uac, secure-boot,
/// drive-encryption, remote-desktop, smb1, restart-pending.
pub fn checks() -> Vec<SecurityCheck> {
    let probe = run_probe();
    let fw = r"SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy";
    let sys_policy = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System";
    let rdp_detail = "Remote Desktop lets other computers sign in to this one. That is fine if you use it, otherwise it is safer off.";

    vec![
        check(
            "antivirus",
            "Antivirus",
            antivirus_status(probe.antivirus.as_deref()),
            "Real-time protection with current definitions stops most malware before it runs.",
            Some("windows-security"),
        ),
        check(
            "firewall",
            "Firewall",
            firewall_status(
                dword(&format!(r"{fw}\DomainProfile"), "EnableFirewall"),
                dword(&format!(r"{fw}\StandardProfile"), "EnableFirewall"),
                dword(&format!(r"{fw}\PublicProfile"), "EnableFirewall"),
                probe.firewalls.as_deref(),
            ),
            "A firewall blocks unwanted connections from other devices, especially on public Wi-Fi.",
            Some("firewall"),
        ),
        check(
            "uac",
            "User Account Control",
            uac_status(dword(sys_policy, "EnableLUA"), dword(sys_policy, "ConsentPromptBehaviorAdmin")),
            "User Account Control asks before programs make changes that need administrator rights.",
            Some("uac"),
        ),
        check(
            "secure-boot",
            "Secure Boot",
            secure_boot_status(dword(
                r"SYSTEM\CurrentControlSet\Control\SecureBoot\State",
                "UEFISecureBootEnabled",
            )),
            "Secure Boot stops tampered start-up code loading before Windows. It is a firmware setting, changed in the PC's UEFI setup rather than in Windows.",
            None,
        ),
        check(
            "drive-encryption",
            "Drive encryption",
            encryption_status(probe.bitlocker),
            "Encryption keeps your files unreadable if the PC or its drive is lost or stolen.",
            Some("device-encryption"),
        ),
        check(
            "remote-desktop",
            "Remote Desktop",
            remote_desktop_status(dword(
                r"SYSTEM\CurrentControlSet\Control\Terminal Server",
                "fDenyTSConnections",
            )),
            rdp_detail,
            Some("remote-desktop"),
        ),
        check(
            "smb1",
            "SMB1 file sharing",
            smb1_status(dword(r"SYSTEM\CurrentControlSet\Services\LanmanServer\Parameters", "SMB1")),
            "SMB1 is an old file sharing protocol that worms such as WannaCry used to spread.",
            None,
        ),
        check(
            "restart-pending",
            "Restart for updates",
            restart_status(
                key_exists(r"SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired"),
                key_exists(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending"),
            ),
            "Some security updates only take effect after a restart.",
            Some("windows-update"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, state: u32) -> Product {
        Product { name: name.into(), state }
    }

    #[test]
    fn decodes_product_state_bits() {
        // Defender on and current, as seen on a real machine.
        let d = p("Windows Defender", 397568);
        assert!(d.is_on());
        assert!(!d.is_out_of_date());
        let stale = p("X", 0x1010);
        assert!(stale.is_on() && stale.is_out_of_date());
        assert!(!p("Off", 0x0000_0100).is_on());
    }

    #[test]
    fn antivirus_paths() {
        assert_eq!(antivirus_status(None).0, Status::Unknown);
        assert_eq!(antivirus_status(Some(&[])).0, Status::Problem);
        let (s, m) = antivirus_status(Some(&[p("Off AV", 0x100), p("Defender", 0x1000)]));
        assert_eq!(s, Status::Good);
        assert!(m.contains("Defender"));
        assert_eq!(antivirus_status(Some(&[p("Old", 0x1010)])).0, Status::Attention);
        assert_eq!(antivirus_status(Some(&[p("Off", 0x100)])).0, Status::Problem);
    }

    #[test]
    fn firewall_paths() {
        assert_eq!(firewall_status(Some(1), Some(1), Some(1), None).0, Status::Good);
        assert_eq!(firewall_status(Some(1), Some(0), Some(1), Some(&[])).0, Status::Problem);
        assert_eq!(firewall_status(Some(1), Some(1), Some(0), None).0, Status::Problem);
        assert_eq!(firewall_status(Some(0), Some(1), Some(1), None).0, Status::Attention);
        let (s, m) = firewall_status(Some(0), Some(0), Some(0), Some(&[p("Comodo", 0x1000)]));
        assert_eq!(s, Status::Good);
        assert!(m.contains("Comodo"));
        // A third-party firewall that is off does not count.
        assert_eq!(firewall_status(Some(1), Some(0), Some(1), Some(&[p("Off", 0x100)])).0, Status::Problem);
        assert_eq!(firewall_status(None, None, None, None).0, Status::Unknown);
        assert_eq!(firewall_status(None, None, None, Some(&[p("FW", 0x1000)])).0, Status::Good);
    }

    #[test]
    fn simple_registry_checks() {
        assert_eq!(uac_status(Some(0), Some(5)).0, Status::Problem);
        assert_eq!(uac_status(Some(1), Some(0)).0, Status::Attention);
        assert_eq!(uac_status(Some(1), Some(5)).0, Status::Good);
        assert_eq!(uac_status(Some(1), None).0, Status::Good);
        assert_eq!(uac_status(None, Some(5)).0, Status::Unknown);

        assert_eq!(secure_boot_status(Some(1)).0, Status::Good);
        assert_eq!(secure_boot_status(Some(0)).0, Status::Attention);
        assert_eq!(secure_boot_status(None).0, Status::Unknown);

        assert_eq!(remote_desktop_status(Some(1)).0, Status::Good);
        assert_eq!(remote_desktop_status(Some(0)).0, Status::Attention);
        assert_eq!(remote_desktop_status(None).0, Status::Unknown);

        assert_eq!(smb1_status(Some(1)).0, Status::Problem);
        assert_eq!(smb1_status(Some(0)).0, Status::Good);
        assert_eq!(smb1_status(None).0, Status::Good);

        assert_eq!(restart_status(Some(true), Some(false)).0, Status::Attention);
        assert_eq!(restart_status(Some(false), Some(true)).0, Status::Attention);
        assert_eq!(restart_status(Some(false), Some(false)).0, Status::Good);
        assert_eq!(restart_status(None, Some(false)).0, Status::Unknown);
    }

    #[test]
    fn encryption_codes() {
        for c in [1, 3, 5] {
            assert_eq!(encryption_status(Some(c)).0, Status::Good);
        }
        assert_eq!(encryption_status(Some(2)).0, Status::Attention);
        assert_eq!(encryption_status(Some(4)).0, Status::Attention);
        assert_eq!(encryption_status(Some(0)).0, Status::Unknown);
        assert_eq!(encryption_status(None).0, Status::Unknown);
    }

    #[test]
    fn parses_probe_json() {
        let full = r#"{"av":[{"state":397568,"name":"Windows Defender"}],"fwOk":true,"fw":[],"bitlocker":2,"avOk":true}"#;
        let probe = parse_probe(full);
        assert_eq!(probe.antivirus, Some(vec![p("Windows Defender", 397568)]));
        assert_eq!(probe.firewalls, Some(vec![]));
        assert_eq!(probe.bitlocker, Some(2));

        // One product printed as an object rather than an array.
        let single = r#"{"avOk":true,"av":{"name":"A","state":4096},"fwOk":false}"#;
        let probe = parse_probe(single);
        assert_eq!(probe.antivirus, Some(vec![p("A", 4096)]));
        assert_eq!(probe.firewalls, None);
        assert_eq!(probe.bitlocker, None);

        assert_eq!(parse_probe(""), Probe::default());
        assert_eq!(parse_probe("not json"), Probe::default());
    }

    #[test]
    fn fix_page_only_when_needed() {
        let good = check("x", "X", (Status::Good, "ok".into()), "d", Some("uac"));
        assert_eq!(good.fix_page, None);
        let bad = check("x", "X", (Status::Problem, "no".into()), "d", Some("uac"));
        assert_eq!(bad.fix_page.as_deref(), Some("uac"));
        for page in ["windows-security", "firewall", "uac", "device-encryption", "remote-desktop", "windows-update"] {
            assert!(crate::shell::KNOWN_PAGES.contains(&page));
        }
    }

    #[test]
    fn real_checks_in_order() {
        let start = Instant::now();
        let all = checks();
        let ids: Vec<&str> = all.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            ["antivirus", "firewall", "uac", "secure-boot", "drive-encryption", "remote-desktop", "smb1", "restart-pending"]
        );
        eprintln!("SECURITY ms={}", start.elapsed().as_millis());
        for c in &all {
            eprintln!("SECURITY {} {:?} {}", c.id, c.status, c.summary);
        }
    }
}
