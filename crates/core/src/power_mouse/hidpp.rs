//! Pure HID++ 2.0 framing for the Logitech adapter: building requests,
//! parsing reports, matching a reply to its request and decoding the few
//! features PowerMouse uses. No I/O happens here.
//!
//! Wire format (long report, 20 bytes): report id `0x11`, device index,
//! feature index, function in the high nibble and software id in the low
//! nibble, then 16 payload bytes. A short report (`0x10`, 7 bytes) has the
//! same header and 3 payload bytes. Multi-byte values are big-endian.

pub const SHORT_REPORT_ID: u8 = 0x10;
pub const LONG_REPORT_ID: u8 = 0x11;
pub const SHORT_REPORT_LEN: usize = 7;
pub const LONG_REPORT_LEN: usize = 20;
pub const PAYLOAD_LEN: usize = 16;
/// Device index of a directly connected device (Bluetooth or cable). A
/// device behind a receiver uses its pairing slot instead.
pub const DIRECT_DEVICE: u8 = 0xFF;
/// Identifies PowerKit's requests so replies can be told from another
/// client's. Zero is reserved: the device uses it for its own events.
pub const SOFTWARE_ID: u8 = 0x0D;
/// Feature index a device answers with when a request failed.
const ERROR_INDEX: u8 = 0xFF;

/// Feature ids, looked up through the root feature to get their index.
pub mod feature {
    pub const DEVICE_NAME: u16 = 0x0005;
    /// Its event tells the host that the device reconnected and lost its
    /// volatile settings, diversions included.
    pub const WIRELESS_STATUS: u16 = 0x1D4B;
    pub const UNIFIED_BATTERY: u16 = 0x1004;
    pub const REPROG_CONTROLS: u16 = 0x1B04;
    pub const CHANGE_HOST: u16 = 0x1814;
    pub const HAPTIC_FEEDBACK: u16 = 0x19B0;
    pub const SMARTSHIFT_ENHANCED: u16 = 0x2111;
    pub const HIRES_WHEEL: u16 = 0x2121;
    pub const THUMB_WHEEL: u16 = 0x2150;
    pub const ADJUSTABLE_DPI: u16 = 0x2201;
}

/// Control ids of the buttons PowerMouse can divert.
pub mod control {
    /// The thumb-rest gesture button of the MX Master family.
    pub const GESTURE_BUTTON: u16 = 0x00C3;
    /// The button behind the wheel that switches ratchet and free spin.
    pub const MODE_SHIFT: u16 = 0x00C4;
    /// The MX Master 4 haptic sense panel.
    pub const HAPTIC_PANEL: u16 = 0x01A0;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub device: u8,
    pub feature_index: u8,
    pub function: u8,
    pub params: [u8; PAYLOAD_LEN],
}

impl Request {
    /// `params` longer than the payload are truncated; the rest is zero.
    pub fn new(device: u8, feature_index: u8, function: u8, params: &[u8]) -> Self {
        let mut padded = [0; PAYLOAD_LEN];
        let len = params.len().min(PAYLOAD_LEN);
        padded[..len].copy_from_slice(&params[..len]);
        Self {
            device,
            feature_index,
            function: function & 0x0F,
            params: padded,
        }
    }

    /// The long report to write. Long reports are accepted on every HID++
    /// 2.0 channel; Bluetooth carries nothing else.
    pub fn report(&self) -> [u8; LONG_REPORT_LEN] {
        let mut report = [0; LONG_REPORT_LEN];
        report[0] = LONG_REPORT_ID;
        report[1] = self.device;
        report[2] = self.feature_index;
        report[3] = self.function << 4 | SOFTWARE_ID;
        report[4..].copy_from_slice(&self.params);
        report
    }
}

/// A request the device refused; the code is the HID++ 2.0 error number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceError(pub u8);

/// One report received from a device, short payloads zero-extended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    pub device: u8,
    pub feature_index: u8,
    pub function: u8,
    pub software_id: u8,
    pub payload: [u8; PAYLOAD_LEN],
}

