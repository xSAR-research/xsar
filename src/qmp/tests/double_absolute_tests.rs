//! Paced five-packet upward-approach ABS diagnostics and targeted release recovery.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize},
};

use super::{
    relative_tests::{Peer, Recorder, Wire, button, config},
    *,
};


/// Exercise the exact ABS experiment without repeating public pacing in fault tests.
fn run<F: FnMut() -> bool>(client: &mut QmpClient, permitted: F) -> Result<(), QmpError> {
    client.diagnostic_drag_with_delay(
        PixelPoint::new(20, 500),
        PixelPoint::new(400, 800),
        (1920, 1080),
        Duration::ZERO,
        permitted,
    )
}


/// Require the independently computed final destination with both absolute axes.
fn destination(request: &Value) {
    assert_eq!(request["execute"], "input-send-event");
    assert_eq!(
        request["arguments"],
        json!({"events":[
            {"type":"abs","data":{"axis":"x","value":6826}},
            {"type":"abs","data":{"axis":"y","value":24271}}
        ]})
    );
}


/// Require the native approach (400, 796), four pixels above the exact destination.
fn approach(request: &Value) {
    assert_eq!(request["execute"], "input-send-event");
    assert_eq!(
        request["arguments"],
        json!({"events":[
            {"type":"abs","data":{"axis":"x","value":6826}},
            {"type":"abs","data":{"axis":"y","value":24150}}
        ]})
    );
}


/// Retain only input attempts from one physical stream without inferring acknowledgement.
fn inputs(wire: &[Wire], connection: QmpWireConnection) -> Vec<Value> {
    wire.iter()
        .filter(|request| request.connection == connection)
        .map(|request| serde_json::from_str(&request.json).unwrap())
        .filter(|request: &Value| request["execute"] == "input-send-event")
        .collect()
}


/// Source ACK gates four named-profile intervals; held ACKs cannot delay the final UP.
#[test]
fn qmp_313_absolute_approach_wire_and_both_named_timing_profiles() {
    for timing in [
        DiagnosticDragTiming::TwoSeconds,
        DiagnosticDragTiming::HalfSecond,
    ] {
        assert_public_timing(timing);
    }
}


/// Observe real pacing independently of ordinary mouse configuration for one public entry point.
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
        let requests = peer.gesture(false);
        assert_eq!(requests.len(), 4);
        approach(&requests[1]);
        destination(&requests[2]);
        assert_ne!(request_id(&requests[1]), request_id(&requests[2]));
        peer.acknowledge_gesture(&requests);
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
    match timing {
        DiagnosticDragTiming::TwoSeconds => client.diagnostic_drag(
            PixelPoint::new(20, 500),
            PixelPoint::new(400, 800),
            1920,
            1080,
            || true,
        ),
        DiagnosticDragTiming::HalfSecond => client.diagnostic_drag_with_timing(
            PixelPoint::new(20, 500),
            PixelPoint::new(400, 800),
            (1920, 1080),
            timing,
            || true,
        ),
    }
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
    let input = inputs(&wire, QmpWireConnection::Primary);
    assert_eq!(input.len(), 5);
    button(&input[1], true);
    approach(&input[2]);
    destination(&input[3]);
    assert_ne!(input[2]["arguments"], input[3]["arguments"]);
    button(&input[4], false);
    let phases: Vec<_> = wire
        .iter()
        .filter(|request| {
            serde_json::from_str::<Value>(&request.json).unwrap()["execute"] == "input-send-event"
        })
        .collect();
    assert_eq!(phases.len(), 5);
    for (boundary, pair) in phases.windows(2).enumerate() {
        assert!(
            pair[1].at.duration_since(pair[0].at) >= expected_delay,
            "diagnostic input gap {} was shorter than {expected_delay:?} for {timing:?}",
            boundary + 1,
        );
    }
    assert!(phases[4].at.duration_since(phases[0].at) >= expected_delay * 4);
    let fresh_routing = wire
        .iter()
        .find(|request| {
            request.at > phases[0].at
                && serde_json::from_str::<Value>(&request.json).unwrap()["execute"] == "query-mice"
        })
        .unwrap();
    assert!(fresh_routing.at.duration_since(source_acknowledged_at) >= expected_delay);
}


