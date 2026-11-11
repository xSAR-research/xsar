//! Four-point absolute diagnostic wire, timing, preflight and release-only recovery.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize},
};

use super::{
    relative_tests::{Peer, Recorder, Wire, button, config},
    *,
};


/// Native source-up, floored original-endpoint midpoint, destination-up and exact destination.
const HELD_POINTS: [PixelPoint; 4] = [
    PixelPoint::new(20, 455),
    PixelPoint::new(210, 650),
    PixelPoint::new(401, 756),
    PixelPoint::new(401, 801),
];


/// Exercise the exact public path's preparation and delivery without real waits in fault cases.
fn run<F: FnMut() -> bool>(client: &mut QmpClient, permitted: F) -> Result<(), QmpError> {
    client.diagnostic_drag_path_with_delay(
        PixelPoint::new(20, 500),
        &HELD_POINTS,
        (1920, 1080),
        Duration::ZERO,
        permitted,
    )
}


/// Require an independently calculated literal ABS pair without invoking production geometry.
fn absolute_point(request: &Value, (x, y): (u32, u32)) {
    assert_eq!(request["execute"], "input-send-event");
    assert_eq!(
        request["arguments"],
        json!({"events":[
            {"type":"abs","data":{"axis":"x","value":x}},
            {"type":"abs","data":{"axis":"y","value":y}}
        ]})
    );
}


/// Require each native point to be mapped independently, including the odd-sum midpoint.
fn held_point(request: &Value, index: usize) {
    absolute_point(
        request,
        [(341, 13804), (3583, 19720), (6843, 22936), (6843, 24302)][index],
    );
}


/// Require each of the six held packets independently of the transport implementation.
fn held_packet(request: &Value, index: usize) {
    match index {
        0 => button(request, true),
        1..=4 => held_point(request, index - 1),
        5 => button(request, false),
        _ => panic!("unexpected held packet index {index}"),
    }
}


/// Read every held request before sending even the first held ACK.
fn gesture(peer: &mut Peer) -> Vec<Value> {
    (0..6)
        .map(|index| {
            let request = peer.request();
            held_packet(&request, index);
            request
        })
        .collect()
}


/// Retain input attempts on one physical stream without inferring success or acknowledgement.
fn inputs(wire: &[Wire], connection: QmpWireConnection) -> Vec<Value> {
    wire.iter()
        .filter(|request| request.connection == connection)
        .map(|request| serde_json::from_str(&request.json).unwrap())
        .filter(|request: &Value| request["execute"] == "input-send-event")
        .collect()
}


/// Seven separately observed packets retain all four points, six gaps and deferred held ACKs.
#[test]
fn qmp_315_path_wire_and_both_public_timing_profiles() {
    for timing in [
        DiagnosticDragTiming::TwoSeconds,
        DiagnosticDragTiming::HalfSecond,
    ] {
        assert_public_timing(timing);
    }
}