impl Message {
    /// Parses an input report including its report id. Anything that is not
    /// a complete HID++ report is `None`.
    pub fn parse(report: &[u8]) -> Option<Self> {
        let len = match *report.first()? {
            SHORT_REPORT_ID => SHORT_REPORT_LEN,
            LONG_REPORT_ID => LONG_REPORT_LEN,
            _ => return None,
        };
        let report = report.get(..len)?;
        let mut payload = [0; PAYLOAD_LEN];
        payload[..len - 4].copy_from_slice(&report[4..]);
        Some(Self {
            device: report[1],
            feature_index: report[2],
            function: report[3] >> 4,
            software_id: report[3] & 0x0F,
            payload,
        })
    }

    /// Whether the device sent this on its own: events carry software id 0.
    pub fn is_event(&self) -> bool {
        self.software_id == 0 && self.feature_index != ERROR_INDEX
    }

    /// The outcome of `request` if this message is its reply. An error reply
    /// has feature index `0xFF` and repeats the request's feature index and
    /// function byte one position to the right, followed by the error code.
    pub fn reply_to(&self, request: &Request) -> Option<Result<[u8; PAYLOAD_LEN], DeviceError>> {
        if self.device != request.device {
            return None;
        }
        let function_byte = request.function << 4 | SOFTWARE_ID;
        if self.feature_index == ERROR_INDEX {
            let echoes_request = (self.function << 4 | self.software_id) == request.feature_index
                && self.payload[0] == function_byte;
            return echoes_request.then_some(Err(DeviceError(self.payload[1])));
        }
        (self.feature_index == request.feature_index
            && self.function == request.function
            && self.software_id == SOFTWARE_ID)
            .then_some(Ok(self.payload))
    }
}

/// Root feature (always index 0), function 1: echoes the third byte. The
/// reply also proves the device speaks HID++ 2.0 at this device index.
pub fn ping(device: u8, token: u8) -> Request {
    Request::new(device, 0, 1, &[0, 0, token])
}

/// Root feature, function 0: where a feature lives in the device's table.
pub fn feature_lookup(device: u8, feature_id: u16) -> Request {
    Request::new(device, 0, 0, &feature_id.to_be_bytes())
}

/// The index from a [`feature_lookup`] reply; `None` when the device does
/// not have the feature (index 0 is the root itself).
pub fn feature_index(reply: &[u8; PAYLOAD_LEN]) -> Option<u8> {
    (reply[0] != 0).then_some(reply[0])
}

/// Device type reported by the name feature (function 2) for a mouse.
pub const DEVICE_TYPE_MOUSE: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    /// State of charge in percent, 0–100.
    pub percent: u8,
    pub charging: bool,
}

impl Battery {
    /// Decodes a unified-battery status, from the reply to function 1 or
    /// from the status event (event 0); both share this layout: percent,
    /// level flags, charging status.
    pub fn from_status(payload: &[u8; PAYLOAD_LEN]) -> Option<Self> {
        let percent = payload[0];
        // 0 discharging, 1 charging, 2 nearly full, 3 full, 4 slow charge;
        // 5–7 are battery faults.
        let charging = match payload[2] {
            0 | 3 => false,
            1 | 2 | 4 => true,
            _ => return None,
        };
        (percent <= 100).then_some(Self { percent, charging })
    }
}

/// One row of the device's control table (reprogrammable controls,
/// function 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    pub id: u16,
    pub task: u16,
    flags: u16,
}

impl Control {
    pub fn from_info(payload: &[u8; PAYLOAD_LEN]) -> Self {
        Self {
            id: u16::from_be_bytes([payload[0], payload[1]]),
            task: u16::from_be_bytes([payload[2], payload[3]]),
            flags: u16::from(payload[4]) | u16::from(payload[8]) << 8,
        }
    }

    /// The control can be handed to host software until the device powers
    /// down or reconnects.
    pub fn divertable(&self) -> bool {
        self.flags & 1 << 5 != 0
    }

    /// While held, the control can report pointer movement instead of moving
    /// the cursor.
    pub fn raw_xy(&self) -> bool {
        self.flags & 1 << 8 != 0
    }
}

