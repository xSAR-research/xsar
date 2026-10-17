//! Independent byte fixtures exercise the production version-3 parser and TCP client.
#![cfg(feature = "tcp_capture")]
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use xsar::tcp::protocol::v3::{Request, ResponseHeader, Status};
use xsar::tcp::{CaptureMode, ScreenCaptureClient, TcpCaptureError};


/// Encode the frozen wire layout independently of the implementation under test.
fn header(
    session: u64,
    request: u64,
    frame: u64,
    generation: u64,
    width: u32,
    height: u32,
) -> [u8; 80] {
    let mut bytes = [0; 80];
    bytes[0..4].copy_from_slice(b"XSCP");
    bytes[4] = 3;
    bytes[6] = 1;
    bytes[7] = 1;
    bytes[8..16].copy_from_slice(&session.to_le_bytes());
    bytes[16..24].copy_from_slice(&request.to_le_bytes());
    bytes[24..32].copy_from_slice(&frame.to_le_bytes());
    bytes[32..40].copy_from_slice(&generation.to_le_bytes());
    bytes[40..44].copy_from_slice(&width.to_le_bytes());
    bytes[44..48].copy_from_slice(&height.to_le_bytes());
    bytes[56..60].copy_from_slice(&width.to_le_bytes());
    bytes[60..64].copy_from_slice(&height.to_le_bytes());
    bytes[64..68].copy_from_slice(&(width * 3).to_le_bytes());
    bytes[68..72].copy_from_slice(&(width * height * 3).to_le_bytes());
    bytes[72..76].copy_from_slice(&(width * height * 3).to_le_bytes());
    bytes[77] = 1;
    bytes
}


/// Read one complete request from an existing connection with bounded peer I/O.
fn read_request(socket: &mut TcpStream) -> [u8; 32] {
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = [0; 32];
    socket.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes[0..8], b"XSCP\x03\x01\0\0");
    bytes
}


/// Accept and read the first request without sharing production wire encoding.
fn accept_request(listener: &TcpListener) -> (TcpStream, [u8; 32]) {
    let (mut socket, _) = listener.accept().unwrap();
    let bytes = read_request(&mut socket);
    (socket, bytes)
}


/// Decode one fixture correlation field without production accessors.
fn integer(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}


#[test]
fn fixed_wire_contract_roundtrips_and_odd_widths_have_no_gdi_row_padding() {
    let mut request = [0; 32];
    request[0..8].copy_from_slice(b"XSCP\x03\x01\0\0");
    request[16..24].copy_from_slice(&7u64.to_le_bytes());
    assert_eq!(Request::new(0, 7, 0).unwrap().to_bytes(), request);
    assert_eq!(Request::from_bytes(request).unwrap().request_id(), 7);
    assert!(Request::new(0, 0, 0).is_err());
    assert!(Request::new(1, 1, 0).is_err());
    assert!(Request::new(0, 1, 1).is_err());
    for offset in [0, 4, 5, 6, 7] {
        let mut invalid = request;
        invalid[offset] = 255;
        assert!(
            Request::from_bytes(invalid).is_err(),
            "accepted request offset {offset}"
        );
    }
    let independent = header(11, 7, 43, 1, 3, 2);
    let parsed = ResponseHeader::from_bytes(independent).unwrap();
    assert_eq!(parsed, ResponseHeader::success(11, 7, 43, 1, 3, 2).unwrap());
    assert_eq!(parsed.to_bytes(), independent);
    assert_eq!(
        (
            parsed.stride(),
            parsed.pixel_bytes(),
            parsed.rgba_stride(),
            parsed.rgba_bytes()
        ),
        (9, 18, 12, 24)
    );
    assert_eq!(
        ResponseHeader::success(1, 1, 1, 1, 1920, 1080)
            .unwrap()
            .pixel_bytes(),
        6_220_800
    );
    assert_eq!(
        ResponseHeader::success(1, 1, 1, 1, 1024, 2048)
            .unwrap()
            .rgba_bytes(),
        8_388_608
    );
    assert!(ResponseHeader::success(1, 1, 1, 1, 1024, 2049).is_err());
    assert!(ResponseHeader::success(1, 1, 1, 1, u32::MAX, u32::MAX).is_err());
    assert!(ResponseHeader::success(1, 1, 1, 1, 0, 1).is_err());
    assert!(ResponseHeader::success(1, 1, 1, 1, 1, 0).is_err());
    for identities in [(0, 1, 1, 1), (1, 0, 1, 1), (1, 1, 0, 1), (1, 1, 1, 0)] {
        assert!(
            ResponseHeader::success(identities.0, identities.1, identities.2, identities.3, 1, 1)
                .is_err()
        );
    }
}


