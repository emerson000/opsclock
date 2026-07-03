//! Real SNTP client. A single blocking UDP exchange (RFC 4330 / NTPv4) on a
//! background thread; the result is delivered to the UI over an mpsc channel.

use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Seconds between the NTP epoch (1900-01-01) and the Unix epoch (1970-01-01).
const NTP_UNIX_DELTA: u64 = 2_208_988_800;

/// A 48-byte NTPv4 client request: LI=0, VN=4, Mode=3 (client).
pub fn build_client_packet() -> [u8; 48] {
    let mut pkt = [0u8; 48];
    // 0b00_100_011 = LI 0, VN 4, Mode 3.
    pkt[0] = 0x23;
    pkt
}

/// Live state of the NTP panel's sync operation.
pub enum SyncState {
    Idle,
    Syncing(Receiver<Result<i64, String>>),
    Failed(String),
}

/// Kick off a sync on a background thread; returns the channel to poll.
pub fn spawn_sync(server: String) -> Receiver<Result<i64, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(query_offset_ms(&server, Duration::from_secs(3)));
    });
    rx
}

/// Blocking query: returns the clock offset in milliseconds (server − local),
/// or an error string on any failure.
pub fn query_offset_ms(server: &str, timeout: Duration) -> Result<i64, String> {
    let addr = (server, 123u16)
        .to_socket_addrs()
        .map_err(|e| format!("DNS FAIL: {e}"))?
        .next()
        .ok_or_else(|| "NO ADDRESS".to_string())?;

    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("BIND FAIL: {e}"))?;
    socket
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    socket
        .connect(addr)
        .map_err(|e| format!("CONNECT FAIL: {e}"))?;

    let t1 = SystemTime::now();
    socket
        .send(&build_client_packet())
        .map_err(|e| format!("SEND FAIL: {e}"))?;

    let mut buf = [0u8; 48];
    let n = socket
        .recv(&mut buf)
        .map_err(|e| format!("NO REPLY: {e}"))?;
    let t4 = SystemTime::now();
    if n < 48 {
        return Err("SHORT REPLY".to_string());
    }

    // Server transmit timestamp (T3) lives in bytes 40..48.
    let secs = u32::from_be_bytes([buf[40], buf[41], buf[42], buf[43]]) as u64;
    let frac = u32::from_be_bytes([buf[44], buf[45], buf[46], buf[47]]) as u64;
    if secs == 0 {
        return Err("BAD TIMESTAMP".to_string());
    }
    let server_ms =
        (secs - NTP_UNIX_DELTA) as i64 * 1000 + ((frac * 1000) >> 32) as i64;

    // Midpoint of the local send/recv window approximates the local instant at T3.
    let t1_ms = unix_ms(t1);
    let t4_ms = unix_ms(t4);
    let local_ms = (t1_ms + t4_ms) / 2;

    Ok(server_ms - local_ms)
}

fn unix_ms(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_shape() {
        let pkt = build_client_packet();
        assert_eq!(pkt.len(), 48);
        assert_eq!(pkt[0], 0x23); // LI=0 VN=4 Mode=3
        assert!(pkt[1..].iter().all(|&b| b == 0));
    }
}
