//! Diagnostic pointer routing, bounded movement probes and release-only recovery.

use std::sync::{Arc, Mutex, atomic::AtomicBool};

use super::*;


/// Bound mock socket I/O independently of real diagnostic phase timing.
pub(super) fn config() -> QmpConfig {
    QmpConfig {
        io_timeout: Duration::from_secs(8),
        ..QmpConfig::default()
    }
}


/// Construct the actual QMP schema with deliberately non-default handler indices.
fn inventory(absolute: i32, relative: i32, current: i32) -> Value {
    json!([
        {"index":absolute,"name":"QEMU USB Tablet","absolute":true,"current":current==absolute},
        {"index":relative,"name":"QEMU PS/2 Mouse","absolute":false,"current":current==relative}
    ])
}


/// One exact attempted request and its independently observed write completion.
#[derive(Clone)]
pub(super) struct Wire {
    /// Original or fresh safety-release stream.
    pub(super) connection: QmpWireConnection,
    /// Numeric identifier scoped to that physical stream.
    pub(super) id: u64,
    /// Exact compact JSON before its newline is written.
    pub(super) json: String,
    /// Monotonic pre-write timestamp for real pacing assertions.
    pub(super) at: Instant,
    /// Whether JSON, newline and flush all completed.
    pub(super) flushed: bool,
}


/// Passive recorder with an optional deterministic input-flush STOP trigger.
pub(super) struct Recorder {
    /// Independently owned outgoing evidence.
    wire: Mutex<Vec<Wire>>,
    /// STOP token and input ordinal; zero means the final left UP.
    stop: Option<(Arc<AtomicBool>, usize)>,
}


impl Recorder {
    /// Create bounded observation with an optional STOP at the selected input boundary.
    pub(super) fn new(stop: Option<(Arc<AtomicBool>, usize)>) -> Self {
        Self {
            wire: Mutex::new(Vec::new()),
            stop,
        }
    }


    /// Copy evidence after transport execution has completed.
    pub(super) fn snapshot(&self) -> Vec<Wire> {
        self.wire.lock().unwrap().clone()
    }
}


impl QmpWireObserver for Recorder {
    /// Record borrowed wire evidence and revoke authority only after a chosen completed flush.
    fn observe(&self, event: QmpWireEvent<'_>) {
        match event {
            QmpWireEvent::Attempted {
                connection,
                id,
                json,
            } => {
                self.wire.lock().unwrap().push(Wire {
                    connection,
                    id,
                    json: json.to_owned(),
                    at: Instant::now(),
                    flushed: false,
                });
            }
            QmpWireEvent::Flushed { connection, id } => {
                let mut wire = self.wire.lock().unwrap();
                let packet = wire
                    .iter_mut()
                    .find(|packet| packet.connection == connection && packet.id == id)
                    .unwrap();
                packet.flushed = true;
                if connection == QmpWireConnection::Primary
                    && let Some((stop, ordinal)) = &self.stop
                {
                    let inputs: Vec<Value> = wire
                        .iter()
                        .filter(|packet| packet.connection == QmpWireConnection::Primary)
                        .map(|packet| serde_json::from_str(&packet.json).unwrap())
                        .filter(|packet: &Value| packet["execute"] == "input-send-event")
                        .collect();
                    let up = inputs.last().is_some_and(|packet| {
                        packet["arguments"]
                            == json!({"events":[
                                {"type":"btn","data":{"button":"left","down":false}}
                            ]})
                    });
                    if (*ordinal == 0 && up) || (*ordinal != 0 && inputs.len() == *ordinal) {
                        stop.store(true, Ordering::Release);
                    }
                }
            }
            QmpWireEvent::Failed { .. } => {}
        }
    }
}


/// Independently read socket peer with varied QMP mouse indices.
pub(super) struct Peer {
    /// Socket used only for mock replies.
    pub(super) stream: UnixStream,
    /// Independent socket reader.
    reader: BufReader<UnixStream>,
    /// Exact request text excluding only the terminating newline.
    wire: Vec<String>,
    /// Queried absolute-handler index.
    pub(super) absolute: i32,
    /// Queried relative-handler index.
    pub(super) relative: i32,
}


impl Peer {
    /// Configure bounded reads and independently chosen handler identities.
    pub(super) fn new(stream: UnixStream, absolute: i32, relative: i32) -> Self {
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        Self {
            stream,
            reader,
            wire: Vec::new(),
            absolute,
            relative,
        }
    }