#[test]
fn independent_headers_reject_format_coverage_stride_length_and_reserved_mismatches() {
    let good = header(1, 1, 1, 1, 3, 2);
    for offset in [0, 4, 5, 6, 7, 48, 52, 56, 60, 64, 68, 72, 76, 77, 78, 79] {
        let mut invalid = good;
        invalid[offset] ^= 255;
        assert!(
            ResponseHeader::from_bytes(invalid).is_err(),
            "accepted header offset {offset}"
        );
    }
    for (offset, value) in [(40, 0u32), (44, 0), (40, u32::MAX), (44, u32::MAX)] {
        let mut invalid = good;
        invalid[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(ResponseHeader::from_bytes(invalid).is_err());
    }
    for status in [
        Status::Busy,
        Status::InvalidRequest,
        Status::CaptureFailed,
        Status::DeadlineExceeded,
        Status::IncompatibleDisplay,
        Status::ShuttingDown,
    ] {
        let error = ResponseHeader::error(status, 5, 7).unwrap();
        let bytes = error.to_bytes();
        assert_eq!(bytes[5], status as u8);
        assert!(bytes[24..76].iter().all(|byte| *byte == 0));
        assert_eq!(ResponseHeader::from_bytes(bytes).unwrap(), error);
        let mut invalid = bytes;
        invalid[72] = 1;
        assert!(ResponseHeader::from_bytes(invalid).is_err());
    }
    assert!(ResponseHeader::error(Status::Ok, 1, 1).is_err());
    assert!(ResponseHeader::error(Status::Busy, 1, 0).is_err());
}


#[test]
fn persistent_capture_uses_one_connection_and_fresh_distinct_identities() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let peer = thread::spawn(move || {
        let (mut socket, first) = accept_request(&listener);
        assert_eq!(
            (integer(&first, 8), integer(&first, 16), integer(&first, 24)),
            (0, 1, 0)
        );
        let pixels = [
            0, 0, 0, 2, 2, 2, 1, 2, 3, 9, 8, 7, 30, 20, 10, 100, 110, 120,
        ];
        let bytes = header(91, 1, 101, 1, 3, 2);
        socket.write_all(&bytes[..11]).unwrap();
        socket.write_all(&bytes[11..]).unwrap();
        socket.write_all(&pixels[..5]).unwrap();
        socket.write_all(&pixels[5..]).unwrap();
        let second = read_request(&mut socket);
        assert_eq!(
            (
                integer(&second, 8),
                integer(&second, 16),
                integer(&second, 24)
            ),
            (91, 2, 1)
        );
        socket.write_all(&header(91, 2, 105, 1, 3, 2)).unwrap();
        socket.write_all(&[3; 18]).unwrap();
    });
    let mut client = ScreenCaptureClient::new(Some(address));
    let (first, timing) = client.capture_full().unwrap();
    assert_eq!((first.width, first.height, first.stride), (3, 2, 12));
    assert_eq!(
        first.pixels,
        [
            0, 0, 0, 255, 2, 2, 2, 255, 3, 2, 1, 255, 7, 8, 9, 255, 10, 20, 30, 255, 120, 110, 100,
            255
        ]
    );
    assert_eq!(
        (timing.metadata.protocol_version, timing.metadata.frame_id),
        (3, 101)
    );
    assert_eq!(
        (
            timing.metadata.request_bytes,
            timing.metadata.response_header_bytes,
            timing.metadata.decoded_bytes,
            timing.metadata.payload_bytes
        ),
        (32, 80, 18, 18)
    );
    assert!(!timing.metadata.reused_connection);
    assert_eq!(
        timing.request_ms,
        timing.request_write_ms + timing.header_wait_ms
    );
    assert!(
        timing.total_ms
            >= timing.connect_ms
                + timing.request_ms
                + timing.pixel_read_ms
                + timing.allocation_ms
                + timing.canvas_prepare_ms
                + timing.conversion_ms
    );
    let (second, timing) = client.capture_full().unwrap();
    assert_eq!(
        (
            timing.metadata.session_id,
            timing.metadata.request_id,
            timing.metadata.frame_id
        ),
        (91, 2, 105)
    );
    assert_eq!(timing.connect_ms, 0.0);
    assert!(timing.metadata.reused_connection);
    assert!(
        second
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [3, 3, 3, 255])
    );
    // A later response cannot overwrite previously published owned bytes.
    assert_eq!(&first.pixels[0..8], &[0, 0, 0, 255, 2, 2, 2, 255]);
    peer.join().unwrap();
}


