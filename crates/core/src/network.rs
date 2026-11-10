//! Open network connections and the programs that own them.
//!
//! Read straight from the IP Helper tables (`GetExtendedTcpTable` and
//! `GetExtendedUdpTable`): no subprocess, no console, no locale issues.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};

use crate::model::Connection;
use crate::{Error, Result};

const AF_INET: u32 = 2;
const AF_INET6: u32 = 23;

/// Friendly names for `MIB_TCP_STATE` values.
pub fn tcp_state_name(state: u32) -> &'static str {
    match state {
        1 => "Closed",
        2 => "Listening",
        3 => "Syn sent",
        4 => "Syn received",
        5 => "Established",
        6 => "Fin wait 1",
        7 => "Fin wait 2",
        8 => "Close wait",
        9 => "Closing",
        10 => "Last ack",
        11 => "Time wait",
        12 => "Delete TCB",
        _ => "Unknown",
    }
}

/// Ports sit in the low 16 bits of a DWORD, in network byte order.
pub fn decode_port(raw: u32) -> u16 {
    u16::from_be((raw & 0xFFFF) as u16)
}

/// An IPv4 address DWORD is stored in network byte order in memory.
pub fn decode_ipv4(raw: u32) -> Ipv4Addr {
    Ipv4Addr::from(raw.to_ne_bytes())
}

/// True when the address is this machine: 127.0.0.0/8 or ::1 (also
/// IPv4-mapped loopback such as ::ffff:127.0.0.1).
pub fn is_loopback(addr: &str) -> bool {
    if let Ok(v4) = addr.parse::<Ipv4Addr>() {
        return v4.is_loopback();
    }
    if let Ok(v6) = addr.parse::<Ipv6Addr>() {
        return v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback());
    }
    false
}

/// Copy the rows out of a raw IP Helper table: a u32 count followed by that
/// many rows. Never reads past the end of `buf`, whatever the count says.
pub fn rows_from_buffer<T: Copy>(buf: &[u8]) -> Vec<T> {
    let row_size = std::mem::size_of::<T>();
    // Every table here starts with a u32 count and rows that are 4-aligned.
    let header = 4usize;
    if buf.len() < header || row_size == 0 {
        return Vec::new();
    }
    let count = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    let fits = (buf.len() - header) / row_size;
    (0..count.min(fits))
        .map(|i| {
            let offset = header + i * row_size;
            // SAFETY: offset + row_size <= buf.len() by the `fits` bound, T is
            // a plain Copy struct, and read_unaligned has no alignment needs.
            unsafe { std::ptr::read_unaligned(buf.as_ptr().add(offset) as *const T) }
        })
        .collect()
}

/// Call a GetExtended*Table function until the buffer is big enough.
fn fetch_table(mut call: impl FnMut(*mut core::ffi::c_void, &mut u32) -> u32) -> Option<Vec<u8>> {
    let mut size: u32 = 16 * 1024;
    for _ in 0..5 {
        let mut buf = vec![0u8; size as usize];
        let rc = call(buf.as_mut_ptr() as *mut _, &mut size);
        if rc == NO_ERROR.0 {
            buf.truncate(size as usize);
            return Some(buf);
        }
        if rc != ERROR_INSUFFICIENT_BUFFER.0 {
            return None;
        }
        // The table can grow between calls, so leave some headroom.
        size = size.saturating_add(4096);
    }
    None
}

fn tcp_table(family: u32) -> Option<Vec<u8>> {
    fetch_table(|ptr, size| {
        // SAFETY: `ptr` points at a buffer of `*size` bytes owned by
        // fetch_table, and Windows writes no more than `*size`.
        unsafe { GetExtendedTcpTable(Some(ptr), size, false, family, TCP_TABLE_OWNER_PID_ALL, 0) }
    })
}

fn udp_table(family: u32) -> Option<Vec<u8>> {
    fetch_table(|ptr, size| {
        // SAFETY: as for tcp_table.
        unsafe { GetExtendedUdpTable(Some(ptr), size, false, family, UDP_TABLE_OWNER_PID, 0) }
    })
}

/// Build one row. Remote fields are blanked for listeners and UDP.
fn make(
    protocol: &str,
    local: String,
    local_port: u16,
    remote: Option<(String, u16)>,
    state: &str,
    pid: u32,
    names: &HashMap<u32, String>,
) -> Connection {
    let (remote_address, remote_port) = remote.unwrap_or_default();
    Connection {
        protocol: protocol.to_string(),
        local_address: local,
        local_port,
        loopback: !remote_address.is_empty() && is_loopback(&remote_address),
        remote_address,
        remote_port,
        state: state.to_string(),
        pid,
        process: process_name(pid, names),
    }
}

