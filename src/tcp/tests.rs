//! Private client checks for deadlines, cancellation and connection-state invalidation.


use super::*;
use std::net::TcpListener;
use std::sync::{Arc, mpsc};
use std::thread;


/// Create a loopback socket pair with a bounded connection attempt.
fn socket_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client =
        TcpStream::connect_timeout(&listener.local_addr().unwrap(), Duration::from_secs(1))
            .unwrap();
    let (peer, _) = listener.accept().unwrap();
    (client, peer)
}


#[test]
fn defaults_are_persistent_v3_and_existing_error_messages_remain_useful() {
    let client = ScreenCaptureClient::new(None);
    assert_eq!(client.server_addr, DEFAULT_SERVER_ADDR);
    assert_eq!(client.mode, CaptureMode::PersistentBgrV3);
    assert!(client.stream.is_none());
    assert_eq!(
        TcpCaptureError::ConnectionFailed("refused".to_string()).to_string(),
        "TCP connection failed: refused"
    );
    assert_eq!(
        TcpCaptureError::Timeout.to_string(),
        "TCP operation timed out"
    );
    assert_eq!(
        TcpCaptureError::Cancelled.to_string(),
        "TCP capture cancelled"
    );
    let timing = TcpCaptureTiming::default();
    assert_eq!(timing.connect_ms, 0.0);
    assert_eq!(timing.canvas_prepare_ms, 0.0);
    assert_eq!(timing.metadata.session_id, 0);
}


#[test]
fn cancelled_before_connect_never_attempts_a_request_or_retains_identity() {
    let mut client = ScreenCaptureClient::new(Some("127.0.0.1:1".to_string()));
    client.session_id = 7;
    assert!(matches!(
        client.capture_full_cancellable(&AtomicBool::new(true)),
        Err(TcpCaptureError::Cancelled)
    ));
    assert_eq!(client.session_id, 0);
    assert!(client.stream.is_none());
}


#[test]
fn numeric_addresses_avoid_unbounded_name_resolution() {
    let mut client = ScreenCaptureClient::new(Some("localhost:9999".to_string()));
    assert!(
        matches!(client.capture_full(), Err(TcpCaptureError::ConnectionFailed(message))
        if message.contains("numeric IP address"))
    );
    assert!(client.stream.is_none());
}


#[test]
fn conversion_preserves_black_near_black_and_inserts_opaque_alpha() {
    let mut rgba = [0; 12];
    bgr_to_rgba(
        &[0, 0, 0, 2, 2, 2, 9, 8, 7],
        &mut rgba,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(rgba, [0, 0, 0, 255, 2, 2, 2, 255, 7, 8, 9, 255]);
    assert!(bgr_to_rgba(&[1], &mut rgba, &AtomicBool::new(false)).is_err());
    assert!(bgr_to_rgba(&[0; 6], &mut rgba, &AtomicBool::new(false)).is_err());
    assert!(matches!(
        bgr_to_rgba(&[0; 9], &mut rgba, &AtomicBool::new(true)),
        Err(TcpCaptureError::Cancelled)
    ));
}


#[test]
fn absolute_read_deadline_is_not_reset_by_partial_progress() {
    let (mut socket, mut peer) = socket_pair();
    let writer = thread::spawn(move || {
        for _ in 0..20 {
            if peer.write_all(&[1]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    });
    let start = Instant::now();
    let result = read_bounded(
        &mut socket,
        &mut [0; 20],
        &AtomicBool::new(false),
        Duration::from_millis(80),
    );
    assert!(matches!(result, Err(TcpCaptureError::Timeout)));
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(socket);
    writer.join().unwrap();
}


#[test]
fn cancellation_interrupts_a_stalled_payload_without_waiting_for_total_deadline() {
    let (mut socket, _peer) = socket_pair();
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancelled);
    let trigger = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        flag.store(true, Ordering::Release);
    });
    let start = Instant::now();
    assert!(matches!(
        read_bounded(
            &mut socket,
            &mut [0; 3],
            &cancelled,
            Duration::from_secs(10)
        ),
        Err(TcpCaptureError::Cancelled)
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
    trigger.join().unwrap();
}


#[test]
fn header_timeout_invalidates_stream_and_session_before_a_later_operation() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = ScreenCaptureClient::new(Some(listener.local_addr().unwrap().to_string()));
    client.socket_timeout = Duration::from_millis(50);
    let (release_tx, release_rx) = mpsc::channel();
    let peer = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut request = [0; protocol::v3::REQUEST_LEN];
        socket.read_exact(&mut request).unwrap();
        release_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    });
    assert!(matches!(
        client.capture_full(),
        Err(TcpCaptureError::Timeout)
    ));
    assert!(client.stream.is_none());
    assert_eq!(
        (
            client.session_id,
            client.display_generation,
            client.last_frame_id
        ),
        (0, 0, 0)
    );
    release_tx.send(()).unwrap();
    peer.join().unwrap();
}