/// Reprogrammable controls, function 3: turn temporary diversion of a
/// control on or off, with raw movement following the same switch when
/// `raw_xy` is set. Each setting is a value bit plus a "valid" bit; settings
/// without the valid bit, and the remap, are left as they are.
pub fn set_diversion(
    device: u8,
    feature_index: u8,
    control: u16,
    diverted: bool,
    raw_xy: bool,
) -> Request {
    let [high, low] = control.to_be_bytes();
    let mut flags = 1 << 1 | u8::from(diverted);
    if raw_xy {
        flags |= 1 << 5 | u8::from(diverted) << 4;
    }
    Request::new(device, feature_index, 3, &[high, low, flags])
}

/// What a device reports on its own through the reprogrammable-controls
/// feature while controls are diverted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlEvent {
    /// Every diverted control held down right now; unused slots are 0.
    Pressed([u16; 4]),
    /// Pointer movement while a raw-XY control is held.
    Moved { dx: i16, dy: i16 },
}

impl ControlEvent {
    /// Decodes an event of the feature at `feature_index`; `None` for other
    /// features, replies and event kinds PowerMouse does not use.
    pub fn decode(message: &Message, feature_index: u8) -> Option<Self> {
        if !message.is_event() || message.feature_index != feature_index {
            return None;
        }
        let word = |at: usize| [message.payload[at], message.payload[at + 1]];
        match message.function {
            0 => Some(Self::Pressed([
                u16::from_be_bytes(word(0)),
                u16::from_be_bytes(word(2)),
                u16::from_be_bytes(word(4)),
                u16::from_be_bytes(word(6)),
            ])),
            1 => Some(Self::Moved {
                dx: i16::from_be_bytes(word(0)),
                dy: i16::from_be_bytes(word(2)),
            }),
            _ => None,
        }
    }
}

/// The DPI values a sensor accepts (adjustable DPI, function 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DpiSupport {
    /// Inclusive ranges as (lowest, highest, step); a single value is a
    /// range of one.
    ranges: Vec<(u16, u16, u16)>,
}

impl DpiSupport {
    /// Decodes the list after the sensor index: big-endian words ending at
    /// 0. A word with its top three bits set is a step, turning the values
    /// before and after it into the ends of a range.
    pub fn from_list(payload: &[u8; PAYLOAD_LEN]) -> Self {
        let words: Vec<u16> = payload[1..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .take_while(|word| *word != 0)
            .collect();
        let mut ranges: Vec<(u16, u16, u16)> = Vec::new();
        let mut index = 0;
        while index < words.len() {
            let word = words[index];
            if word & 0xE000 == 0xE000 {
                let step = word & 0x1FFF;
                if let (Some(last), Some(end)) = (ranges.last_mut(), words.get(index + 1))
                    && step > 0
                    && *end >= last.0
                {
                    *last = (last.0, *end, step);
                }
                index += 2;
            } else {
                ranges.push((word, word, 1));
                index += 1;
            }
        }
        Self { ranges }
    }

    pub fn supports(&self, dpi: u16) -> bool {
        self.ranges.iter().any(|(low, high, step)| {
            (*low..=*high).contains(&dpi) && (dpi - low).is_multiple_of(*step)
        })
    }
}

/// Adjustable DPI, function 2: the sensor's current DPI.
pub fn dpi_from_reply(reply: &[u8; PAYLOAD_LEN]) -> u16 {
    u16::from_be_bytes([reply[1], reply[2]])
}

/// Adjustable DPI, function 3, for sensor 0.
pub fn set_dpi(device: u8, feature_index: u8, dpi: u16) -> Request {
    let [high, low] = dpi.to_be_bytes();
    Request::new(device, feature_index, 3, &[0, high, low])
}

/// How the main wheel turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelMode {
    /// Always clicks from notch to notch.
    Ratchet,
    /// Always spins freely.
    FreeSpin,
    /// Ratchet that releases into free spin when the wheel is flicked.
    SmartShift,
}

impl WheelMode {
    pub const ALL: [Self; 3] = [Self::Ratchet, Self::FreeSpin, Self::SmartShift];

    /// Stable name used in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            Self::Ratchet => "ratchet",
            Self::FreeSpin => "free_spin",
            Self::SmartShift => "smart_shift",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.key() == key)
    }
}