fn process_name(pid: u32, names: &HashMap<u32, String>) -> String {
    match pid {
        0 => "System Idle Process".to_string(),
        4 => "System".to_string(),
        _ => names.get(&pid).cloned().unwrap_or_default(),
    }
}

fn tcp_remote(state: u32, addr: String, port: u16) -> Option<(String, u16)> {
    // A listener has no remote end; Windows reports 0.0.0.0:0 there.
    if state == 2 {
        None
    } else {
        Some((addr, port))
    }
}

/// Established first, then listening, then the rest; each by process name
/// (case-insensitive) and then local port.
pub fn sort_connections(list: &mut [Connection]) {
    fn rank(c: &Connection) -> u8 {
        match c.state.as_str() {
            "Established" => 0,
            "Listening" => 1,
            _ => 2,
        }
    }
    list.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.process.to_lowercase().cmp(&b.process.to_lowercase()))
            .then(a.local_port.cmp(&b.local_port))
            .then(a.protocol.cmp(&b.protocol))
    });
}

fn process_names() -> HashMap<u32, String> {
    let Ok(mut sys) = sysinfo::System::new() else { return HashMap::new() };
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::nothing(),
    );
    sys.processes()
        .iter()
        .map(|(pid, p)| (pid.as_u32(), p.name().to_string_lossy().into_owned()))
        .collect()
}