#[test]
fn explicit_nonpersistent_bgr_mode_restarts_session_each_capture() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let peer = thread::spawn(move || {
        for session in 1..=2 {
            let (mut socket, request) = accept_request(&listener);
            assert_eq!(
                (
                    integer(&request, 8),
                    integer(&request, 16),
                    integer(&request, 24)
                ),
                (0, 1, 0)
            );
            socket.write_all(&header(session, 1, 1, 1, 1, 1)).unwrap();
            socket.write_all(&[1, 2, 3]).unwrap();
        }
    });
    let mut client = ScreenCaptureClient::with_mode(Some(address), CaptureMode::BgrV3);
    for _ in 0..2 {
        let (frame, timing) = client.capture_full().unwrap();
        assert_eq!(frame.pixels, [3, 2, 1, 255]);
        assert!(!timing.metadata.reused_connection);
    }
    peer.join().unwrap();
}


#[test]
fn invalid_headers_are_rejected_without_reading_or_allocating_the_pixel_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let offsets = [
        0, 4, 5, 6, 7, 40, 44, 48, 52, 56, 60, 64, 68, 72, 76, 77, 78,
    ];
    let peer = thread::spawn(move || {
        for offset in offsets {
            let (mut socket, _) = accept_request(&listener);
            let mut invalid = header(1, 1, 1, 1, 1, 1);
            invalid[offset] ^= 255;
            socket.write_all(&invalid).unwrap();
        }
    });
    let mut client = ScreenCaptureClient::new(Some(address));
    for _ in offsets {
        assert!(matches!(
            client.capture_full(),
            Err(TcpCaptureError::ProtocolError(_))
        ));
    }
    peer.join().unwrap();
}


#[test]
fn session_request_frame_generation_and_dimensions_are_enforced_across_reuse() {
    for fault in 0..5 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let peer = thread::spawn(move || {
            let (mut socket, _) = accept_request(&listener);
            socket.write_all(&header(7, 1, 10, 1, 1, 1)).unwrap();
            socket.write_all(&[0, 0, 0]).unwrap();
            read_request(&mut socket);
            let mut values = (7, 2, 11, 1, 1, 1);
            match fault {
                0 => values.0 = 8,
                1 => values.1 = 1,
                2 => values.2 = 10,
                3 => values.3 = 2,
                4 => values.4 = 2,
                _ => unreachable!(),
            }
            socket
                .write_all(&header(
                    values.0, values.1, values.2, values.3, values.4, values.5,
                ))
                .unwrap();
            // No payload: validation must fail before trying to read one.
        });
        let mut client = ScreenCaptureClient::new(Some(address));
        client.capture_full().unwrap();
        assert!(
            matches!(
                client.capture_full(),
                Err(TcpCaptureError::ProtocolError(_))
            ),
            "fault {fault}"
        );
        peer.join().unwrap();
    }
}