#[test]
fn cancellation_and_elapsed_deadline_are_checked_before_any_io() {
    assert!(matches!(
        poll_remaining(
            &AtomicBool::new(true),
            Instant::now() + Duration::from_secs(1)
        ),
        Err(TcpCaptureError::Cancelled)
    ));
    assert!(matches!(
        poll_remaining(&AtomicBool::new(false), Instant::now()),
        Err(TcpCaptureError::Timeout)
    ));
}


#[test]
fn positively_observed_eof_preserves_the_frame_and_next_capture_connects_directly() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = ScreenCaptureClient::new(Some(listener.local_addr().unwrap().to_string()));
    let (mut first_socket, mut first_peer) = socket_pair();
    let first = thread::spawn(move || {
        first_peer
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut bytes = [0; protocol::v3::REQUEST_LEN];
        first_peer.read_exact(&mut bytes).unwrap();
        let request = protocol::v3::Request::from_bytes(bytes).unwrap();
        let response =
            protocol::v3::ResponseHeader::success(7, request.request_id(), 11, 1, 1, 1).unwrap();
        first_peer.write_all(&response.to_bytes()).unwrap();
        first_peer.write_all(&[3, 2, 1]).unwrap();
        first_peer.shutdown(std::net::Shutdown::Write).unwrap();
    });
    let mut timing = TcpCaptureTiming::default();
    let frame = client
        .capture_v3(&mut first_socket, &AtomicBool::new(false), &mut timing)
        .unwrap();
    assert_eq!(frame.pixels, [1, 2, 3, 255]);
    assert_eq!(client.session_id, 7);
    // Wait for positive EOF so the finaliser test has no FIN-arrival timing race.
    first_socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    assert_eq!(first_socket.read(&mut [0; 1]).unwrap(), 0);
    client.finish_response_connection(first_socket).unwrap();
    assert!(client.stream.is_none());
    assert_eq!(
        (
            client.session_id,
            client.display_generation,
            client.last_frame_id
        ),
        (0, 0, 0)
    );
    first.join().unwrap();
    let second = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut bytes = [0; protocol::v3::REQUEST_LEN];
        socket.read_exact(&mut bytes).unwrap();
        let request = protocol::v3::Request::from_bytes(bytes).unwrap();
        assert_eq!(
            (
                request.session_id(),
                request.request_id(),
                request.display_generation()
            ),
            (0, 1, 0)
        );
        let response = protocol::v3::ResponseHeader::success(8, 1, 12, 1, 1, 1).unwrap();
        socket.write_all(&response.to_bytes()).unwrap();
        socket.write_all(&[6, 5, 4]).unwrap();
    });
    let (next, timing) = client.capture_full().unwrap();
    assert_eq!(next.pixels, [4, 5, 6, 255]);
    assert_eq!(timing.metadata.session_id, 8);
    assert!(!timing.metadata.reused_connection);
    assert_eq!(frame.pixels, [1, 2, 3, 255]);
    second.join().unwrap();
}