    /// Retain actual serialized bytes before parsing their independent value.
    pub(super) fn request(&mut self) -> Value {
        let mut line = String::new();
        assert!(self.reader.read_line(&mut line).unwrap() > 0);
        self.wire.push(line.strip_suffix('\n').unwrap().to_owned());
        serde_json::from_str(&line).unwrap()
    }


    /// Require the client to wait for an outstanding reply, including already buffered bytes.
    pub(super) fn assert_waiting_for_reply(&mut self) {
        assert!(self.reader.buffer().is_empty());
        self.stream
            .set_read_timeout(Some(Duration::from_millis(40)))
            .unwrap();
        let mut byte = [0];
        assert!(matches!(
            self.reader.read(&mut byte).unwrap_err().kind(),
            ErrorKind::WouldBlock | ErrorKind::TimedOut
        ));
        self.stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
    }


    /// Reply using the request's actual numeric ID.
    pub(super) fn answer(&mut self, request: &Value, payload: Value) {
        write_packet(
            &mut self.stream,
            &json!({"return":payload,"id":request_id(request)}),
        );
    }


    /// Complete capability negotiation.
    pub(super) fn greet(&mut self) {
        write_packet(
            &mut self.stream,
            &json!({"QMP":{"version":{},"capabilities":[]}}),
        );
        let request = self.request();
        assert_eq!(request["execute"], "qmp_capabilities");
        self.answer(&request, json!({}));
    }


    /// Require fresh inventory using actual indices rather than assumed zero/one routing.
    pub(super) fn query(&mut self, current: i32) {
        let request = self.request();
        assert_eq!(request["execute"], "query-mice");
        self.answer(&request, inventory(self.absolute, self.relative, current));
    }


    /// Require fresh pre-selection evidence, restricted HMP and independent verification.
    pub(super) fn select(&mut self, old: i32, new: i32) {
        self.query(old);
        let request = self.request();
        assert_eq!(request["execute"], "human-monitor-command");
        assert_eq!(
            request["arguments"],
            json!({"command-line":format!("mouse_set {new}")})
        );
        self.answer(&request, json!(""));
        self.query(new);
    }


    /// Position through verified ABS and select REL when requested, before the final routing query.
    pub(super) fn position_and_select(&mut self, relative: bool, original: i32) {
        self.greet();
        self.query(original);
        self.select(original, self.absolute);
        let source = self.request();
        assert_eq!(source["execute"], "input-send-event");
        assert_eq!(
            source["arguments"],
            json!({"events":[
                {"type":"abs","data":{"axis":"x","value":341}},
                {"type":"abs","data":{"axis":"y","value":15169}}
            ]})
        );
        self.answer(&source, json!({}));
        if relative {
            self.query(self.absolute);
            self.select(self.absolute, self.relative);
        }
    }


    /// Require a final fresh current-handler verification before DOWN.
    pub(super) fn prepare(&mut self, relative: bool, original: i32) {
        self.position_and_select(relative, original);
        self.query(if relative {
            self.relative
        } else {
            self.absolute
        });
    }


    /// Require the native ABS approach/destination or ten REL movements before the sole UP.
    pub(super) fn gesture(&mut self, relative: bool) -> Vec<Value> {
        let down = self.request();
        button(&down, true);
        let mut requests = vec![down];
        let points = [
            (22, 502),
            (64, 535),
            (106, 568),
            (148, 601),
            (190, 634),
            (232, 667),
            (274, 700),
            (316, 733),
            (358, 766),
            (400, 800),
        ];
        let points = if relative {
            points.as_slice()
        } else {
            &[(400, 796), (400, 800)]
        };
        for &(x, y) in points {
            let moved = self.request();
            assert_eq!(moved["execute"], "input-send-event");
            let expected = if relative {
                json!({"events":[
                    {"type":"rel","data":{"axis":"x","value":2}},
                    {"type":"rel","data":{"axis":"y","value":2}}
                ]})
            } else {
                json!({"events":[
                    {"type":"abs","data":{"axis":"x","value":x * 32767 / 1920}},
                    {"type":"abs","data":{"axis":"y","value":y * 32767 / 1080}}
                ]})
            };
            assert_eq!(moved["arguments"], expected);
            requests.push(moved);
        }
        let up = self.request();
        button(&up, false);
        requests.push(up);
        requests
    }


    /// ACK in reverse request order only after the final UP was independently read.
    pub(super) fn acknowledge_gesture(&mut self, requests: &[Value]) {
        for request in requests.iter().rev() {
            self.answer(request, json!({}));
        }
    }


