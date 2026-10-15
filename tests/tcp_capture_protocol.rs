//! Exercise the actual client against independent version-2 wire fixtures.

#![cfg(feature = "tcp_capture")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use xsar::tcp::{ScreenCaptureClient, TcpCaptureError};


/// Receive one request, with a deadline so a broken client cannot hang the peer.
fn request(listener: &TcpListener) -> TcpStream {
    let (mut socket, _) = listener.accept().unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut bytes = [0u8; 5];
    socket.read_exact(&mut bytes).unwrap();
    assert_eq!(bytes, [0x42, 0x02, 0x01, 0, 0]);
    socket
}


/// Encode a fixture independently of the production header implementation.
fn header(width: u32, height: u32, stride: u16, sequence: u32) -> Vec<u8> {
    [
        width.to_le_bytes().as_slice(),
        height.to_le_bytes().as_slice(),
        stride.to_le_bytes().as_slice(),
        sequence.to_le_bytes().as_slice(),
    ].concat()
}


#[test]
fn full_hd_capture_reads_u16_stride_converts_bgra_and_reconnects() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let peer = thread::spawn(move || {
        for sequence in 1..=2 {
            // The real Windows server handles exactly one frame per connection.
            let mut socket = request(&listener);
            let mut bytes = header(1920, 1080, 7680, sequence);
            assert_eq!(bytes.len(), 14);
            // B, G, R, A on TCP; R and B must exchange places in the result.
            bytes.extend([0x11, 0x22, 0x33, 0xff].repeat(1920 * 1080));
            // Deliberately split inside the u16 stride and frame number fields.
            socket.write_all(&bytes[..9]).unwrap();
            socket.write_all(&bytes[9..12]).unwrap();
            socket.write_all(&bytes[12..]).unwrap();
        }
    });

    let mut client = ScreenCaptureClient::new(Some(address));
    for _ in 0..2 {
        let (frame, _) = client.capture_full().unwrap();
        assert_eq!((frame.width, frame.height, frame.stride), (1920, 1080, 7680));
        assert_eq!(frame.pixels.len(), 8_294_400);
        assert!(frame.pixels.as_chunks::<4>().0.iter()
            .all(|pixel| *pixel == [0x33, 0x22, 0x11, 0xff]));
    }
    peer.join().unwrap();
}


#[test]
fn invalid_wire_geometry_is_rejected_before_reading_pixels() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = ScreenCaptureClient::new(Some(listener.local_addr().unwrap().to_string()));
    let invalid = [
        header(0, 1, 4, 0),
        header(1, 0, 4, 0),
        header(1, 1, 0, 0),
        // Signed -1's byte pattern is u16::MAX and is not a four-byte row.
        header(1920, 1080, u16::from_le_bytes((-1i16).to_le_bytes()), 0),
        // Even aligned signed -4 bytes cannot describe a packed 1920-pixel row.
        header(1920, 1080, u16::from_le_bytes((-4i16).to_le_bytes()), 0),
        // 16384 * 4 = 65536, one byte beyond the u16 range; do not wrap to zero.
        header(16384, 1, 0, 0),
        header(16384, 1, 65532, 0),
        header(1920, 1, 4, 0),
        header(1920, 1093, 7680, 0), // Exceeds the independent 8 MiB frame cap.
    ];
    let count = invalid.len();
    let peer = thread::spawn(move || {
        for bytes in invalid {
            request(&listener).write_all(&bytes).unwrap();
            // No payload: an attempt to read pixels would produce an I/O error.
        }
    });
    for _ in 0..count {
        assert!(matches!(client.capture_full(), Err(TcpCaptureError::ProtocolError(_))));
    }
    peer.join().unwrap();
}


#[test]
fn incomplete_payload_is_not_reused_for_the_next_capture() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let peer = thread::spawn(move || {
        {
            let mut socket = request(&listener);
            socket.write_all(&header(2, 2, 8, 10)).unwrap();
            socket.write_all(&[0x11]).unwrap();
        }
        let mut socket = request(&listener);
        socket.write_all(&header(1, 1, 4, 11)).unwrap();
        socket.write_all(&[0x11, 0x22, 0x33, 0xff]).unwrap();
    });
    let mut client = ScreenCaptureClient::new(Some(address));
    assert!(matches!(client.capture_full(), Err(TcpCaptureError::IoError(_))));
    let (frame, _) = client.capture_full().unwrap();
    assert_eq!(frame.pixels, [0x33, 0x22, 0x11, 0xff]);
    peer.join().unwrap();
}