/// Revocation at the source wait's final authority check prevents fresh routing and DOWN.
#[test]
fn qmp_3111_absolute_source_wait_stop_sends_only_acknowledged_source() {
    let (stream, server) = UnixStream::pair().unwrap();
    let recorder = Arc::new(Recorder::new(None));
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 19, 61);
        peer.position_and_select(false, peer.relative);
        peer.finish()
    });
    let mut client = super::relative_tests::observed(stream, recorder.clone());
    let mut source_checks = 0;
    let error = run(&mut client, || {
        if inputs(&recorder.snapshot(), QmpWireConnection::Primary).len() == 1 {
            source_checks += 1;
            return source_checks < 2;
        }
        true
    })
    .unwrap_err();
    assert!(
        matches!(error, QmpError::Protocol(ref text) if text == "diagnostic input authority interrupted")
    );
    assert_eq!(source_checks, 2);
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
    assert_eq!(actual.len(), 6);
    assert_eq!(inputs(&wire, QmpWireConnection::Primary).len(), 1);
    assert!(inputs(&wire, QmpWireConnection::ReleaseRecovery).is_empty());
}


/// Native approach literals cover all directions, y=4, aliases and large native bounds.
#[test]
fn qmp_313_absolute_native_approaches_cover_quadrants_aliases_and_bounds() {
    for (source, target, above, frame) in [
        (
            PixelPoint::new(0, 0),
            PixelPoint::new(1919, 1079),
            PixelPoint::new(1919, 1075),
            (1920, 1080),
        ),
        (
            PixelPoint::new(1919, 1079),
            PixelPoint::new(0, 4),
            PixelPoint::new(0, 0),
            (1920, 1080),
        ),
        (
            PixelPoint::new(0, 1079),
            PixelPoint::new(1919, 4),
            PixelPoint::new(1919, 0),
            (1920, 1080),
        ),
        (
            PixelPoint::new(1919, 0),
            PixelPoint::new(0, 1079),
            PixelPoint::new(0, 1075),
            (1920, 1080),
        ),
        (
            PixelPoint::new(400, 796),
            PixelPoint::new(400, 800),
            PixelPoint::new(400, 796),
            (1920, 1080),
        ),
        (
            PixelPoint::new(100, 100),
            PixelPoint::new(101, 100),
            PixelPoint::new(101, 96),
            (1920, 1080),
        ),
        (
            PixelPoint::new(100, 100),
            PixelPoint::new(99, 101),
            PixelPoint::new(99, 97),
            (1920, 1080),
        ),
        (
            PixelPoint::new(100, 100),
            PixelPoint::new(100, 99),
            PixelPoint::new(100, 95),
            (1920, 1080),
        ),
        (
            PixelPoint::new(0, 0),
            PixelPoint::new(3, 4),
            PixelPoint::new(3, 0),
            (65_536, 5),
        ),
        (
            PixelPoint::new(3, 4),
            PixelPoint::new(0, 4),
            PixelPoint::new(0, 0),
            (65_536, 5),
        ),
        (
            PixelPoint::new(20, 500),
            PixelPoint::new(400, 14),
            PixelPoint::new(400, 10),
            (1920, 262_144),
        ),
        (
            PixelPoint::new(i32::MAX - 200_000, i32::MAX - 100_000),
            PixelPoint::new(i32::MAX, i32::MAX),
            PixelPoint::new(2_147_483_647, 2_147_483_643),
            (2_147_483_648, 2_147_483_648),
        ),
        (
            PixelPoint::new(i32::MAX, i32::MAX),
            PixelPoint::new(0, 4),
            PixelPoint::new(0, 0),
            (u32::MAX, u32::MAX),
        ),
    ] {
        let (stream, server) = UnixStream::pair().unwrap();
        let peer = thread::spawn(move || {
            let mut peer = Peer::new(server, 23, 47);
            peer.greet();
            peer.query(peer.absolute);
            peer.select(peer.absolute, peer.absolute);
            let positioned = peer.request();
            assert_eq!(positioned["arguments"], expected_absolute(source, frame));
            peer.answer(&positioned, json!({}));
            peer.query(peer.absolute);
            let down = peer.request();
            button(&down, true);
            let approached = peer.request();
            let final_motion = peer.request();
            assert_eq!(approached["execute"], "input-send-event");
            assert_eq!(approached["arguments"], expected_absolute(above, frame));
            assert_eq!(final_motion["execute"], "input-send-event");
            assert_eq!(final_motion["arguments"], expected_absolute(target, frame));
            assert_ne!(request_id(&approached), request_id(&final_motion));
            let up = peer.request();
            button(&up, false);
            peer.acknowledge_gesture(&[down, approached, final_motion, up]);
            peer.finish()
        });
        let mut client =
            QmpClient::from_connected_stream_with_config(Path::new("mock"), stream, config())
                .unwrap();
        client
            .diagnostic_drag_with_delay(source, target, frame, Duration::ZERO, || true)
            .unwrap();
        drop(client);
        let actual = peer.join().unwrap();
        assert_eq!(actual.len(), 11);
    }
}