/// Exercise real selected timing while ordinary hold and pointer delays are deliberately longer.
fn assert_public_timing(timing: DiagnosticDragTiming) {
    let expected_delay = match timing {
        DiagnosticDragTiming::TwoSeconds => Duration::from_secs(2),
        DiagnosticDragTiming::HalfSecond => Duration::from_millis(500),
    };
    let (stream, server) = UnixStream::pair().unwrap();
    let recorder = Arc::new(Recorder::new(None));
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 17, 42);
        peer.greet();
        peer.query(peer.relative);
        peer.select(peer.relative, peer.absolute);
        let source = peer.request();
        assert_eq!(
            source["arguments"],
            json!({"events":[
                {"type":"abs","data":{"axis":"x","value":341}},
                {"type":"abs","data":{"axis":"y","value":15169}}
            ]})
        );
        peer.assert_waiting_for_reply();
        let source_acknowledged_at = Instant::now();
        peer.answer(&source, json!({}));
        peer.query(peer.absolute);
        let held = gesture(&mut peer);
        assert_eq!(held.len(), 6);
        peer.acknowledge_gesture(&held);
        peer.select(peer.absolute, peer.relative);
        (peer.finish(), source_acknowledged_at)
    });
    let mut client = QmpClient::from_connected_stream_observed(
        Path::new("mock"),
        stream,
        QmpConfig {
            mouse_hold: Duration::from_secs(30),
            pointer_settle_delay: Duration::from_secs(30),
            ..config()
        },
        Some(recorder.clone()),
        QmpWireConnection::Primary,
    )
    .unwrap();
    client
        .diagnostic_drag_path_with_timing(
            PixelPoint::new(20, 500),
            &HELD_POINTS,
            (1920, 1080),
            timing,
            || true,
        )
        .unwrap();
    assert!(client.usable);
    drop(client);
    let (actual, source_acknowledged_at) = peer.join().unwrap();
    let wire = recorder.snapshot();
    assert_eq!(
        wire.iter()
            .map(|request| request.json.clone())
            .collect::<Vec<_>>(),
        actual
    );
    assert!(
        wire.iter()
            .all(|request| request.flushed && request.connection == QmpWireConnection::Primary)
    );
    assert_eq!(
        wire.iter().map(|request| request.id).collect::<Vec<_>>(),
        (1..=u64::try_from(wire.len()).unwrap()).collect::<Vec<_>>()
    );
    let phases: Vec<_> = wire
        .iter()
        .filter(|request| {
            serde_json::from_str::<Value>(&request.json).unwrap()["execute"] == "input-send-event"
        })
        .collect();
    assert_eq!(phases.len(), 7);
    for (boundary, pair) in phases.windows(2).enumerate() {
        assert!(
            pair[1].at.duration_since(pair[0].at) >= expected_delay,
            "path input gap {} was shorter than {expected_delay:?} for {timing:?}",
            boundary + 1,
        );
    }
    assert!(phases[6].at.duration_since(phases[0].at) >= expected_delay * 6);
    let routing = wire
        .iter()
        .find(|request| {
            request.at > phases[0].at
                && serde_json::from_str::<Value>(&request.json).unwrap()["execute"] == "query-mice"
        })
        .unwrap();
    assert!(routing.at.duration_since(source_acknowledged_at) >= expected_delay);
    let input = inputs(&wire, QmpWireConnection::Primary);
    assert_eq!(input.len(), 7);
    for (index, request) in input.iter().skip(1).enumerate() {
        held_packet(request, index);
    }
}


