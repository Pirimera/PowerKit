//! HID++ report I/O on one HID collection. A channel belongs to the thread
//! that opened it: it is neither `Send` nor `Sync`, and every read, write
//! and the final close happen on that thread.
#![expect(
    unsafe_code,
    reason = "overlapped ReadFile/WriteFile on a HID collection handle"
)]
pub use powerkit_app::controller::power_mouse::{Gone, RequestError};
use powerkit_app::ports::HidppChannel;
use powerkit_core::power_mouse::hidpp::{LONG_REPORT_LEN, Message, PAYLOAD_LEN, Request};
use std::io;
use std::marker::PhantomData;
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Devices::HumanInterfaceDevice::{
        HIDP_CAPS, HIDP_STATUS_SUCCESS, HidD_FreePreparsedData, HidD_GetPreparsedData, HidP_GetCaps,
    },
    Foundation::{
        CloseHandle, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        ReadFile, WriteFile,
    },
    System::{
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        Threading::{CreateEventW, WaitForMultipleObjects},
    },
};

/// How long a device may take to answer one request. Bluetooth LE answers
/// in tens of milliseconds; a sleeping or departed device never does.
pub const REPLY_TIMEOUT: Duration = Duration::from_millis(1500);
const WRITE_TIMEOUT: Duration = Duration::from_millis(1000);
/// Largest input report a HID++ collection is read with.
const READ_BUFFER: usize = 64;

/// What a read produced.
#[derive(Debug)]
pub enum Read {
    Message(Message),
    /// A report arrived that is not HID++; nothing to act on.
    Other,
    /// The wake handle was signaled before a report arrived.
    Woken,
    Timeout,
}

/// The pending read's OVERLAPPED and buffer. They are boxed so their
/// addresses stay fixed while the kernel owns them, wherever the channel
/// itself moves.
struct PendingRead {
    overlapped: OVERLAPPED,
    buffer: [u8; READ_BUFFER],
    /// A ReadFile was issued and its result has not been collected.
    active: bool,
}

pub struct Channel {
    file: HANDLE,
    read_event: HANDLE,
    write_event: HANDLE,
    input_len: u32,
    output_len: usize,
    read: Box<PendingRead>,
    /// Events that arrived while waiting for a reply, oldest first.
    events: Vec<Message>,
    /// Raw handles and a pending kernel read: keep it on its own thread.
    _thread_bound: PhantomData<*mut ()>,
}