/// Independently map a supplied native literal using widened integer products.
fn expected_absolute(point: PixelPoint, frame: (u32, u32)) -> Value {
    json!({"events":[
        {"type":"abs","data":{"axis":"x","value":u64::try_from(point.x).unwrap() * 32767 / u64::from(frame.0)}},
        {"type":"abs","data":{"axis":"y","value":u64::try_from(point.y).unwrap() * 32767 / u64::from(frame.1)}}
    ]})
}


/// Invalid native inputs and mapped-alias endpoints refuse before authority callbacks or I/O.
#[test]
fn qmp_313_absolute_approach_preflight_refuses_before_inventory_and_authority() {
    let (stream, server) = UnixStream::pair().unwrap();
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 23, 47);
        peer.greet();
        peer.finish()
    });
    let mut client =
        QmpClient::from_connected_stream_with_config(Path::new("mock"), stream, config()).unwrap();
    let valid_source = PixelPoint::new(100, 100);
    let valid_target = PixelPoint::new(200, 200);
    let mut checks = 0;
    for (source, target, frame) in [
        (PixelPoint::new(-1, 100), valid_target, (1920, 1080)),
        (PixelPoint::new(100, -1), valid_target, (1920, 1080)),
        (PixelPoint::new(1920, 100), valid_target, (1920, 1080)),
        (PixelPoint::new(100, 1080), valid_target, (1920, 1080)),
        (valid_source, PixelPoint::new(-1, 100), (1920, 1080)),
        (valid_source, PixelPoint::new(100, -1), (1920, 1080)),
        (valid_source, PixelPoint::new(100, i32::MIN), (1920, 1080)),
        (valid_source, PixelPoint::new(100, 0), (1920, 1080)),
        (valid_source, PixelPoint::new(100, 1), (1920, 1080)),
        (valid_source, PixelPoint::new(100, 2), (1920, 1080)),
        (valid_source, PixelPoint::new(100, 3), (1920, 1080)),
        (valid_source, PixelPoint::new(1920, 100), (1920, 1080)),
        (valid_source, PixelPoint::new(100, 1080), (1920, 1080)),
        (
            valid_source,
            PixelPoint::new(i32::MAX, i32::MAX),
            (1920, 1080),
        ),
        (valid_source, valid_target, (0, 1080)),
        (valid_source, valid_target, (1920, 0)),
        (valid_source, valid_source, (1920, 1080)),
        (
            PixelPoint::new(0, 100),
            PixelPoint::new(1, 100),
            (65_536, 1080),
        ),
    ] {
        for timing in [
            DiagnosticDragTiming::TwoSeconds,
            DiagnosticDragTiming::HalfSecond,
        ] {
            let error = client
                .diagnostic_drag_with_timing(source, target, frame, timing, || {
                    checks += 1;
                    true
                })
                .unwrap_err();
            if (0..4).contains(&target.y) {
                assert!(
                    matches!(error, QmpError::Protocol(ref text) if text == "diagnostic approach requires destination y >= 4")
                );
            }
        }
    }
    assert_eq!(checks, 0);
    assert!(client.usable);
    drop(client);
    assert_eq!(peer.join().unwrap().len(), 1);
}


