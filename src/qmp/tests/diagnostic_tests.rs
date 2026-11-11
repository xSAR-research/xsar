//! Passive wire observation and guarded diagnostic input regressions.

use std::sync::{Mutex, atomic::AtomicBool};

use super::*;


/// Adding a passive observer preserves the existing client's public auto-trait bounds.
#[test]
fn qmp_316_observer_preserves_client_thread_and_unwind_traits() {
    /// Require the pre-existing public thread and unwind marker compatibility at compile time.
    fn require<T: Send + Sync + std::panic::UnwindSafe + std::panic::RefUnwindSafe>() {}
    require::<QmpClient>();
}


/// Owned observation payload retained independently of the synchronous callback borrow.
#[derive(Clone, Debug)]
enum SeenKind {
    /// Exact compact intended JSON before its first write.
    Attempt(String),
    /// Complete JSON/newline/flush without any acknowledgement claim.
    Flushed,
    /// Original write-stage failure and its unchanged displayed error.
    Failed(QmpWireStage, String),
}


/// One caller-owned observation with a monotonic callback timestamp.
#[derive(Clone, Debug)]
struct Seen {
    /// Original or fresh release-only stream within this observer instance.
    connection: QmpWireConnection,
    /// Numeric request identifier scoped to that physical stream.
    id: u64,
    /// Owned boundary payload, never an input-authority token.
    kind: SeenKind,
    /// Monotonic callback time used to check requested pacing lower bounds.
    at: Instant,
}


/// Passive local recorder that can deliberately unwind after releasing its own lock.
struct Recorder {
    /// Exact observations in synchronous callback order.
    events: Mutex<Vec<Seen>>,
    /// Whether callback unwinding must be contained at every boundary.
    panic: bool,
}


impl Recorder {
    /// Construct an empty caller-owned recorder without transport or filesystem effects.
    fn new(panic: bool) -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            panic,
        }
    }


    /// Copy the bounded local test trace after an operation completes.
    fn snapshot(&self) -> Vec<Seen> {
        self.events.lock().unwrap().clone()
    }
}


impl QmpWireObserver for Recorder {
    /// Record the exact borrowed boundary, then optionally panic outside the recorder lock.
    fn observe(&self, event: QmpWireEvent<'_>) {
        let (connection, id, kind) = match event {
            QmpWireEvent::Attempted {
                connection,
                id,
                json,
            } => (connection, id, SeenKind::Attempt(json.to_owned())),
            QmpWireEvent::Flushed { connection, id } => (connection, id, SeenKind::Flushed),
            QmpWireEvent::Failed {
                connection,
                id,
                stage,
                error,
            } => (connection, id, SeenKind::Failed(stage, error.to_string())),
        };
        self.events.lock().unwrap().push(Seen {
            connection,
            id,
            kind,
            at: Instant::now(),
        });
        assert!(!self.panic, "injected passive observer panic");
    }
}


/// Bound mock I/O while preserving the real fifty-millisecond final click hold.
fn config() -> QmpConfig {
    QmpConfig {
        io_timeout: Duration::from_secs(5),
        pointer_settle_delay: Duration::ZERO,
        mouse_hold: Duration::from_millis(50),
        ..QmpConfig::default()
    }
}


/// Read one actual newline-delimited socket request without reserializing its text.
fn read_raw(reader: &mut BufReader<UnixStream>) -> (String, Value) {
    let mut line = String::new();
    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
    assert!(line.ends_with('\n'));
    let packet = serde_json::from_str(&line).unwrap();
    line.pop();
    (line, packet)
}


/// Negotiate a valid greeting while retaining the exact first outgoing capability JSON.
fn handshake(stream: &mut UnixStream, reader: &mut BufReader<UnixStream>) -> String {
    write_packet(
        stream,
        &json!({"QMP":{"version":{"qemu":{"major":11,"minor":1,"micro":1}},"capabilities":[]}}),
    );
    let (raw, packet) = read_raw(reader);
    assert_eq!(packet["execute"], "qmp_capabilities");
    acknowledge(stream, &packet);
    raw
}


/// Require a single left-button transition, never an implicit pointer move.
fn button(packet: &Value, down: bool) {
    assert_eq!(
        packet["arguments"],
        json!({"events":[{"type":"btn","data":{"button":"left","down":down}}]})
    );
}