/// All path coordinates, count and endpoint aliasing refuse before authority and socket input.
#[test]
fn qmp_315_path_preflight_checks_complete_path_before_authority_or_io() {
    let (stream, server) = UnixStream::pair().unwrap();
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 7, 35);
        peer.greet();
        peer.finish()
    });
    let mut client =
        QmpClient::from_connected_stream_with_config(Path::new("mock"), stream, config()).unwrap();
    let mut checks = 0;
    for count in [0, 1, 2, 3, 5, 6] {
        let points = vec![PixelPoint::new(400, 800); count];
        let error = client
            .diagnostic_drag_path_with_timing(
                PixelPoint::new(20, 500),
                &points,
                (1920, 1080),
                DiagnosticDragTiming::HalfSecond,
                || {
                    checks += 1;
                    true
                },
            )
            .unwrap_err();
        assert!(
            matches!(error, QmpError::Protocol(ref text) if text == "diagnostic absolute path requires exactly four held points")
        );
    }
    for invalid in [
        PixelPoint::new(-1, 500),
        PixelPoint::new(1920, 500),
        PixelPoint::new(20, -1),
        PixelPoint::new(20, 1080),
    ] {
        for invalid_index in 0..=4 {
            let mut source = PixelPoint::new(20, 500);
            let mut points = HELD_POINTS;
            if invalid_index == 0 {
                source = invalid;
            } else {
                points[invalid_index - 1] = invalid;
            }
            assert!(
                client
                    .diagnostic_drag_path_with_delay(
                        source,
                        &points,
                        (1920, 1080),
                        Duration::ZERO,
                        || {
                            checks += 1;
                            false
                        },
                    )
                    .is_err()
            );
        }
    }
    for source_y in [0, 44] {
        let error = client
            .diagnostic_drag_path_with_delay(
                PixelPoint::new(20, source_y),
                &[
                    PixelPoint::new(20, source_y - 45),
                    PixelPoint::new(210, (source_y + 801) / 2),
                    PixelPoint::new(401, 756),
                    PixelPoint::new(401, 801),
                ],
                (1920, 1080),
                Duration::ZERO,
                || {
                    checks += 1;
                    true
                },
            )
            .unwrap_err();
        assert!(
            matches!(error, QmpError::Protocol(ref text) if text == "negative drag Y coordinate")
        );
    }
    for frame in [(0, 1080), (1920, 0)] {
        assert!(
            client
                .diagnostic_drag_path_with_delay(
                    PixelPoint::new(20, 500),
                    &HELD_POINTS,
                    frame,
                    Duration::ZERO,
                    || {
                        checks += 1;
                        true
                    },
                )
                .is_err()
        );
    }
    for final_point in [PixelPoint::new(0, 0), PixelPoint::new(1, 0)] {
        let error = client
            .diagnostic_drag_path_with_delay(
                PixelPoint::new(0, 0),
                &[
                    PixelPoint::new(10, 0),
                    PixelPoint::new(20, 0),
                    PixelPoint::new(30, 0),
                    final_point,
                ],
                (65_536, 1),
                Duration::ZERO,
                || {
                    checks += 1;
                    true
                },
            )
            .unwrap_err();
        assert!(
            matches!(error, QmpError::Protocol(ref text) if text == "drag endpoints must differ")
        );
    }
    assert_eq!(checks, 0);
    assert!(run(&mut client, || false).is_err());
    drop(client);
    assert_eq!(peer.join().unwrap().len(), 1);
}


/// Source-up at zero and large caller-supplied coordinates retain their literal wire geometry.
#[test]
fn qmp_316_path_source_up_boundary_and_large_coordinates_map_independently() {
    assert_literal_path(
        PixelPoint::new(20, 45),
        [
            PixelPoint::new(20, 0),
            PixelPoint::new(210, 423),
            PixelPoint::new(401, 756),
            PixelPoint::new(401, 801),
        ],
        (1920, 1080),
        [
            (341, 1365),
            (341, 0),
            (3583, 12833),
            (6843, 22936),
            (6843, 24302),
        ],
    );
    assert_literal_path(
        PixelPoint::new(1_000_000_001, 1_400_000_001),
        [
            PixelPoint::new(1_000_000_001, 1_399_999_956),
            PixelPoint::new(1_500_000_001, 1_700_000_001),
            PixelPoint::new(2_000_000_002, 1_999_999_957),
            PixelPoint::new(2_000_000_002, 2_000_000_002),
        ],
        (u32::MAX, u32::MAX),
        [
            (7629, 10680),
            (7629, 10680),
            (11443, 12969),
            (15258, 15258),
            (15258, 15258),
        ],
    );
}