/// Accept one expected loopback connection within two seconds and bound its peer I/O.
fn accept_idle_test_peer(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let socket = loop {
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "expected capture connection did not arrive"
                );
                thread::yield_now();
            }
            Err(error) => panic!("capture peer accept failed: {error}"),
        }
    };
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
}


/// Read and validate one request using the peer's already bounded socket timeout.
fn read_idle_test_request(socket: &mut TcpStream) -> protocol::v3::Request {
    let mut bytes = [0; protocol::v3::REQUEST_LEN];
    socket.read_exact(&mut bytes).unwrap();
    protocol::v3::Request::from_bytes(bytes).unwrap()
}


/// Return a public capture with its original server socket still owned and open.
fn retained_idle_test_client() -> (
    ScreenCaptureClient,
    TcpStream,
    TcpListener,
    TcpCapturedFrame,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server_listener = listener.try_clone().unwrap();
    let mut client = ScreenCaptureClient::new(Some(listener.local_addr().unwrap().to_string()));
    client.socket_timeout = Duration::from_secs(2);
    let peer = thread::spawn(move || {
        let mut socket = accept_idle_test_peer(&server_listener);
        let request = read_idle_test_request(&mut socket);
        assert_eq!(
            (
                request.session_id(),
                request.request_id(),
                request.display_generation()
            ),
            (0, 1, 0)
        );
        let header = protocol::v3::ResponseHeader::success(7, 1, 11, 9, 1, 1).unwrap();
        socket.write_all(&header.to_bytes()).unwrap();
        socket.write_all(&[3, 2, 1]).unwrap();
        // The join result owns the live socket until the caller receives it.
        socket
    });
    let (frame, timing) = client.capture_full().unwrap();
    let peer = peer.join().unwrap();
    assert!(client.stream.is_some());
    assert_eq!(
        (
            client.session_id,
            client.next_request_id,
            client.display_generation,
            client.last_frame_id
        ),
        (7, 2, 9, 11)
    );
    assert_eq!(client.display_dimensions, Some((1, 1)));
    assert!(!timing.metadata.reused_connection);
    assert_eq!(frame.pixels, [1, 2, 3, 255]);
    (client, peer, listener, frame)
}


/// Positively observe EOF or pending bytes without consuming data or resetting client state.
fn assert_idle_test_peek(client: &ScreenCaptureClient, expected: usize) {
    let socket = client.stream.as_ref().unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    assert_eq!(socket.peek(&mut [0; 1]).unwrap(), expected);
}