/// Every TCP connection and listener, and every bound UDP port, IPv4 and
/// IPv6, with the owning process name where Windows will tell us.
/// Established connections first, then listeners, then the rest.
pub fn connections() -> Result<Vec<Connection>> {
    let names = process_names();
    let mut out = Vec::new();
    let mut any = false;

    if let Some(buf) = tcp_table(AF_INET) {
        any = true;
        for r in rows_from_buffer::<MIB_TCPROW_OWNER_PID>(&buf) {
            let remote = tcp_remote(
                r.dwState,
                decode_ipv4(r.dwRemoteAddr).to_string(),
                decode_port(r.dwRemotePort),
            );
            out.push(make(
                "TCP",
                decode_ipv4(r.dwLocalAddr).to_string(),
                decode_port(r.dwLocalPort),
                remote,
                tcp_state_name(r.dwState),
                r.dwOwningPid,
                &names,
            ));
        }
    }
    if let Some(buf) = tcp_table(AF_INET6) {
        any = true;
        for r in rows_from_buffer::<MIB_TCP6ROW_OWNER_PID>(&buf) {
            let remote = tcp_remote(
                r.dwState,
                Ipv6Addr::from(r.ucRemoteAddr).to_string(),
                decode_port(r.dwRemotePort),
            );
            out.push(make(
                "TCP",
                Ipv6Addr::from(r.ucLocalAddr).to_string(),
                decode_port(r.dwLocalPort),
                remote,
                tcp_state_name(r.dwState),
                r.dwOwningPid,
                &names,
            ));
        }
    }
    if let Some(buf) = udp_table(AF_INET) {
        any = true;
        for r in rows_from_buffer::<MIB_UDPROW_OWNER_PID>(&buf) {
            out.push(make(
                "UDP",
                decode_ipv4(r.dwLocalAddr).to_string(),
                decode_port(r.dwLocalPort),
                None,
                "",
                r.dwOwningPid,
                &names,
            ));
        }
    }
    if let Some(buf) = udp_table(AF_INET6) {
        any = true;
        for r in rows_from_buffer::<MIB_UDP6ROW_OWNER_PID>(&buf) {
            out.push(make(
                "UDP",
                Ipv6Addr::from(r.ucLocalAddr).to_string(),
                decode_port(r.dwLocalPort),
                None,
                "",
                r.dwOwningPid,
                &names,
            ));
        }
    }
    if !any {
        return Err(Error::msg("Windows would not list the network connections."));
    }
    sort_connections(&mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_ports_and_addresses() {
        // Port 443 in network order sits in memory as bytes 01 BB.
        let raw = u32::from_ne_bytes([0x01, 0xBB, 0, 0]);
        assert_eq!(decode_port(raw), 443);
        assert_eq!(decode_ipv4(u32::from_ne_bytes([127, 0, 0, 1])), Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(decode_ipv4(u32::from_ne_bytes([192, 168, 1, 20])).to_string(), "192.168.1.20");
    }

    #[test]
    fn names_states() {
        assert_eq!(tcp_state_name(5), "Established");
        assert_eq!(tcp_state_name(2), "Listening");
        assert_eq!(tcp_state_name(11), "Time wait");
        assert_eq!(tcp_state_name(8), "Close wait");
        assert_eq!(tcp_state_name(3), "Syn sent");
        assert_eq!(tcp_state_name(99), "Unknown");
    }

    #[test]
    fn spots_loopback() {
        assert!(is_loopback("127.0.0.1"));
        assert!(is_loopback("127.5.6.7"));
        assert!(is_loopback("::1"));
        assert!(is_loopback("::ffff:127.0.0.1"));
        assert!(!is_loopback("192.168.0.1"));
        assert!(!is_loopback("::"));
        assert!(!is_loopback(""));
    }

    #[test]
    fn reads_rows_safely() {
        let row = MIB_TCPROW_OWNER_PID {
            dwState: 5,
            dwLocalAddr: u32::from_ne_bytes([10, 0, 0, 2]),
            dwLocalPort: u32::from_ne_bytes([0x1F, 0x90, 0, 0]),
            dwRemoteAddr: u32::from_ne_bytes([127, 0, 0, 1]),
            dwRemotePort: u32::from_ne_bytes([0x01, 0xBB, 0, 0]),
            dwOwningPid: 1234,
        };
        let mut buf = 2u32.to_ne_bytes().to_vec();
        for _ in 0..2 {
            // SAFETY (test): viewing a plain repr(C) struct of u32s as bytes.
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    &row as *const _ as *const u8,
                    std::mem::size_of::<MIB_TCPROW_OWNER_PID>(),
                )
            };
            buf.extend_from_slice(bytes);
        }
        let rows = rows_from_buffer::<MIB_TCPROW_OWNER_PID>(&buf);
        assert_eq!(rows.len(), 2);
        assert_eq!(decode_port(rows[0].dwLocalPort), 8080);
        assert_eq!(rows[1].dwOwningPid, 1234);

        // A count that claims more rows than the buffer holds is clamped.
        let mut lying = buf.clone();
        lying[..4].copy_from_slice(&1000u32.to_ne_bytes());
        assert_eq!(rows_from_buffer::<MIB_TCPROW_OWNER_PID>(&lying).len(), 2);
        assert!(rows_from_buffer::<MIB_TCPROW_OWNER_PID>(&[1, 0]).is_empty());
    }

    #[test]
    fn listeners_and_udp_have_no_remote() {
        let names = HashMap::from([(77u32, "app.exe".to_string())]);
        let l = make("TCP", "0.0.0.0".into(), 80, tcp_remote(2, "0.0.0.0".into(), 0), "Listening", 77, &names);
        assert_eq!(l.remote_address, "");
        assert!(!l.loopback);
        assert_eq!(l.process, "app.exe");
        let e = make("TCP", "127.0.0.1".into(), 5000, tcp_remote(5, "127.0.0.1".into(), 6000), "Established", 4, &names);
        assert!(e.loopback);
        assert_eq!(e.process, "System");
    }

    #[test]
    fn sorts_established_first() {
        let names = HashMap::new();
        let mut v = vec![
            make("UDP", "0.0.0.0".into(), 53, None, "", 9, &names),
            make("TCP", "0.0.0.0".into(), 80, None, "Listening", 9, &names),
            make("TCP", "10.0.0.1".into(), 50000, Some(("1.1.1.1".into(), 443)), "Established", 9, &names),
            make("TCP", "10.0.0.1".into(), 40000, Some(("1.1.1.1".into(), 443)), "Time wait", 9, &names),
        ];
        sort_connections(&mut v);
        assert_eq!(v[0].state, "Established");
        assert_eq!(v[1].state, "Listening");
        assert_eq!(v[2].local_port, 53);
        assert_eq!(v[3].local_port, 40000);
    }

    #[test]
    fn real_connections_are_quick() {
        let start = std::time::Instant::now();
        let list = connections().unwrap();
        let ms = start.elapsed().as_millis();
        let named = list.iter().filter(|c| !c.process.is_empty()).count();
        eprintln!("NETWORK count={} named={} ms={}", list.len(), named, ms);
        assert!(!list.is_empty());
    }
}
