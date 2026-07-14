//! Runtime smoke test against the real display: capture a frame from the
//! primary monitor and run it through the H.264 encoder. Read-only (no input
//! injection), safe to run on a real desktop session.

use visuara_agent::capture::{list_monitors, Capturer};
use visuara_agent::encode::VideoEncoder;

#[test]
fn capture_and_encode_one_frame() {
    let monitors = list_monitors().expect("list monitors");
    assert!(!monitors.is_empty(), "expected at least one monitor");
    println!("monitors: {monitors:?}");

    let capturer = Capturer::primary().expect("open primary monitor");
    assert!(capturer.width() > 0);
    assert!(capturer.height() > 0);

    let frame = capturer.capture_frame().expect("capture frame");
    assert_eq!(frame.width(), capturer.width());
    assert_eq!(frame.height(), capturer.height());

    let mut encoder = VideoEncoder::new().expect("create encoder");
    let encoded = encoder.encode_frame(&frame).expect("encode frame");
    assert!(!encoded.is_empty(), "expected non-empty H.264 bitstream");
    println!("encoded {} bytes from a {}x{} frame", encoded.len(), frame.width(), frame.height());
}