/// SmartShift enhanced, function 2: wheel mode (1 free spin, 2 ratchet),
/// then the speed at which the ratchet releases, then the torque. Zero keeps
/// a value; a release speed of `0xFF` never releases. `auto_disengage` is
/// the device's own default release speed, from its capabilities.
pub fn set_wheel_mode(
    device: u8,
    feature_index: u8,
    mode: WheelMode,
    auto_disengage: u8,
) -> Request {
    let params = match mode {
        WheelMode::Ratchet => [2, 0xFF],
        WheelMode::FreeSpin => [1, 0],
        WheelMode::SmartShift => [2, auto_disengage.clamp(1, 0xFE)],
    };
    Request::new(device, feature_index, 2, &params)
}

/// High-resolution wheel, function 2, from the mode read with function 1:
/// only the resolution bit changes, so the event target, the inversion and
/// any bits this code does not know stay as the device reported them.
pub fn set_wheel_resolution(
    device: u8,
    feature_index: u8,
    current_mode: &[u8; PAYLOAD_LEN],
    high: bool,
) -> Request {
    const RESOLUTION: u8 = 1 << 1;
    let flags = current_mode[0] & !RESOLUTION | u8::from(high) << 1;
    Request::new(
        device,
        feature_index,
        2,
        &[flags, current_mode[1], current_mode[2]],
    )
}

/// Thumb wheel, function 2: report rotation to the host (diverted) instead
/// of scrolling horizontally (native). The direction is not inverted.
pub fn set_thumb_wheel_diverted(device: u8, feature_index: u8, diverted: bool) -> Request {
    Request::new(device, feature_index, 2, &[u8::from(diverted), 0])
}

/// Rotation from a thumb-wheel event (event 0) of the feature at
/// `feature_index`; positive is left or back on a device whose default
/// direction is 0.
pub fn thumb_wheel_rotation(message: &Message, feature_index: u8) -> Option<i16> {
    (message.is_event() && message.feature_index == feature_index && message.function == 0)
        .then(|| i16::from_be_bytes([message.payload[0], message.payload[1]]))
}

/// Change host, function 1: move the device to another paired computer.
/// `host` counts from 0. The device leaves at once and sends no reply.
pub fn change_host(device: u8, feature_index: u8, host: u8) -> Request {
    Request::new(device, feature_index, 1, &[host])
}