#[test]
fn truncated_payload_and_error_status_discard_session_then_later_capture_reconnects() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let peer = thread::spawn(move || {
        {
            let (mut socket, _) = accept_request(&listener);
            socket.write_all(&header(4, 1, 1, 1, 2, 1)).unwrap();
            socket.write_all(&[0]).unwrap();
        }
        {
            let (mut socket, request) = accept_request(&listener);
            assert_eq!(
                (
                    integer(&request, 8),
                    integer(&request, 16),
                    integer(&request, 24)
                ),
                (0, 1, 0)
            );
            let mut bytes = [0; 80];
            bytes[0..4].copy_from_slice(b"XSCP");
            bytes[4] = 3;
            bytes[5] = 1; // Busy.
            bytes[8..16].copy_from_slice(&5u64.to_le_bytes());
            bytes[16..24].copy_from_slice(&1u64.to_le_bytes());
            bytes[77] = 1;
            socket.write_all(&bytes).unwrap();
        }
        let (mut socket, request) = accept_request(&listener);
        assert_eq!(
            (
                integer(&request, 8),
                integer(&request, 16),
                integer(&request, 24)
            ),
            (0, 1, 0)
        );
        socket.write_all(&header(6, 1, 1, 1, 1, 1)).unwrap();
        socket.write_all(&[3, 2, 1]).unwrap();
    });
    let mut client = ScreenCaptureClient::new(Some(address));
    assert!(matches!(
        client.capture_full(),
        Err(TcpCaptureError::IoError(_))
    ));
    assert!(
        matches!(client.capture_full(), Err(TcpCaptureError::GuestError(message)) if message == "Busy")
    );
    let (frame, timing) = client.capture_full().unwrap();
    assert_eq!(frame.pixels, [1, 2, 3, 255]);
    assert_eq!(timing.metadata.session_id, 6);
    assert!(!timing.metadata.reused_connection);
    peer.join().unwrap();
}


#[test]
fn truncated_headers_and_unsolicited_trailing_data_never_publish_a_frame() {
    for trailing in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let peer = thread::spawn(move || {
            let (mut socket, _) = accept_request(&listener);
            let mut bytes = header(1, 1, 1, 1, 1, 1).to_vec();
            if trailing {
                bytes.extend_from_slice(&[1, 2, 3, 99]);
                socket.write_all(&bytes).unwrap();
            } else {
                socket.write_all(&bytes[..20]).unwrap();
            }
        });
        let mut client = ScreenCaptureClient::new(Some(address));
        let result = client.capture_full();
        if trailing {
            assert!(matches!(result, Err(TcpCaptureError::ProtocolError(_))));
        } else {
            assert!(matches!(result, Err(TcpCaptureError::IoError(_))));
        }
        peer.join().unwrap();
    }
}


#[test]
fn cancellation_during_header_wait_closes_before_any_later_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancelled);
    let (ready_tx, ready_rx) = mpsc::channel();
    let peer = thread::spawn(move || {
        let (mut socket, _) = accept_request(&listener);
        ready_tx.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).unwrap(), 0);
    });
    let trigger = thread::spawn(move || {
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        flag.store(true, Ordering::Release);
    });
    let mut client = ScreenCaptureClient::new(Some(address));
    let start = Instant::now();
    assert!(matches!(
        client.capture_full_cancellable(&cancelled),
        Err(TcpCaptureError::Cancelled)
    ));
    assert!(start.elapsed() < Duration::from_secs(2));
    trigger.join().unwrap();
    peer.join().unwrap();
}
