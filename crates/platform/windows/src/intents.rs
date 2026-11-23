//! Carries intents from a UI to the app window.
//!
//! The settings shell and the app window share the UI thread, so an intent
//! waits in a thread-local queue and a posted message tells the app window to
//! drain it. Intents therefore run in the order they were sent and never
//! inside the UI kit's own event handler.
use powerkit_app::intent::Intent;
use std::{cell::RefCell, collections::VecDeque};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{PostMessageW, WM_APP},
};

/// Tells the app window that intents are waiting.
pub const WAKE: u32 = WM_APP + 14;

thread_local! {
    static QUEUE: RefCell<VecDeque<Intent>> = const { RefCell::new(VecDeque::new()) };
}

/// Queues an intent for the app window `app`, which must live on this thread.
pub fn send(app: HWND, intent: Intent) {
    QUEUE.with(|queue| queue.borrow_mut().push_back(intent));
    // SAFETY: posting to a window handle has no memory-safety requirement; a
    // destroyed window makes the call fail and the intent is never run.
    unsafe {
        PostMessageW(app, WAKE, 0, 0);
    }
}

/// The oldest waiting intent. Taken one at a time so a handler that pumps
/// messages (an error dialog) can let the next wake continue the queue.
pub fn next() -> Option<Intent> {
    QUEUE.with(|queue| queue.borrow_mut().pop_front())
}