/// Require error invalidation to close the original peer, send no request and open no replacement.
fn assert_idle_test_refused(
    client: &ScreenCaptureClient,
    peer: &mut TcpStream,
    listener: &TcpListener,
) {
    assert!(client.stream.is_none());
    assert_eq!(
        (
            client.session_id,
            client.next_request_id,
            client.display_generation,
            client.last_frame_id
        ),
        (0, 1, 0, 0)
    );
    assert_eq!(client.display_dimensions, None);
    match peer.read(&mut [0; 1]) {
        Ok(0) => {}
        Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {}
        result => {
            panic!("refused capture unexpectedly sent data or kept its peer open: {result:?}")
        }
    }
    listener.set_nonblocking(true).unwrap();
    assert!(matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
}


/// EOF arriving after a returned frame reconnects before sending and resets all wire identities.
#[test]
fn idle_eof_before_the_next_capture_reconnects_without_sending_on_the_old_socket() {
    let (mut client, mut old_peer, listener, first) = retained_idle_test_client();
    old_peer.shutdown(std::net::Shutdown::Write).unwrap();
    assert_idle_test_peek(&client, 0);
    let next_peer = thread::spawn(move || {
        let mut socket = accept_idle_test_peer(&listener);
        let request = read_idle_test_request(&mut socket);
        assert_eq!(
            (
                request.session_id(),
                request.request_id(),
                request.display_generation()
            ),
            (0, 1, 0)
        );
        let header = protocol::v3::ResponseHeader::success(8, 1, 1, 10, 2, 1).unwrap();
        socket.write_all(&header.to_bytes()).unwrap();
        socket.write_all(&[6, 5, 4, 9, 8, 7]).unwrap();
        socket
    });
    let (second, timing) = client.capture_full().unwrap();
    let _next_peer = next_peer.join().unwrap();
    assert_eq!(old_peer.read(&mut [0; 1]).unwrap(), 0);
    assert_eq!(
        (
            timing.metadata.session_id,
            timing.metadata.request_id,
            timing.metadata.frame_id,
            timing.metadata.display_generation
        ),
        (8, 1, 1, 10)
    );
    assert!(!timing.metadata.reused_connection);
    assert_eq!((second.width, second.height), (2, 1));
    assert_eq!(second.pixels, [4, 5, 6, 255, 7, 8, 9, 255]);
    assert_eq!(first.pixels, [1, 2, 3, 255]);
}


/// Cancellation wins over a positively observed stale socket and never reconnects or sends.
#[test]
fn cancellation_with_idle_eof_discards_the_socket_without_a_new_connection() {
    let (mut client, mut peer, listener, _first) = retained_idle_test_client();
    peer.shutdown(std::net::Shutdown::Write).unwrap();
    assert_idle_test_peek(&client, 0);
    assert!(matches!(
        client.capture_full_cancellable(&AtomicBool::new(true)),
        Err(TcpCaptureError::Cancelled)
    ));
    assert_idle_test_refused(&client, &mut peer, &listener);
}


/// Idle unsolicited bytes remain a protocol failure rather than permission to reconnect.
#[test]
fn unsolicited_idle_data_refuses_the_capture_without_sending_or_reconnecting() {
    let (mut client, mut peer, listener, _first) = retained_idle_test_client();
    peer.write_all(&[99]).unwrap();
    assert_idle_test_peek(&client, 1);
    assert!(
        matches!(client.capture_full(), Err(TcpCaptureError::ProtocolError(message)) if message == "unsolicited or trailing capture response bytes")
    );
    assert_idle_test_refused(&client, &mut peer, &listener);
}


/// Failure after a reused request was sent never triggers a same-call reconnect or replay.
#[test]
fn truncated_reused_response_requires_a_later_explicit_capture_to_reconnect() {
    let (mut client, mut peer, listener, first) = retained_idle_test_client();
    let truncated = thread::spawn(move || {
        let request = read_idle_test_request(&mut peer);
        assert_eq!(
            (
                request.session_id(),
                request.request_id(),
                request.display_generation()
            ),
            (7, 2, 9)
        );
        let header = protocol::v3::ResponseHeader::success(7, 2, 12, 9, 1, 1).unwrap();
        peer.write_all(&header.to_bytes()).unwrap();
        peer.write_all(&[6]).unwrap();
        peer.shutdown(std::net::Shutdown::Write).unwrap();
        peer
    });
    assert!(
        matches!(client.capture_full(), Err(TcpCaptureError::IoError(message)) if message == "truncated capture response")
    );
    let mut peer = truncated.join().unwrap();
    assert_idle_test_refused(&client, &mut peer, &listener);
    let explicit = thread::spawn(move || {
        let mut socket = accept_idle_test_peer(&listener);
        let request = read_idle_test_request(&mut socket);
        assert_eq!(
            (
                request.session_id(),
                request.request_id(),
                request.display_generation()
            ),
            (0, 1, 0)
        );
        let header = protocol::v3::ResponseHeader::success(8, 1, 1, 10, 1, 1).unwrap();
        socket.write_all(&header.to_bytes()).unwrap();
        socket.write_all(&[6, 5, 4]).unwrap();
        socket
    });
    let (next, timing) = client.capture_full().unwrap();
    let _peer = explicit.join().unwrap();
    assert!(!timing.metadata.reused_connection);
    assert_eq!(next.pixels, [4, 5, 6, 255]);
    assert_eq!(first.pixels, [1, 2, 3, 255]);
}