/// The public observer sees exact handshake/probe bytes and preserves a genuine failed write.
#[test]
fn qmp_316_wire_exact_handshake_probe_and_write_failure() {
    let path = SocketFile(std::env::temp_dir().join(format!(
        "xsar-wire-{}-{}.sock",
        process::id(),
        NEXT_SOCKET_ID.fetch_add(1, Ordering::Relaxed)
    )));
    let listener = UnixListener::bind(&path.0).unwrap();
    listener.set_nonblocking(true).unwrap();
    let peer = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener);
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut received = vec![handshake(&mut stream, &mut reader)];
        for command in ["query-status", "query-mice"] {
            let (raw, packet) = read_raw(&mut reader);
            assert_eq!(packet["execute"], command);
            received.push(raw);
            acknowledge(&mut stream, &packet);
        }
        let mut trailing = Vec::new();
        reader.read_to_end(&mut trailing).unwrap();
        assert!(trailing.is_empty());
        received
    });
    let observer = Arc::new(Recorder::new(false));
    let mut client = QmpClient::connect_with_observer(&path.0, config(), observer.clone()).unwrap();
    client.probe().unwrap();
    client.writer.shutdown(Shutdown::Write).unwrap();
    let error = client.execute("query-status", None).unwrap_err();
    assert!(matches!(error, QmpError::Json(_)));
    let received = peer.join().unwrap();
    let events = observer.snapshot();
    assert_eq!(events.len(), 8);
    for (index, raw) in received.iter().enumerate() {
        let attempt = &events[index * 2];
        assert_eq!(attempt.connection, QmpWireConnection::Primary);
        assert_eq!(attempt.id, u64::try_from(index).unwrap() + 1);
        assert!(matches!(&attempt.kind, SeenKind::Attempt(json) if json == raw));
        assert!(matches!(events[index * 2 + 1].kind, SeenKind::Flushed));
    }
    assert_eq!(events[6].id, 4);
    assert!(
        matches!(&events[7].kind, SeenKind::Failed(QmpWireStage::JsonWrite, text) if text == &error.to_string())
    );
}


/// Invalid and mapped-alias ABS endpoints, zero REL direction and initial STOP emit no requests.
#[test]
fn qmp_318_preflight_refuses_before_inventory_or_input() {
    let (stream, server) = UnixStream::pair().unwrap();
    let peer = thread::spawn(move || {
        let mut peer = super::relative_tests::Peer::new(server, 7, 35);
        peer.greet();
        peer.finish()
    });
    let mut client =
        QmpClient::from_connected_stream_with_config(Path::new("mock"), stream, config()).unwrap();
    let mut checks = 0;
    let error = client
        .diagnostic_drag(
            PixelPoint::new(0, 0),
            PixelPoint::new(1, 0),
            65_536,
            1,
            || {
                checks += 1;
                true
            },
        )
        .unwrap_err();
    assert!(
        matches!(error,QmpError::Protocol(ref message) if message=="drag endpoints must differ")
    );
    assert_eq!(checks, 0);
    for destination in [
        PixelPoint::new(-1, 100),
        PixelPoint::new(1920, 100),
        PixelPoint::new(100, 1080),
    ] {
        assert!(
            client
                .diagnostic_drag(PixelPoint::new(100, 100), destination, 1920, 1080, || true)
                .is_err()
        );
        assert!(
            client
                .diagnostic_relative_drag(
                    PixelPoint::new(100, 100),
                    destination,
                    1920,
                    1080,
                    || true
                )
                .is_err()
        );
    }
    for source in [
        PixelPoint::new(-1, 100),
        PixelPoint::new(1920, 100),
        PixelPoint::new(100, 1080),
    ] {
        assert!(
            client
                .diagnostic_drag(source, PixelPoint::new(200, 200), 1920, 1080, || true)
                .is_err()
        );
    }
    for (width, height) in [(0, 1080), (1920, 0)] {
        assert!(
            client
                .diagnostic_drag(
                    PixelPoint::new(100, 100),
                    PixelPoint::new(200, 200),
                    width,
                    height,
                    || true
                )
                .is_err()
        );
    }
    assert!(
        client
            .diagnostic_relative_drag(
                PixelPoint::new(100, 100),
                PixelPoint::new(100, 100),
                1920,
                1080,
                || true
            )
            .is_err()
    );
    for relative in [false, true] {
        assert!(super::relative_tests::run(&mut client, relative, || false).is_err());
    }
    drop(client);
    assert_eq!(peer.join().unwrap().len(), 1);
}