/// Read source, DOWN, four independently mapped points and UP before acknowledging held input.
fn assert_literal_path(
    source: PixelPoint,
    held_points: [PixelPoint; 4],
    frame: (u32, u32),
    expected: [(u32, u32); 5],
) {
    let (stream, server) = UnixStream::pair().unwrap();
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 23, 67);
        peer.greet();
        peer.query(peer.absolute);
        peer.select(peer.absolute, peer.absolute);
        let source = peer.request();
        absolute_point(&source, expected[0]);
        peer.assert_waiting_for_reply();
        peer.answer(&source, json!({}));
        peer.query(peer.absolute);
        let down = peer.request();
        button(&down, true);
        let mut held = vec![down];
        for point in expected.into_iter().skip(1) {
            let request = peer.request();
            absolute_point(&request, point);
            held.push(request);
        }
        let up = peer.request();
        button(&up, false);
        held.push(up);
        peer.acknowledge_gesture(&held);
        peer.finish()
    });
    let mut client =
        QmpClient::from_connected_stream_with_config(Path::new("mock"), stream, config()).unwrap();
    client
        .diagnostic_drag_path_with_delay(source, &held_points, frame, Duration::ZERO, || true)
        .unwrap();
    drop(client);
    assert_eq!(
        peer.join()
            .unwrap()
            .iter()
            .map(|raw| serde_json::from_str::<Value>(raw).unwrap())
            .filter(|request| request["execute"] == "input-send-event")
            .count(),
        7
    );
}


/// Intermediate aliases remain four distinct requests and do not impose hidden path geometry.
#[test]
fn qmp_315_path_intermediate_mapped_aliases_are_not_deduplicated() {
    let (stream, server) = UnixStream::pair().unwrap();
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 23, 67);
        peer.greet();
        peer.query(peer.absolute);
        peer.select(peer.absolute, peer.absolute);
        let source = peer.request();
        assert_eq!(source["arguments"]["events"][0]["data"]["value"], 0);
        peer.answer(&source, json!({}));
        peer.query(peer.absolute);
        let down = peer.request();
        button(&down, true);
        let mut held = vec![down];
        for x in [0, 0, 0, 1] {
            let point = peer.request();
            assert_eq!(
                point["arguments"],
                json!({"events":[
                    {"type":"abs","data":{"axis":"x","value":x}},
                    {"type":"abs","data":{"axis":"y","value":0}}
                ]})
            );
            held.push(point);
        }
        let up = peer.request();
        button(&up, false);
        held.push(up);
        peer.acknowledge_gesture(&held);
        peer.finish()
    });
    let mut client =
        QmpClient::from_connected_stream_with_config(Path::new("mock"), stream, config()).unwrap();
    client
        .diagnostic_drag_path_with_delay(
            PixelPoint::new(0, 0),
            &[
                PixelPoint::new(0, 0),
                PixelPoint::new(1, 0),
                PixelPoint::new(2, 0),
                PixelPoint::new(3, 0),
            ],
            (65_536, 1),
            Duration::ZERO,
            || true,
        )
        .unwrap();
    drop(client);
    let wire = peer.join().unwrap();
    assert_eq!(
        wire.iter()
            .map(|raw| serde_json::from_str::<Value>(raw).unwrap())
            .filter(|request| request["execute"] == "input-send-event")
            .count(),
        7
    );
}


/// Source rejection never allows DOWN, any held point or pointer restoration.
#[test]
fn qmp_315_path_source_ack_failure_sends_no_held_input() {
    let (stream, server) = UnixStream::pair().unwrap();
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 19, 61);
        peer.greet();
        peer.query(peer.relative);
        peer.select(peer.relative, peer.absolute);
        let source = peer.request();
        peer.assert_waiting_for_reply();
        write_packet(
            &mut peer.stream,
            &json!({"error":{"class":"GenericError","desc":"source refused"},"id":request_id(&source)}),
        );
        peer.finish()
    });
    let recorder = Arc::new(Recorder::new(None));
    let mut client = super::relative_tests::observed(stream, recorder.clone());
    let error = run(&mut client, || true).unwrap_err();
    assert!(matches!(error, QmpError::Command(ref text) if text == "source refused"));
    drop(client);
    let actual = peer.join().unwrap();
    let wire = recorder.snapshot();
    assert_eq!(
        wire.iter()
            .map(|request| request.json.clone())
            .collect::<Vec<_>>(),
        actual
    );
    assert_eq!(inputs(&wire, QmpWireConnection::Primary).len(), 1);
}


