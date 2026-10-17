# TCP capture protocol and client

Local candidate 0.4.2-candidate.2 pairs with screen-grab servers 0.1.2/0.1.3 and
qmp-qemu-socket 2.2.2-candidate.14. `tcp_capture` enables only `pixel_format`;
the transport and codecs use the standard library. The default feature set
remains dependency-free. This candidate has not been registry-published.

## Modes and returned frames

`ScreenCaptureClient::new` now selects `PersistentBgrV3`. Use `with_mode` to
select `LegacyV2`, `BgrV3` or `PersistentBgrV3` explicitly. There is no negotiation,
automatic downgrade or failed-request replay. Both one-frame modes close their
connection after each successful frame. Persistent mode retains only a fully
validated session and permits one outstanding request.

Endpoints are numeric `SocketAddr` strings, such as `127.0.0.1:9999` or
`[::1]:9999`. Hostnames previously accepted through DNS are now refused so that
name resolution cannot escape the connection deadline. Callers must select a
numeric endpoint before requesting capture.

`capture_full` returns the existing owned `TcpCapturedFrame` and expanded
`TcpCaptureTiming`. `capture_full_cancellable` additionally accepts a borrowed
`AtomicBool`. Frames contain complete top-down packed RGBA8, with alpha 255.
The timing metadata retains protocol, session, request, frame and display
generation identifiers, exact byte counts and connection reuse. Adding timing
fields and a cancellation error variant may require updates to downstream
struct literals or exhaustive error matches.

## Version 3 wire layout

All integers are unsigned little-endian values. Serialisation copies explicit
fields; Rust structure layout and padding never enter the wire format.

The request is exactly 32 bytes:

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `XSCP` |
| 4 | 1 | Version 3 |
| 5 | 1 | Full-display command 1 |
| 6 | 2 | Reserved, zero |
| 8 | 8 | Session; zero on the initial request |
| 16 | 8 | Nonzero increasing request identity |
| 24 | 8 | Expected display generation; zero initially |

The response header is exactly 80 bytes:

| Offset | Bytes | Meaning on success |
| --- | --- | --- |
| 0 | 4 | ASCII `XSCP` |
| 4 | 1 | Version 3 |
| 5 | 1 | Status 0 |
| 6 | 1 | BGR8 format 1 |
| 7 | 1 | Top-down orientation 1 |
| 8 | 8 | Nonzero server session |
| 16 | 8 | Echoed request identity |
| 24 | 8 | Nonzero increasing capture identity |
| 32 | 8 | Nonzero display generation |
| 40, 44 | 4 each | Complete display width, height |
| 48, 52 | 4 each | Returned origin, both zero |
| 56, 60 | 4 each | Returned width, height, equal to display |
| 64 | 4 | Packed BGR stride, `width * 3` |
| 68 | 4 | Decoded BGR byte length, `stride * height` |
| 72 | 4 | Transmitted byte length, equal to decoded length |
| 76 | 1 | Raw compression 0 |
| 77 | 1 | Echoed full-display command 1 |
| 78 | 2 | Reserved, zero |

Only the exact successful payload follows. The canonical RGBA allocation must
fit 8 MiB; dimensions, arithmetic, full coverage, stride and lengths are checked
before allocation. At 1920x1080 the wire payload is 6,220,800 bytes; the host
RGBA frame is 8,294,400 bytes with stride 7680. Odd widths have no BGR row padding.
No regional image, synthetic surround, compression, scaling or colour tolerance
is supported by this command.

Failure statuses are Busy=1, InvalidRequest=2, CaptureFailed=3,
DeadlineExceeded=4, IncompatibleDisplay=5 and ShuttingDown=6. They carry no
pixels: format/orientation and bytes 24..75 must be zero. Session/request
correlation, version, command and reserved bytes are still checked. Unknown
statuses and contradictory error metadata are refused.

Version 2 remains explicit: request bytes `42 02 01 00 00`, then a 14-byte
response containing width `u32`, height `u32`, packed BGRA stride `u16` and
frame number `u32`. Its pixel allocation remains bounded to 8 MiB. This legacy
format has no session/request/display-generation correlation.

## Bounds, cancellation and failures

Connect makes one attempt with a five-second bound and checks cancellation
before and after. Established request-write, header-read and payload-read stages
each have an absolute ten-second deadline, with approximately 100 ms socket
polling for cancellation. Partial progress does not renew a deadline. Conversion
also checks cancellation periodically. These are separate stage bounds, not a
ten-second limit for the entire API call or a promise of immediate cancellation.

Every error discards the socket and all session evidence. A later independent
call can reconnect; the failed call never retries or returns partial pixels.
Before sending a new request, a retained socket is checked for already queued
data or clean EOF. Clean EOF discards the old connection and identities before
the existing single bounded connection attempt. The new request starts with
session/generation zero and request identity one; timing reports a new connection.
This is pre-request connection selection, not replay of a failed request.
Unsolicited bytes and peek errors fail the call. A peer can still close after a
successful preflight; write/read failures are never retried within that call.
Response identities reject stale/out-of-order frames and silent session,
dimension or generation changes. Checks for unsolicited data concern bytes
currently queued on the socket; they do not predict future peer behaviour.
This is a trusted-network protocol, without authentication or encryption.

Timings use adjacent local monotonic boundaries for connect, request write,
header wait, wire-buffer allocation, payload read, RGBA destination allocation,
conversion and total API time. Request time aggregates write plus header wait.
Guest capture, transport buffering and scheduling may overlap; host/guest wall
clocks must not be subtracted to manufacture a capture measurement.