/// A separate guarded click remains a movement-free DOWN/UP pair with its configured hold.
#[test]
fn qmp_316_separate_current_click_retains_configured_hold() {
    let (stream, mut server) = UnixStream::pair().unwrap();
    let peer = thread::spawn(move || {
        let mut reader = BufReader::new(server.try_clone().unwrap());
        let mut wire = vec![handshake(&mut server, &mut reader)];
        let (raw, down) = read_raw(&mut reader);
        button(&down, true);
        wire.push(raw);
        let (raw, up) = read_raw(&mut reader);
        button(&up, false);
        wire.push(raw);
        for packet in [&up, &down] {
            acknowledge(&mut server, packet);
        }
        let mut trailing = Vec::new();
        reader.read_to_end(&mut trailing).unwrap();
        assert!(trailing.is_empty());
        wire
    });
    let observer = Arc::new(Recorder::new(false));
    let mut client = QmpClient::from_connected_stream_observed(
        Path::new("mock"),
        stream,
        config(),
        Some(observer.clone()),
        QmpWireConnection::Primary,
    )
    .unwrap();
    client.click_current_guarded(|| true).unwrap();
    assert!(client.click_current_guarded(|| false).is_err());
    drop(client);
    let actual = peer.join().unwrap();
    let attempts: Vec<_> = observer
        .snapshot()
        .into_iter()
        .filter(|event| matches!(event.kind, SeenKind::Attempt(_)))
        .collect();
    assert_eq!(attempts.len(), 3);
    for (event, raw) in attempts.iter().zip(actual) {
        assert!(matches!(&event.kind,SeenKind::Attempt(json) if json==&raw));
    }
    assert!(attempts[2].at.duration_since(attempts[1].at) >= Duration::from_millis(50));
}


/// Passive observer panics cannot replace guarded-click STOP/error causes or release recovery.
#[test]
fn qmp_316_input_failure_stop_recovery_and_observer_panics() {
    for stop_after_down in [false, true] {
        let path = SocketFile(std::env::temp_dir().join(format!(
            "xsar-click-release-{}-{}.sock",
            process::id(),
            NEXT_SOCKET_ID.fetch_add(1, Ordering::Relaxed)
        )));
        let listener = UnixListener::bind(&path.0).unwrap();
        listener.set_nonblocking(true).unwrap();
        let peer = thread::spawn(move || {
            let mut stream = accept_with_timeout(&listener);
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            start_mock_qmp(&mut stream, &mut reader);
            let down = read_request(&mut reader);
            button(&down, true);
            if !stop_after_down {
                let up = read_request(&mut reader);
                button(&up, false);
                write_packet(
                    &mut stream,
                    &json!({"error":{"class":"GenericError",
                    "desc":"diagnostic peer rejected input"},"id":request_id(&down)}),
                );
            }
            let up = read_request(&mut reader);
            button(&up, false);
            let mut trailing = Vec::new();
            reader.read_to_end(&mut trailing).unwrap();
            assert!(trailing.is_empty());
            drop(reader);
            drop(stream);
            let mut stream = accept_with_timeout(&listener);
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            start_mock_qmp(&mut stream, &mut reader);
            let up = read_request(&mut reader);
            button(&up, false);
            acknowledge(&mut stream, &up);
            let mut trailing = Vec::new();
            reader.read_to_end(&mut trailing).unwrap();
            assert!(trailing.is_empty());
        });
        let observer = Arc::new(Recorder::new(true));
        let mut client =
            QmpClient::connect_with_observer(&path.0, config(), observer.clone()).unwrap();
        let mut checks = 0;
        let error = client
            .click_current_guarded(|| {
                checks += 1;
                !stop_after_down || checks < 3
            })
            .unwrap_err();
        if stop_after_down {
            assert!(
                matches!(error,QmpError::Protocol(ref s) if s=="diagnostic input authority interrupted")
            );
        } else {
            assert!(
                matches!(error,QmpError::Command(ref s) if s=="diagnostic peer rejected input")
            );
        }
        assert!(!client.usable);
        drop(client);
        peer.join().unwrap();
        let recovery: Vec<_> = observer
            .snapshot()
            .into_iter()
            .filter(|event| event.connection == QmpWireConnection::ReleaseRecovery)
            .collect();
        assert_eq!(recovery.len(), 4);
        assert_eq!(recovery[0].id, 1);
        assert_eq!(recovery[2].id, 2);
    }
}


/// STOP after source ACK sends no DOWN, ordinary restoration or release-recovery requests.
#[test]
fn qmp_316_diagnostic_stop_before_down_sends_only_source() {
    for relative in [false, true] {
        let (stream, server) = UnixStream::pair().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let peer_stop = stop.clone();
        let peer = thread::spawn(move || {
            let mut peer = super::relative_tests::Peer::new(server, 19, 61);
            peer.greet();
            peer.query(peer.relative);
            peer.select(peer.relative, peer.absolute);
            let source = peer.request();
            assert_eq!(source["arguments"]["events"][0]["type"], "abs");
            peer_stop.store(true, Ordering::Release);
            peer.answer(&source, json!({}));
            peer.finish()
        });
        let mut client =
            QmpClient::from_connected_stream_with_config(Path::new("mock"), stream, config())
                .unwrap();
        assert!(
            super::relative_tests::run(&mut client, relative, || !stop.load(Ordering::Acquire))
                .is_err()
        );
        assert!(client.usable);
        drop(client);
        assert_eq!(peer.join().unwrap().len(), 6);
    }
}