/// STOP at the source wait's final check prevents fresh routing and DOWN.
#[test]
fn qmp_315_path_source_wait_stop_sends_only_acknowledged_source() {
    let (stream, server) = UnixStream::pair().unwrap();
    let recorder = Arc::new(Recorder::new(None));
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 19, 61);
        peer.position_and_select(false, peer.relative);
        peer.finish()
    });
    let mut client = super::relative_tests::observed(stream, recorder.clone());
    let mut checks = 0;
    let error = run(&mut client, || {
        if inputs(&recorder.snapshot(), QmpWireConnection::Primary).len() == 1 {
            checks += 1;
            return checks < 2;
        }
        true
    })
    .unwrap_err();
    assert!(
        matches!(error, QmpError::Protocol(ref text) if text == "diagnostic input authority interrupted")
    );
    assert_eq!(checks, 2);
    assert!(client.usable);
    drop(client);
    let actual = peer.join().unwrap();
    let wire = recorder.snapshot();
    assert_eq!(
        wire.iter()
            .map(|request| request.json.clone())
            .collect::<Vec<_>>(),
        actual
    );
    assert_eq!(inputs(&wire, QmpWireConnection::Primary).len(), 1);
    assert!(inputs(&wire, QmpWireConnection::ReleaseRecovery).is_empty());
}


/// A confirmed final UP followed by STOP allows neither recovery nor restoration.
#[test]
fn qmp_315_path_confirmed_up_stop_has_no_restoration_or_recovery() {
    let (stream, server) = UnixStream::pair().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let recorder = Arc::new(Recorder::new(Some((stop.clone(), 0))));
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 31, 59);
        peer.prepare(false, peer.relative);
        let held = gesture(&mut peer);
        peer.acknowledge_gesture(&held);
        peer.finish()
    });
    let mut client = super::relative_tests::observed(stream, recorder.clone());
    let error = run(&mut client, || !stop.load(Ordering::Acquire)).unwrap_err();
    assert!(
        matches!(error, QmpError::Protocol(ref text) if text == "diagnostic input authority interrupted")
    );
    assert!(client.usable);
    drop(client);
    let actual = peer.join().unwrap();
    let wire = recorder.snapshot();
    assert_eq!(
        wire.iter()
            .map(|request| request.json.clone())
            .collect::<Vec<_>>(),
        actual
    );
    assert!(
        wire.iter()
            .all(|request| request.connection == QmpWireConnection::Primary)
    );
    assert_eq!(inputs(&wire, QmpWireConnection::Primary).len(), 7);
}


/// Restoration errors occur after confirmed release and never replay input or recover release.
#[test]
fn qmp_315_path_restoration_failure_preserves_confirmed_release() {
    let (stream, server) = UnixStream::pair().unwrap();
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 29, 53);
        peer.prepare(false, peer.relative);
        let held = gesture(&mut peer);
        peer.acknowledge_gesture(&held);
        peer.query(peer.absolute);
        let restore = peer.request();
        assert_eq!(restore["execute"], "human-monitor-command");
        assert_eq!(restore["arguments"], json!({"command-line":"mouse_set 53"}));
        write_packet(
            &mut peer.stream,
            &json!({"error":{"class":"GenericError","desc":"restore refused"},"id":request_id(&restore)}),
        );
        peer.finish()
    });
    let recorder = Arc::new(Recorder::new(None));
    let mut client = super::relative_tests::observed(stream, recorder.clone());
    let error = run(&mut client, || true).unwrap_err();
    assert!(matches!(error, QmpError::Command(ref text) if text == "restore refused"));
    drop(client);
    let actual = peer.join().unwrap();
    let wire = recorder.snapshot();
    assert_eq!(
        wire.iter()
            .map(|request| request.json.clone())
            .collect::<Vec<_>>(),
        actual
    );
    assert!(
        wire.iter()
            .all(|request| request.connection == QmpWireConnection::Primary)
    );
    assert_eq!(inputs(&wire, QmpWireConnection::Primary).len(), 7);
}


