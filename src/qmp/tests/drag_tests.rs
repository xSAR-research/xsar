//! Neutral held-drag phase, acknowledgement and release-only recovery regressions.

use super::*;


/// Remove gesture delays while bounding mock transport response collection.
fn config() -> QmpConfig {
    QmpConfig {
        io_timeout: Duration::from_secs(2),
        pointer_settle_delay: Duration::ZERO,
        mouse_hold: Duration::ZERO,
        ..QmpConfig::default()
    }
}


/// Require the complete checked absolute-axis movement batch.
fn movement(request: &Value, x: u32, y: u32) {
    assert_eq!(request["execute"], "input-send-event");
    assert_eq!(
        request["arguments"],
        json!({"events":[
        {"type":"abs","data":{"axis":"x","value":x}},
        {"type":"abs","data":{"axis":"y","value":y}}]})
    );
}


/// Require one left-button transition on its own request ID.
fn button(request: &Value, down: bool) {
    let event = first_input_event(request);
    assert_eq!(
        event,
        &json!({"type":"btn","data":{"button":"left","down":down}})
    );
}


/// Distinct pixels mapping to the same QMP point cannot silently become a click.
#[test]
fn drag_312_mapped_alias_refuses_before_input() {
    let (client, mut server) = UnixStream::pair().unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let peer = thread::spawn(move || {
        let mut reader = BufReader::new(server.try_clone().unwrap());
        start_mock_qmp(&mut server, &mut reader);
        let request = read_request(&mut reader);
        movement(&request, 341, 15_169);
        acknowledge(&mut server, &request);
    });
    let mut client =
        QmpClient::from_connected_stream_with_config(Path::new("mock"), client, config()).unwrap();
    let mut calls = 0;
    let error = client
        .drag(
            PixelPoint::new(0, 0),
            PixelPoint::new(1, 0),
            65_536,
            1,
            || {
                calls += 1;
                true
            },
        )
        .unwrap_err();
    assert!(matches!(error,QmpError::Protocol(ref s) if s=="drag endpoints must differ"));
    assert_eq!(calls, 0);
    client
        .move_pointer(PixelPoint::new(20, 500), 1920, 1080)
        .unwrap();
    peer.join().unwrap();
}


/// Source ACK precedes down; all gesture ACKs may wait until UP is transmitted.
#[test]
fn drag_312_release_precedes_gesture_ack_wait() {
    let (client, mut server) = UnixStream::pair().unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let peer = thread::spawn(move || {
        let mut reader = BufReader::new(server.try_clone().unwrap());
        start_mock_qmp(&mut server, &mut reader);
        let source = read_request(&mut reader);
        movement(&source, 341, 15_169);
        reader
            .get_ref()
            .set_read_timeout(Some(Duration::from_millis(30)))
            .unwrap();
        let mut premature = String::new();
        let error = reader.read_line(&mut premature).unwrap_err();
        assert!(matches!(
            error.kind(),
            ErrorKind::WouldBlock | ErrorKind::TimedOut
        ));
        assert!(premature.is_empty());
        reader
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        acknowledge(&mut server, &source);
        let down = read_request(&mut reader);
        button(&down, true);
        let destination = read_request(&mut reader);
        movement(&destination, 6_826, 24_271);
        let up = read_request(&mut reader);
        button(&up, false);
        write_packet(&mut server, &json!({"event":"RESET"}));
        for request in [&up, &destination, &down] {
            acknowledge(&mut server, request);
        }
    });
    let mut client =
        QmpClient::from_connected_stream_with_config(Path::new("mock"), client, config()).unwrap();
    client
        .drag(
            PixelPoint::new(20, 500),
            PixelPoint::new(400, 800),
            1920,
            1080,
            || true,
        )
        .unwrap();
    peer.join().unwrap();
}


/// Observe cancellation after down or a held-movement error, with no press replay.
fn recovery(cancel_after_down: bool) {
    let path = SocketFile(std::env::temp_dir().join(format!(
        "xsar-drag-{}-{}.sock",
        process::id(),
        NEXT_SOCKET_ID.fetch_add(1, Ordering::Relaxed)
    )));
    let listener = UnixListener::bind(&path.0).unwrap();
    listener.set_nonblocking(true).unwrap();
    let peer = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener);
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        start_mock_qmp(&mut stream, &mut reader);
        let source = read_request(&mut reader);
        movement(&source, 341, 15_169);
        acknowledge(&mut stream, &source);
        let down = read_request(&mut reader);
        button(&down, true);
        if !cancel_after_down {
            let destination = read_request(&mut reader);
            movement(&destination, 6_826, 24_271);
            let up = read_request(&mut reader);
            button(&up, false);
            write_packet(
                &mut stream,
                &json!({"error":{"class":"GenericError","desc":"held movement failed"},"id":request_id(&destination)}),
            );
        }
        let release = read_request(&mut reader);
        button(&release, false);
        let mut trailing = Vec::new();
        reader.read_to_end(&mut trailing).unwrap();
        assert!(trailing.is_empty());
        drop(reader);
        drop(stream);
        let mut fresh = accept_with_timeout(&listener);
        fresh
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(fresh.try_clone().unwrap());
        start_mock_qmp(&mut fresh, &mut reader);
        let release = read_request(&mut reader);
        button(&release, false);
        acknowledge(&mut fresh, &release);
        let mut trailing = Vec::new();
        reader.read_to_end(&mut trailing).unwrap();
        assert!(trailing.is_empty());
    });
    let mut client = QmpClient::connect_with_config(&path.0, config()).unwrap();
    let mut calls = 0;
    let error = client
        .drag(
            PixelPoint::new(20, 500),
            PixelPoint::new(400, 800),
            1920,
            1080,
            || {
                calls += 1;
                !cancel_after_down || calls < 3
            },
        )
        .unwrap_err();
    if cancel_after_down {
        assert!(matches!(error,QmpError::Protocol(ref s) if s=="drag authority interrupted"));
    } else {
        assert!(matches!(error,QmpError::Command(ref s) if s=="held movement failed"));
    }
    assert!(
        matches!(client.move_pointer(PixelPoint::new(20,500),1920,1080),Err(QmpError::Protocol(ref s)) if s=="the QMP connection is no longer usable")
    );
    peer.join().unwrap();
}


/// Revocation after down releases at the source without moving to the target.
#[test]
fn drag_312_cancel_after_down_releases_only() {
    recovery(true);
}


/// A held-movement refusal preserves its cause and permits release recovery only.
#[test]
fn drag_312_failed_movement_never_replays() {
    recovery(false);
}