    /// Require no trailing input or restoration and return actual request bytes.
    pub(super) fn finish(mut self) -> Vec<String> {
        let mut trailing = Vec::new();
        self.reader.read_to_end(&mut trailing).unwrap();
        assert!(trailing.is_empty());
        self.wire
    }
}


/// Require a sole left transition without movement or another click.
pub(super) fn button(request: &Value, down: bool) {
    assert_eq!(request["execute"], "input-send-event");
    assert_eq!(
        request["arguments"],
        json!({"events":[
            {"type":"btn","data":{"button":"left","down":down}}
        ]})
    );
}


/// Construct observed transport before its capability negotiation.
pub(super) fn observed(stream: UnixStream, recorder: Arc<Recorder>) -> QmpClient {
    QmpClient::from_connected_stream_observed(
        Path::new("mock"),
        stream,
        config(),
        Some(recorder),
        QmpWireConnection::Primary,
    )
    .unwrap()
}


/// Exercise the exact diagnostic paths without repeating their pacing in boundary tests.
pub(super) fn run<F: FnMut() -> bool>(
    client: &mut QmpClient,
    relative: bool,
    permitted: F,
) -> Result<(), QmpError> {
    if relative {
        client.diagnostic_relative_drag_with_delay(
            PixelPoint::new(20, 500),
            PixelPoint::new(400, 800),
            (1920, 1080),
            Duration::ZERO,
            permitted,
        )
    } else {
        client.diagnostic_drag_with_delay(
            PixelPoint::new(20, 500),
            PixelPoint::new(400, 800),
            (1920, 1080),
            Duration::ZERO,
            permitted,
        )
    }
}


/// Prove exact wire bytes, distinct IDs, ten bounded motions and real two-second phase gaps.
pub(super) fn real_timing(relative: bool) {
    let (stream, server) = UnixStream::pair().unwrap();
    let recorder = Arc::new(Recorder::new(None));
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 17, 42);
        let original = if relative {
            peer.absolute
        } else {
            peer.relative
        };
        peer.prepare(relative, original);
        let requests = peer.gesture(relative);
        peer.acknowledge_gesture(&requests);
        peer.select(
            if relative {
                peer.relative
            } else {
                peer.absolute
            },
            original,
        );
        peer.finish()
    });
    let mut client = observed(stream, recorder.clone());
    if relative {
        client
            .diagnostic_relative_drag(
                PixelPoint::new(20, 500),
                PixelPoint::new(400, 800),
                1920,
                1080,
                || true,
            )
            .unwrap();
    } else {
        client
            .diagnostic_drag(
                PixelPoint::new(20, 500),
                PixelPoint::new(400, 800),
                1920,
                1080,
                || true,
            )
            .unwrap();
    }
    assert!(client.usable);
    drop(client);
    let actual = peer.join().unwrap();
    let wire = recorder.snapshot();
    assert_eq!(
        wire.iter().map(|p| p.json.clone()).collect::<Vec<_>>(),
        actual
    );
    assert!(
        wire.iter()
            .all(|p| p.connection == QmpWireConnection::Primary && p.flushed)
    );
    assert_eq!(
        wire.iter().map(|p| p.id).collect::<Vec<_>>(),
        (1..=u64::try_from(wire.len()).unwrap()).collect::<Vec<_>>()
    );
    let phases: Vec<_> = wire
        .iter()
        .filter(|p| {
            serde_json::from_str::<Value>(&p.json).unwrap()["execute"] == "input-send-event"
        })
        .collect();
    assert_eq!(phases.len(), 13);
    for pair in phases.windows(2) {
        assert!(pair[1].at.duration_since(pair[0].at) >= Duration::from_secs(2));
    }
}


/// The public REL probe uses two device counts per direction axis with real phase delays.
#[test]
fn qmp_relative_exact_wire_timing_and_restoration_without_extra_click() {
    real_timing(true);
}


