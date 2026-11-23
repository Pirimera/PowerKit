//! Worker threads for MediaRenamer. A job runs one scan, apply, undo or history load on its
//! own thread, reports through a channel, and wakes the main thread so it drains the
//! channel promptly.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// Asks the main thread to poll the session. Called from worker threads.
pub type Wake = Arc<dyn Fn() + Send + Sync>;
const WAKE_INTERVAL: Duration = Duration::from_millis(150);

/// Sends worker events and wakes the main thread, at most every `WAKE_INTERVAL`
/// unless the event is final.
pub(super) struct Reporter<E> {
    sender: Sender<E>,
    wake: Wake,
    last: Option<Instant>,
}
impl<E> Reporter<E> {
    pub(super) fn send(&mut self, event: E, urgent: bool) {
        if self.sender.send(event).is_err() {
            return;
        }
        let due = self.last.is_none_or(|last| last.elapsed() >= WAKE_INTERVAL);
        if urgent || due {
            self.last = Some(Instant::now());
            (self.wake)();
        }
    }
}

pub(super) struct Job<E> {
    pub(super) receiver: Receiver<E>,
    cancel: Arc<AtomicBool>,
    pub(super) thread: Option<JoinHandle<()>>,
}
impl<E: Send + 'static> Job<E> {
    pub(super) fn spawn(
        wake: Wake,
        body: impl FnOnce(Reporter<E>, &AtomicBool) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let thread = std::thread::Builder::new()
            .name("powerkit-media-renamer".into())
            .spawn(move || {
                body(
                    Reporter {
                        sender,
                        wake,
                        last: None,
                    },
                    &flag,
                )
            })
            .ok();
        Self {
            receiver,
            cancel,
            thread,
        }
    }
}
impl<E> Job<E> {
    pub(super) fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    /// Waits a bounded time; a rename in progress finishes its current file first.
    pub(super) fn join(mut self, limit: Duration) {
        self.stop();
        let deadline = Instant::now() + limit;
        if let Some(thread) = self.thread.take() {
            while !thread.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}