/// Confirmed final UP followed by STOP does not restore routing or open a recovery stream.
#[test]
fn qmp_3110_absolute_final_up_stop_has_no_restoration_or_recovery() {
    let (stream, server) = UnixStream::pair().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let recorder = Arc::new(Recorder::new(Some((stop.clone(), 0))));
    let peer = thread::spawn(move || {
        let mut peer = Peer::new(server, 31, 59);
        peer.prepare(false, peer.relative);
        let requests = peer.gesture(false);
        peer.acknowledge_gesture(&requests);
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
    assert_eq!(inputs(&wire, QmpWireConnection::Primary).len(), 5);
}


/// Failure input ordinal: source is one, then DOWN/APPROACH/DESTINATION/UP are two to five.
#[derive(Clone, Copy, Debug)]
enum Failure {
    /// Revoke authority immediately after a successful flush.
    Stop(usize),
    /// Revoke authority on the post-wait check after the chosen successful input.
    StopAfterWait(usize),
    /// Reject the chosen held request only after the peer has independently received UP.
    Command(usize),
    /// Shut down only the original stream's write half immediately before this write.
    Write(usize),
}


/// Passive wire recorder plus a deterministic test-only original-stream write failure.
struct FaultRecorder {
    /// Existing exact recorder also supplies deterministic flush-boundary STOP.
    inner: Recorder,
    /// Chosen primary input ordinal whose write must fail, or zero for no fault.
    fail_at: usize,
    /// Number of primary input attempts, independently of their write outcome.
    attempted: AtomicUsize,
    /// Original stream clone consumed at the injected write boundary.
    stream: Mutex<Option<UnixStream>>,
    /// Original write errors in observation order, retained separately from recovery failures.
    errors: Mutex<Vec<String>>,
}


impl QmpWireObserver for FaultRecorder {
    /// Inject no input; an original-stream shutdown cannot alter fresh recovery routing.
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
            } => {
                self.errors.lock().unwrap().push(error.to_string());
            }
            _ => {}
        }
        self.inner.observe(event);
    }
}


/// One STOP/error permits only captured-ABS old/fresh UP and retains the first cause.
fn assert_recovery(failure: Failure) {
    let path = SocketFile(std::env::temp_dir().join(format!(
        "xsar-double-absolute-{}-{}.sock",
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
            Failure::Command(_) => 4,
            Failure::Write(ordinal) => ordinal - 2,
        };
        let mut held = Vec::new();
        for index in 0..received {
            let request = peer.request();
            match index {
                0 => button(&request, true),
                1 => approach(&request),
                2 => destination(&request),
                3 => button(&request, false),
                _ => unreachable!(),
            }
            held.push(request);
        }
        if let Failure::Command(ordinal) = failure {
            write_packet(
                &mut peer.stream,
                &json!({
                    "error":{"class":"GenericError","desc":"injected ABS phase rejection"},
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
            matches!(error, QmpError::Command(ref text) if text == "injected ABS phase rejection")
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


/// STOP after DOWN or either held motion never permits further motion or restoration.
#[test]
fn qmp_3110_absolute_stop_at_every_held_boundary_uses_only_targeted_release() {
    for ordinal in 2..=4 {
        assert_recovery(Failure::Stop(ordinal));
    }
}


/// STOP detected after each held wait permits only captured-target release recovery.
#[test]
fn qmp_3111_absolute_stop_after_every_held_wait_uses_only_targeted_release() {
    for ordinal in 2..=4 {
        assert_recovery(Failure::StopAfterWait(ordinal));
    }
}


/// Any held-phase command error is causal even when later ACKs are intentionally withheld.
#[test]
fn qmp_3110_absolute_command_failure_at_every_held_phase_keeps_cause() {
    for ordinal in 2..=5 {
        assert_recovery(Failure::Command(ordinal));
    }
}


/// Partial-delivery uncertainty at each held write never replays DOWN or destination motion.
#[test]
fn qmp_3110_absolute_write_failure_at_every_held_phase_keeps_cause() {
    for ordinal in 2..=5 {
        assert_recovery(Failure::Write(ordinal));
    }
}