/// Confirmed final UP followed by STOP or restore rejection never triggers input replay/recovery.
#[test]
fn qmp_relative_final_up_stop_and_restore_failure_never_replay() {
    for relative in [false, true] {
        for stop_on_up in [false, true] {
            let (stream, server) = UnixStream::pair().unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let recorder = Arc::new(Recorder::new(stop_on_up.then(|| (stop.clone(), 0))));
            let peer = thread::spawn(move || {
                let mut peer = Peer::new(server, 6, 29);
                let original = if relative {
                    peer.absolute
                } else {
                    peer.relative
                };
                peer.prepare(relative, original);
                let requests = peer.gesture(relative);
                peer.acknowledge_gesture(&requests);
                if !stop_on_up {
                    peer.query(if relative {
                        peer.relative
                    } else {
                        peer.absolute
                    });
                    let restore = peer.request();
                    assert_eq!(
                        restore["arguments"],
                        json!({"command-line":format!("mouse_set {original}")})
                    );
                    peer.answer(&restore, json!("Error: selection failed"));
                }
                peer.finish()
            });
            let mut client = observed(stream, recorder.clone());
            let error = run(&mut client, relative, || !stop.load(Ordering::Acquire)).unwrap_err();
            if stop_on_up {
                assert!(
                    matches!(error,QmpError::Protocol(ref s) if s == "diagnostic input authority interrupted")
                );
            } else {
                assert!(matches!(error,QmpError::Command(ref s) if s.contains("selection failed")));
            }
            assert!(client.usable);
            drop(client);
            let actual = peer.join().unwrap();
            let wire = recorder.snapshot();
            assert_eq!(
                wire.iter().map(|p| p.json.clone()).collect::<Vec<_>>(),
                actual
            );
            assert!(
                wire.iter()
                    .all(|p| p.connection == QmpWireConnection::Primary && p.flushed)
            );
        }
    }
}


/// STOP after an intermediate motion or a held ACK error allows only targeted old/fresh safety UP.
#[test]
fn qmp_relative_uncertainty_reselects_only_for_release_and_keeps_cause() {
    for relative in [false, true] {
        for stopped in [false, true] {
            let path = SocketFile(std::env::temp_dir().join(format!(
                "xsar-step-release-{}-{}.sock",
                process::id(),
                NEXT_SOCKET_ID.fetch_add(1, Ordering::Relaxed)
            )));
            let listener = UnixListener::bind(&path.0).unwrap();
            listener.set_nonblocking(true).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let stop_at = if relative { 5 } else { 3 };
            let recorder = Arc::new(Recorder::new(stopped.then(|| (stop.clone(), stop_at))));
            let peer = thread::spawn(move || {
                let mut peer = Peer::new(accept_with_timeout(&listener), 11, 37);
                peer.prepare(relative, peer.absolute);
                if stopped {
                    let down = peer.request();
                    button(&down, true);
                    for _ in 0..if relative { 3 } else { 1 } {
                        let moved = peer.request();
                        assert_eq!(
                            moved["arguments"]["events"][0]["type"],
                            if relative { "rel" } else { "abs" }
                        );
                    }
                } else {
                    let requests = peer.gesture(relative);
                    write_packet(
                        &mut peer.stream,
                        &json!({"error":{
                        "class":"GenericError","desc":"injected held movement failure"
                    },"id":request_id(&requests[if relative { 4 } else { 1 }])}),
                    );
                }
                let selected = if relative {
                    peer.relative
                } else {
                    peer.absolute
                };
                let selector = peer.request();
                assert_eq!(
                    selector["arguments"],
                    json!({"command-line":format!("mouse_set {selected}")})
                );
                let release = peer.request();
                button(&release, false);
                peer.finish();
                let mut peer = Peer::new(accept_with_timeout(&listener), 11, 37);
                peer.greet();
                peer.select(peer.absolute, selected);
                let release = peer.request();
                button(&release, false);
                peer.answer(&release, json!({}));
                peer.finish();
            });
            let mut client =
                QmpClient::connect_with_observer(&path.0, config(), recorder.clone()).unwrap();
            let error = run(&mut client, relative, || !stop.load(Ordering::Acquire)).unwrap_err();
            if stopped {
                assert!(
                    matches!(error,QmpError::Protocol(ref s) if s == "diagnostic input authority interrupted")
                );
            } else {
                assert!(
                    matches!(error,QmpError::Command(ref s) if s == "injected held movement failure")
                );
            }
            assert!(!client.usable);
            drop(client);
            peer.join().unwrap();
            let recovery: Vec<Value> = recorder
                .snapshot()
                .into_iter()
                .filter(|p| p.connection == QmpWireConnection::ReleaseRecovery)
                .map(|p| serde_json::from_str(&p.json).unwrap())
                .collect();
            assert_eq!(recovery.len(), 5);
            assert_eq!(recovery[0]["execute"], "qmp_capabilities");
            assert_eq!(recovery[1]["execute"], "query-mice");
            assert_eq!(recovery[2]["execute"], "human-monitor-command");
            assert_eq!(recovery[3]["execute"], "query-mice");
            button(&recovery[4], false);
        }
    }
}