/// Source is ordinal one; DOWN, the four held points and UP are ordinals two through seven.
#[derive(Clone, Copy, Debug)]
enum Failure {
    /// Revoke authority immediately after this successful input flush.
    Stop(usize),
    /// Revoke authority at the post-wait check following this successful input.
    StopAfterWait(usize),
    /// Reject this held request after the peer has independently received UP.
    Command(usize),
    /// Shut down the original stream's write half immediately before this write.
    Write(usize),
}


/// Passive recorder with a deterministic original-stream write fault and original error retention.
struct FaultRecorder {
    /// Exact wire observer, also supporting a deterministic flush STOP.
    inner: Recorder,
    /// Primary input ordinal whose write fails, or zero when no fault is injected.
    fail_at: usize,
    /// Primary input attempts counted independently of write completion.
    attempted: AtomicUsize,
    /// Original stream clone consumed only when injecting the write fault.
    stream: Mutex<Option<UnixStream>>,
    /// Original write errors retained separately from later safety-release errors.
    errors: Mutex<Vec<String>>,
}


impl QmpWireObserver for FaultRecorder {
    /// Inject a local write shutdown without supplying any guest input or recovery routing.
    fn observe(&self, event: QmpWireEvent<'_>) {
        match &event {
            QmpWireEvent::Attempted {
                connection: QmpWireConnection::Primary,
                json,
                ..
            } => {
                let request: Value = serde_json::from_str(json).unwrap();
                if request["execute"] == "input-send-event"
                    && self.attempted.fetch_add(1, Ordering::Relaxed) + 1 == self.fail_at
                {
                    self.stream
                        .lock()
                        .unwrap()
                        .take()
                        .unwrap()
                        .shutdown(Shutdown::Write)
                        .unwrap();
                }
            }
            QmpWireEvent::Failed {
                connection: QmpWireConnection::Primary,
                error,
                ..
            } => self.errors.lock().unwrap().push(error.to_string()),
            _ => {}
        }
        self.inner.observe(event);
    }
}


