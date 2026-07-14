//! Bidirectional clipboard sync, symmetric on both host and controller: a
//! background thread polls the local clipboard and reports changes via
//! `on_local_change`, while `apply_remote_update` feeds incoming updates
//! back in. Both are funneled through the same thread-confined backend so
//! applying a remote update also updates the "last seen" baseline, which is
//! what stops an incoming update from being immediately polled back out and
//! re-sent (an echo loop).
//!
//! Decoupled from WebRTC entirely (`on_local_change` is a plain callback) so
//! the echo-avoidance logic can be unit tested without a real data channel
//! or the real OS clipboard — see the test module below.

use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use visuara_agent::clipboard::ClipboardBackend;

pub struct ClipboardSync {
    remote_update_tx: std_mpsc::Sender<String>,
}

impl ClipboardSync {
    /// Spawns the polling thread. `backend_factory` constructs the clipboard
    /// backend on that thread (real `ClipboardHandle` in production, a fake
    /// in tests). `on_local_change` is called with the new text whenever a
    /// genuine local clipboard change is observed.
    pub fn start(
        backend_factory: impl FnOnce() -> anyhow::Result<Box<dyn ClipboardBackend>> + Send + 'static,
        on_local_change: impl Fn(String) + Send + 'static,
    ) -> Self {
        Self::start_with_interval(backend_factory, on_local_change, Duration::from_millis(500))
    }

    fn start_with_interval(
        backend_factory: impl FnOnce() -> anyhow::Result<Box<dyn ClipboardBackend>> + Send + 'static,
        on_local_change: impl Fn(String) + Send + 'static,
        poll_interval: Duration,
    ) -> Self {
        let (remote_update_tx, remote_update_rx) = std_mpsc::channel::<String>();

        std::thread::spawn(move || {
            let mut backend = match backend_factory() {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("[clipboard-sync] failed to open clipboard: {e:#}");
                    return;
                }
            };
            let mut last_seen = String::new();
            loop {
                while let Ok(text) = remote_update_rx.try_recv() {
                    let _ = backend.set_text(&text);
                    last_seen = text;
                }
                if let Ok(text) = backend.get_text() {
                    if text != last_seen {
                        last_seen = text.clone();
                        on_local_change(text);
                    }
                }
                std::thread::sleep(poll_interval);
            }
        });

        Self { remote_update_tx }
    }

    /// Call when a clipboard update arrives from the remote peer.
    pub fn apply_remote_update(&self, text: String) {
        let _ = self.remote_update_tx.send(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct FakeBackend {
        text: Arc<Mutex<String>>,
    }

    impl ClipboardBackend for FakeBackend {
        fn get_text(&mut self) -> anyhow::Result<String> {
            Ok(self.text.lock().unwrap().clone())
        }

        fn set_text(&mut self, text: &str) -> anyhow::Result<()> {
            *self.text.lock().unwrap() = text.to_string();
            Ok(())
        }
    }

    #[test]
    fn local_change_is_reported_once() {
        let backing = Arc::new(Mutex::new(String::new()));
        let backing_for_factory = backing.clone();
        let (changes_tx, changes_rx) = std_mpsc::channel::<String>();

        let _sync = ClipboardSync::start_with_interval(
            move || Ok(Box::new(FakeBackend { text: backing_for_factory }) as Box<dyn ClipboardBackend>),
            move |text| {
                let _ = changes_tx.send(text);
            },
            Duration::from_millis(20),
        );

        *backing.lock().unwrap() = "hello from local".to_string();

        let seen = changes_rx.recv_timeout(Duration::from_secs(2)).expect("expected a local change to be reported");
        assert_eq!(seen, "hello from local");

        // No further changes should be reported since nothing changed again.
        assert!(changes_rx.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn remote_update_is_not_echoed_back() {
        let backing = Arc::new(Mutex::new(String::new()));
        let backing_for_factory = backing.clone();
        let (changes_tx, changes_rx) = std_mpsc::channel::<String>();

        let sync = ClipboardSync::start_with_interval(
            move || Ok(Box::new(FakeBackend { text: backing_for_factory }) as Box<dyn ClipboardBackend>),
            move |text| {
                let _ = changes_tx.send(text);
            },
            Duration::from_millis(20),
        );

        sync.apply_remote_update("from the remote side".to_string());

        // The backend should now hold the remote text...
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(*backing.lock().unwrap(), "from the remote side");

        // ...but that must NOT be reported back out as a "local change",
        // which would otherwise bounce forever between host and controller.
        assert!(changes_rx.recv_timeout(Duration::from_millis(300)).is_err());
    }
}