/// Unique ABS/REL ambiguity and malformed inventories refuse before source movement or DOWN.
#[test]
fn qmp_relative_ambiguous_and_invalid_inventory_refuses_before_input() {
    for relative in [false, true] {
        for malformed in [false, true] {
            let (stream, server) = UnixStream::pair().unwrap();
            let peer = thread::spawn(move || {
                let mut peer = Peer::new(server, 13, 41);
                peer.greet();
                let request = peer.request();
                assert_eq!(request["execute"], "query-mice");
                let mut devices = inventory(13, 41, 13);
                if malformed {
                    devices[1]["index"] = json!(13);
                } else {
                    devices.as_array_mut().unwrap().push(json!({
                        "index":58,"name":"additional pointer","absolute":!relative,"current":false
                    }));
                }
                peer.answer(&request, devices);
                peer.finish()
            });
            let recorder = Arc::new(Recorder::new(None));
            let mut client = observed(stream, recorder);
            assert!(run(&mut client, relative, || true).is_err());
            drop(client);
            let actual = peer.join().unwrap();
            assert_eq!(actual.len(), 2);
        }
    }
}


/// HMP rejection and final handler changes refuse before pressing, with no restore or recovery.
#[test]
fn qmp_relative_rejected_activation_and_changed_routing_send_no_press() {
    for relative in [false, true] {
        for changed in [false, true] {
            let (stream, server) = UnixStream::pair().unwrap();
            let peer = thread::spawn(move || {
                let mut peer = Peer::new(server, 9, 53);
                if changed {
                    peer.position_and_select(relative, peer.absolute);
                    let request = peer.request();
                    assert_eq!(request["execute"], "query-mice");
                    let target = if relative {
                        peer.relative
                    } else {
                        peer.absolute
                    };
                    let mut devices = inventory(peer.absolute, peer.relative, target);
                    devices[if relative { 1 } else { 0 }]["absolute"] = json!(relative);
                    peer.answer(&request, devices);
                } else {
                    peer.greet();
                    peer.query(peer.relative);
                    peer.query(peer.relative);
                    let request = peer.request();
                    assert_eq!(request["arguments"], json!({"command-line":"mouse_set 9"}));
                    peer.answer(&request, json!("Error: activation denied"));
                }
                peer.finish()
            });
            let recorder = Arc::new(Recorder::new(None));
            let mut client = observed(stream, recorder);
            let error = run(&mut client, relative, || true).unwrap_err();
            if changed {
                assert!(
                    matches!(error,QmpError::Protocol(ref s) if s == "diagnostic pointer changed before button down")
                );
            } else {
                assert!(matches!(error, QmpError::Command(_)));
            }
            assert!(client.usable);
            drop(client);
            let actual = peer.join().unwrap();
            assert!(!actual.iter().any(|line| {
                let packet: Value = serde_json::from_str(line).unwrap();
                packet["arguments"]["events"]
                    .as_array()
                    .is_some_and(|events| events.iter().any(|event| event["type"] == "btn"))
            }));
        }
    }
}


/// Every direction uses ten tiny REL increments even when the native delta is much larger.
#[test]
fn qmp_relative_all_direction_axes_are_two_device_counts_per_step() {
    for (destination, dx, dy) in [
        (PixelPoint::new(400, 800), 2, 2),
        (PixelPoint::new(0, 800), -2, 2),
        (PixelPoint::new(400, 0), 2, -2),
        (PixelPoint::new(0, 0), -2, -2),
        (PixelPoint::new(20, 800), 0, 2),
        (PixelPoint::new(400, 500), 2, 0),
    ] {
        let (stream, server) = UnixStream::pair().unwrap();
        let peer = thread::spawn(move || {
            let mut peer = Peer::new(server, 23, 71);
            peer.prepare(true, peer.relative);
            let down = peer.request();
            button(&down, true);
            let mut requests = vec![down];
            for _ in 0..10 {
                let moved = peer.request();
                assert_eq!(
                    moved["arguments"],
                    json!({"events":[
                        {"type":"rel","data":{"axis":"x","value":dx}},
                        {"type":"rel","data":{"axis":"y","value":dy}}
                    ]})
                );
                requests.push(moved);
            }
            let up = peer.request();
            button(&up, false);
            requests.push(up);
            peer.acknowledge_gesture(&requests);
            peer.finish()
        });
        let mut client =
            QmpClient::from_connected_stream_with_config(Path::new("mock"), stream, config())
                .unwrap();
        client
            .diagnostic_relative_drag_with_delay(
                PixelPoint::new(20, 500),
                destination,
                (1920, 1080),
                Duration::ZERO,
                || true,
            )
            .unwrap();
        drop(client);
        peer.join().unwrap();
    }
}