/// A held failure preserves its first cause and permits only captured-target old/fresh safety UP.
fn assert_recovery(failure: Failure) {
    let path = SocketFile(std::env::temp_dir().join(format!(
        "xsar-path-absolute-{}-{}.sock",
        process::id(),
        NEXT_SOCKET_ID.fetch_add(1, Ordering::Relaxed)
    )));
    let listener = UnixListener::bind(&path.0).unwrap();
    listener.set_nonblocking(true).unwrap();
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(accept_with_timeout(&listener), 11, 37);
        peer.prepare(false, peer.relative);
        let received = match failure {
            Failure::Stop(ordinal) | Failure::StopAfterWait(ordinal) => ordinal - 1,
            Failure::Command(_) => 6,
            Failure::Write(ordinal) => ordinal - 2,
        };
        let mut held = Vec::new();
        for index in 0..received {
            let request = peer.request();
            held_packet(&request, index);
            held.push(request);
        }
        if let Failure::Command(ordinal) = failure {
            write_packet(
                &mut peer.stream,
                &json!({
                    "error":{"class":"GenericError","desc":"injected path phase rejection"},
                    "id":request_id(&held[ordinal - 2])
                }),
            );
        }
        if !matches!(failure, Failure::Write(_)) {
            let select = peer.request();
            assert_eq!(select["execute"], "human-monitor-command");
            assert_eq!(select["arguments"], json!({"command-line":"mouse_set 11"}));
            button(&peer.request(), false);
        }
        let mut wire = peer.finish();
        let mut fresh = Peer::new(accept_with_timeout(&listener), 11, 37);
        fresh.greet();
        fresh.select(fresh.absolute, fresh.absolute);
        let up = fresh.request();
        button(&up, false);
        fresh.answer(&up, json!({}));
        wire.extend(fresh.finish());
        wire
    });
    let stop = Arc::new(AtomicBool::new(false));
    let stream = UnixStream::connect(&path.0).unwrap();
    let recorder = Arc::new(FaultRecorder {
        inner: Recorder::new(match failure {
            Failure::Stop(ordinal) => Some((stop.clone(), ordinal)),
            _ => None,
        }),
        fail_at: match failure {
            Failure::Write(ordinal) => ordinal,
            _ => 0,
        },
        attempted: AtomicUsize::new(0),
        stream: Mutex::new(
            matches!(failure, Failure::Write(_)).then(|| stream.try_clone().unwrap()),
        ),
        errors: Mutex::new(Vec::new()),
    });
    let mut client = QmpClient::from_connected_stream_observed(
        &path.0,
        stream,
        config(),
        Some(recorder.clone()),
        QmpWireConnection::Primary,
    )
    .unwrap();
    let mut wait_checks = 0;
    let error = run(&mut client, || {
        if let Failure::StopAfterWait(ordinal) = failure
            && inputs(&recorder.inner.snapshot(), QmpWireConnection::Primary).len() == ordinal
        {
            wait_checks += 1;
            if wait_checks == 2 {
                stop.store(true, Ordering::Release);
            }
        }
        !stop.load(Ordering::Acquire)
    })
    .unwrap_err();
    match failure {
        Failure::Stop(_) | Failure::StopAfterWait(_) => assert!(
            matches!(error, QmpError::Protocol(ref text) if text == "diagnostic input authority interrupted")
        ),
        Failure::Command(_) => assert!(
            matches!(error, QmpError::Command(ref text) if text == "injected path phase rejection")
        ),
        Failure::Write(_) => {
            assert!(matches!(error, QmpError::Json(_)));
            assert_eq!(
                Some(&error.to_string()),
                recorder.errors.lock().unwrap().first()
            );
        }
    }
    assert!(!client.usable);
    drop(client);
    let actual = peer.join().unwrap();
    let wire = recorder.inner.snapshot();
    assert_eq!(
        wire.iter()
            .filter(|request| request.flushed)
            .map(|request| request.json.clone())
            .collect::<Vec<_>>(),
        actual
    );
    let primary = inputs(&wire, QmpWireConnection::Primary);
    assert_eq!(
        primary
            .iter()
            .filter(|request| request["arguments"]["events"][0]["data"]["down"] == true)
            .count(),
        1
    );
    let recovery = inputs(&wire, QmpWireConnection::ReleaseRecovery);
    assert_eq!(recovery.len(), 1);
    button(&recovery[0], false);
}


/// STOP after DOWN or any of four points permits only routed safety release.
#[test]
fn qmp_315_path_stop_at_every_held_boundary_uses_only_targeted_release() {
    for ordinal in 2..=6 {
        assert_recovery(Failure::Stop(ordinal));
    }
}


/// STOP caught after each held wait prevents the next point or final UP from normal delivery.
#[test]
fn qmp_315_path_stop_after_every_held_wait_uses_only_targeted_release() {
    for ordinal in 2..=6 {
        assert_recovery(Failure::StopAfterWait(ordinal));
    }
}


/// Each of six held command errors remains causal even with the other ACKs withheld.
#[test]
fn qmp_315_path_command_failure_at_every_held_phase_keeps_cause() {
    for ordinal in 2..=7 {
        assert_recovery(Failure::Command(ordinal));
    }
}


/// Each of six held write failures preserves uncertainty without replaying any press or motion.
#[test]
fn qmp_315_path_write_failure_at_every_held_phase_keeps_cause() {
    for ordinal in 2..=7 {
        assert_recovery(Failure::Write(ordinal));
    }
}