/// Haptic feedback, function 4: play one waveform. 4 is a short, subtle tap.
pub fn play_haptic(device: u8, feature_index: u8) -> Request {
    Request::new(device, feature_index, 4, &[4])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long(bytes: &[u8]) -> Message {
        let mut report = [0u8; LONG_REPORT_LEN];
        report[0] = LONG_REPORT_ID;
        report[1..=bytes.len()].copy_from_slice(bytes);
        Message::parse(&report).unwrap()
    }

    #[test]
    fn request_is_framed_as_a_long_report() {
        let request = feature_lookup(DIRECT_DEVICE, feature::REPROG_CONTROLS);
        let report = request.report();
        assert_eq!(report[..6], [0x11, 0xFF, 0x00, 0x0D, 0x1B, 0x04]);
        assert!(report[6..].iter().all(|byte| *byte == 0));
        // Function 3 lands in the high nibble beside the software id.
        assert_eq!(
            set_diversion(1, 9, control::GESTURE_BUTTON, true, true).report()[..7],
            [0x11, 0x01, 0x09, 0x3D, 0x00, 0xC3, 0b0011_0011]
        );
        assert_eq!(
            set_diversion(1, 9, control::HAPTIC_PANEL, false, false).report()[4..7],
            [0x01, 0xA0, 0b0000_0010]
        );
    }

    #[test]
    fn reports_parse_by_id_and_reject_truncated_or_foreign_ones() {
        let short = Message::parse(&[0x10, 0x01, 0x05, 0x1D, 0xAA, 0xBB, 0xCC]).unwrap();
        assert_eq!((short.device, short.feature_index), (1, 5));
        assert_eq!((short.function, short.software_id), (1, 0x0D));
        assert_eq!(short.payload[..4], [0xAA, 0xBB, 0xCC, 0]);
        assert!(Message::parse(&[0x10, 0x01, 0x05]).is_none());
        assert!(Message::parse(&[0x11; 19]).is_none());
        assert!(Message::parse(&[0x01, 0, 0, 0, 0, 0, 0]).is_none());
        assert!(Message::parse(&[]).is_none());
        // Windows pads input reports; extra bytes after a full report are fine.
        assert!(Message::parse(&[0x10, 1, 2, 3, 4, 5, 6, 0, 0]).is_some());
    }

    #[test]
    fn a_reply_is_matched_to_its_request_only() {
        let request = Request::new(DIRECT_DEVICE, 7, 3, &[]);
        let ok = long(&[0xFF, 7, 0x3D, 0x42]);
        assert_eq!(ok.reply_to(&request).unwrap().unwrap()[0], 0x42);
        // Another client's reply, another function, another device, an event.
        for other in [
            long(&[0xFF, 7, 0x3A, 0x42]),
            long(&[0xFF, 7, 0x2D, 0x42]),
            long(&[0x01, 7, 0x3D, 0x42]),
            long(&[0xFF, 7, 0x30, 0x42]),
        ] {
            assert_eq!(other.reply_to(&request), None, "{other:?}");
        }
        // An error names the request: index 7, function byte 0x3D, code 2.
        let error = long(&[0xFF, 0xFF, 0x07, 0x3D, 0x02]);
        assert_eq!(error.reply_to(&request), Some(Err(DeviceError(2))));
        assert!(!error.is_event());
        let other_error = long(&[0xFF, 0xFF, 0x08, 0x3D, 0x02]);
        assert_eq!(other_error.reply_to(&request), None);
    }

    #[test]
    fn feature_battery_and_control_replies_decode() {
        let mut payload = [0u8; PAYLOAD_LEN];
        assert_eq!(feature_index(&payload), None);
        payload[0] = 9;
        assert_eq!(feature_index(&payload), Some(9));

        let status = |percent, state| {
            let mut payload = [0u8; PAYLOAD_LEN];
            payload[0] = percent;
            payload[2] = state;
            Battery::from_status(&payload)
        };
        assert_eq!(
            status(55, 0),
            Some(Battery {
                percent: 55,
                charging: false
            })
        );
        assert!(status(80, 1).unwrap().charging);
        assert!(!status(100, 3).unwrap().charging);
        assert_eq!(status(50, 5), None, "a battery fault is not a level");
        assert_eq!(status(200, 0), None);

        let mut info = [0u8; PAYLOAD_LEN];
        info[..5].copy_from_slice(&[0x00, 0xC3, 0x00, 0x9C, 0b0011_0001]);
        info[8] = 0b0000_0001;
        let control = Control::from_info(&info);
        assert_eq!((control.id, control.task), (0x00C3, 0x009C));
        assert!(control.divertable() && control.raw_xy());
        info[4] = 0b0000_0001;
        info[8] = 0;
        let plain = Control::from_info(&info);
        assert!(!plain.divertable() && !plain.raw_xy());
    }

    #[test]
    fn dpi_list_decodes_ranges_and_single_values() {
        // The list an MX Master 4 reports: 200 to 8000 in steps of 50.
        let mut payload = [0u8; PAYLOAD_LEN];
        payload[1..7].copy_from_slice(&[0x00, 0xC8, 0xE0, 0x32, 0x1F, 0x40]);
        let range = DpiSupport::from_list(&payload);
        assert!(range.supports(200) && range.supports(1000) && range.supports(8000));
        assert!(!range.supports(150) && !range.supports(1025) && !range.supports(8050));

        // Discrete values, as older mice report them.
        let mut payload = [0u8; PAYLOAD_LEN];
        payload[1..7].copy_from_slice(&[0x01, 0x90, 0x03, 0x20, 0x06, 0x40]);
        let list = DpiSupport::from_list(&payload);
        assert!(list.supports(400) && list.supports(800) && list.supports(1600));
        assert!(!list.supports(1000));
        assert_eq!(
            DpiSupport::from_list(&[0; PAYLOAD_LEN]),
            DpiSupport::default()
        );
        // A step with nothing before it, or a zero step, is ignored.
        let mut payload = [0u8; PAYLOAD_LEN];
        payload[1..5].copy_from_slice(&[0xE0, 0x32, 0x1F, 0x40]);
        assert!(!DpiSupport::from_list(&payload).supports(8000));

        let mut reply = [0u8; PAYLOAD_LEN];
        reply[..5].copy_from_slice(&[0, 0x03, 0xE8, 0x03, 0xE8]);
        assert_eq!(dpi_from_reply(&reply), 1000);
        assert_eq!(
            set_dpi(DIRECT_DEVICE, 20, 1600).report()[2..7],
            [20, 0x3D, 0, 0x06, 0x40]
        );
    }

    #[test]
    fn wheel_host_and_haptic_requests_are_framed() {
        let params = |request: Request| request.params[..3].to_vec();
        assert_eq!(
            params(set_wheel_mode(1, 17, WheelMode::Ratchet, 10)),
            [2, 0xFF, 0]
        );
        assert_eq!(
            params(set_wheel_mode(1, 17, WheelMode::FreeSpin, 10)),
            [1, 0, 0]
        );
        assert_eq!(
            params(set_wheel_mode(1, 17, WheelMode::SmartShift, 10)),
            [2, 10, 0]
        );
        // A missing default must not turn SmartShift into "keep" or "never".
        assert_eq!(
            params(set_wheel_mode(1, 17, WheelMode::SmartShift, 0)),
            [2, 1, 0]
        );
        assert_eq!(set_wheel_mode(1, 17, WheelMode::Ratchet, 10).function, 2);

        // Only the resolution bit changes; inversion and unknown bits stay.
        let mut mode = [0u8; PAYLOAD_LEN];
        mode[..3].copy_from_slice(&[0b1010_0100, 0x12, 0x34]);
        assert_eq!(
            params(set_wheel_resolution(1, 18, &mode, true)),
            [0b1010_0110, 0x12, 0x34]
        );
        mode[0] = 0b0000_0010;
        assert_eq!(
            params(set_wheel_resolution(1, 18, &mode, false)),
            [0, 0x12, 0x34]
        );

        assert_eq!(params(set_thumb_wheel_diverted(1, 19, true)), [1, 0, 0]);
        assert_eq!(params(set_thumb_wheel_diverted(1, 19, false)), [0, 0, 0]);
        let turned = long(&[0xFF, 19, 0x00, 0xFF, 0xEC]);
        assert_eq!(thumb_wheel_rotation(&turned, 19), Some(-20));
        assert_eq!(thumb_wheel_rotation(&turned, 18), None);
        assert_eq!(
            thumb_wheel_rotation(&long(&[0xFF, 19, 0x1D, 0, 5]), 19),
            None
        );

        let host = change_host(DIRECT_DEVICE, 14, 2);
        assert_eq!((host.function, host.params[0]), (1, 2));
        let tap = play_haptic(DIRECT_DEVICE, 11);
        assert_eq!((tap.function, tap.params[0]), (4, 4));
    }

    #[test]
    fn control_events_decode_only_from_the_controls_feature() {
        let pressed = long(&[0xFF, 9, 0x00, 0x00, 0xC3, 0x01, 0xA0]);
        assert_eq!(
            ControlEvent::decode(&pressed, 9),
            Some(ControlEvent::Pressed([0x00C3, 0x01A0, 0, 0]))
        );
        let released = long(&[0xFF, 9, 0x00]);
        assert_eq!(
            ControlEvent::decode(&released, 9),
            Some(ControlEvent::Pressed([0; 4]))
        );
        let moved = long(&[0xFF, 9, 0x10, 0xFF, 0xF6, 0x00, 0x14]);
        assert_eq!(
            ControlEvent::decode(&moved, 9),
            Some(ControlEvent::Moved { dx: -10, dy: 20 })
        );
        // Another feature's event, and a reply on the right feature.
        assert_eq!(ControlEvent::decode(&pressed, 8), None);
        assert_eq!(
            ControlEvent::decode(&long(&[0xFF, 9, 0x0D, 0x00, 0xC3]), 9),
            None
        );
    }
}