impl Channel {
    /// Opens the collection for report I/O. Other applications and Windows
    /// keep their own access: the collection is opened shared.
    pub fn open(path: &str) -> io::Result<Self> {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: wide is null-terminated and outlives the call.
        let file = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };
        if file == INVALID_HANDLE_VALUE || file.is_null() {
            return Err(io::Error::last_os_error());
        }
        // From here the handles are owned by `channel` and closed by Drop.
        let mut channel = Self {
            file,
            read_event: std::ptr::null_mut(),
            write_event: std::ptr::null_mut(),
            input_len: 0,
            output_len: 0,
            read: Box::new(PendingRead {
                overlapped: OVERLAPPED::default(),
                buffer: [0; READ_BUFFER],
                active: false,
            }),
            events: Vec::new(),
            _thread_bound: PhantomData,
        };
        channel.read_event = manual_event()?;
        channel.write_event = manual_event()?;
        let (input_len, output_len) = channel.report_lengths()?;
        if usize::from(output_len) < LONG_REPORT_LEN || usize::from(input_len) > READ_BUFFER {
            return Err(io::Error::other(
                "collection does not carry HID++ long reports",
            ));
        }
        channel.input_len = u32::from(input_len);
        channel.output_len = usize::from(output_len);
        Ok(channel)
    }

    fn report_lengths(&self) -> io::Result<(u16, u16)> {
        let mut preparsed = 0;
        // SAFETY: live handle; preparsed receives data freed below.
        if !unsafe { HidD_GetPreparsedData(self.file, &raw mut preparsed) } {
            return Err(io::Error::last_os_error());
        }
        let mut caps = HIDP_CAPS::default();
        // SAFETY: preparsed was just returned; caps is writable.
        let status = unsafe { HidP_GetCaps(preparsed, &raw mut caps) };
        // SAFETY: preparsed is freed exactly once.
        unsafe {
            HidD_FreePreparsedData(preparsed);
        }
        if status != HIDP_STATUS_SUCCESS {
            return Err(io::Error::other("HID capabilities are unavailable"));
        }
        Ok((caps.InputReportByteLength, caps.OutputReportByteLength))
    }

    /// Sends a request and waits for its reply. Events that arrive meanwhile
    /// are kept for [`Self::take_events`]; other clients' replies are dropped.
    pub fn request(&mut self, request: &Request) -> Result<[u8; PAYLOAD_LEN], RequestError> {
        self.write(&request.report())?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(RequestError::Timeout);
            }
            match self.read(left, None)? {
                Read::Message(message) => match message.reply_to(request) {
                    Some(Ok(payload)) => return Ok(payload),
                    Some(Err(error)) => return Err(RequestError::Device(error)),
                    None if message.is_event() => self.events.push(message),
                    None => {}
                },
                Read::Other => {}
                Read::Timeout | Read::Woken => return Err(RequestError::Timeout),
            }
        }
    }

    /// Sends a request the device does not answer, such as moving itself to
    /// another computer.
    pub fn notify(&mut self, request: &Request) -> io::Result<()> {
        self.write(&request.report())
    }

    /// Events received while requests were waiting for their replies.
    pub fn take_events(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.events)
    }

    fn write(&mut self, report: &[u8; LONG_REPORT_LEN]) -> io::Result<()> {
        // Windows wants exactly the collection's output report length.
        let mut buffer = vec![0u8; self.output_len];
        buffer[..LONG_REPORT_LEN].copy_from_slice(report);
        let mut overlapped = OVERLAPPED {
            hEvent: self.write_event,
            ..OVERLAPPED::default()
        };
        // SAFETY: buffer and overlapped live until the write has completed
        // or been cancelled and collected below, both before returning.
        let started = unsafe {
            WriteFile(
                self.file,
                buffer.as_ptr(),
                u32::try_from(buffer.len()).unwrap_or(0),
                std::ptr::null_mut(),
                &raw mut overlapped,
            )
        };
        // SAFETY: reads this thread's error slot right after WriteFile.
        if started == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
            return Err(io::Error::last_os_error());
        }
        let finished = wait(&[self.write_event], WRITE_TIMEOUT) == Some(0);
        if !finished {
            // SAFETY: cancels only this write; the result is collected next.
            unsafe {
                CancelIoEx(self.file, &raw const overlapped);
            }
        }
        let mut written = 0;
        // SAFETY: waits for the write to finish or its cancellation to land,
        // so the kernel no longer references buffer or overlapped afterwards.
        let ok =
            unsafe { GetOverlappedResult(self.file, &raw const overlapped, &raw mut written, 1) };
        if ok == 0 || !finished {
            return Err(if finished {
                io::Error::last_os_error()
            } else {
                io::Error::new(io::ErrorKind::TimedOut, "HID++ write timed out")
            });
        }
        Ok(())
    }

    /// Waits for the next report, for `wake` to be signaled or for the
    /// timeout. A read left pending by a timeout or a wake is reused by the
    /// next call, so no report is lost in between.
    pub fn read(&mut self, timeout: Duration, wake: Option<HANDLE>) -> io::Result<Read> {
        if !self.read.active {
            self.read.overlapped = OVERLAPPED {
                hEvent: self.read_event,
                ..OVERLAPPED::default()
            };
            // SAFETY: the boxed buffer and OVERLAPPED stay at fixed addresses
            // and are not touched again until the read is collected or
            // cancelled (here, or in Drop).
            let started = unsafe {
                ReadFile(
                    self.file,
                    self.read.buffer.as_mut_ptr(),
                    self.input_len,
                    std::ptr::null_mut(),
                    &raw mut self.read.overlapped,
                )
            };
            // SAFETY: reads this thread's error slot right after ReadFile.
            if started == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
                return Err(io::Error::last_os_error());
            }
            self.read.active = true;
        }
        let signaled = match wake {
            Some(wake) => wait(&[self.read_event, wake], timeout),
            None => wait(&[self.read_event], timeout),
        };
        match signaled {
            Some(0) => {}
            Some(_) => return Ok(Read::Woken),
            None => return Ok(Read::Timeout),
        }
        let mut len = 0;
        // SAFETY: the read has completed (its event is signaled); this only
        // collects the result.
        let ok = unsafe {
            GetOverlappedResult(self.file, &raw const self.read.overlapped, &raw mut len, 0)
        };
        self.read.active = false;
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let len = (len as usize).min(READ_BUFFER);
        Ok(Message::parse(&self.read.buffer[..len]).map_or(Read::Other, Read::Message))
    }
}

impl HidppChannel for Channel {
    fn request(&mut self, request: &Request) -> Result<[u8; PAYLOAD_LEN], RequestError> {
        Channel::request(self, request)
    }
    fn notify(&mut self, request: &Request) -> io::Result<()> {
        Channel::notify(self, request)
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        if self.read.active {
            let mut len = 0;
            // SAFETY: cancels the pending read and waits until the kernel has
            // released the boxed buffer and OVERLAPPED before they are freed.
            unsafe {
                CancelIoEx(self.file, &raw const self.read.overlapped);
                GetOverlappedResult(self.file, &raw const self.read.overlapped, &raw mut len, 1);
            }
        }
        for handle in [self.file, self.read_event, self.write_event] {
            if !handle.is_null() {
                // SAFETY: each handle is owned by this channel and closed once.
                unsafe {
                    CloseHandle(handle);
                }
            }
        }
    }
}

fn manual_event() -> io::Result<HANDLE> {
    // SAFETY: unnamed manual-reset event, initially not signaled. Overlapped
    // I/O resets it when an operation starts and sets it on completion.
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(event)
}

/// Index of the first signaled handle, or `None` on timeout or failure.
fn wait(handles: &[HANDLE], timeout: Duration) -> Option<usize> {
    let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
    // SAFETY: handles are live for the duration of the call.
    let result = unsafe {
        WaitForMultipleObjects(
            u32::try_from(handles.len()).ok()?,
            handles.as_ptr(),
            0,
            millis,
        )
    };
    if result == WAIT_TIMEOUT {
        return None;
    }
    let index = result.checked_sub(WAIT_OBJECT_0)? as usize;
    (index < handles.len()).then_some(index)
}
