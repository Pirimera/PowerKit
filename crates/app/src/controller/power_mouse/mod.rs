//! PowerMouse device adapters above the transport: the Logitech HID++ 2.0
//! conversation with a mouse, spoken over a [`HidppChannel`].
//!
//! The button engine itself is `powerkit_core::power_mouse`. Reading mouse
//! input, injecting keys and opening a device are the platform's job; this
//! module only decides which requests to send and what their replies mean.
//! Adapter features stay off for models that were not physically verified:
//! nothing here chooses which device to talk to.
pub mod device_settings;
pub mod link;
pub mod logitech;

use powerkit_core::power_mouse::hidpp::DeviceError;
use std::io;

pub use crate::ports::HidppChannel;

/// Why a request produced no payload.
#[derive(Debug)]
pub enum RequestError {
    /// The device answered with an HID++ error code.
    Device(DeviceError),
    /// No matching reply arrived in time.
    Timeout,
    /// The channel failed: usually the device left.
    Io(io::Error),
}

impl From<io::Error> for RequestError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// The device's channel failed: it has left and the channel is of no more use.
#[derive(Debug, PartialEq, Eq)]
pub struct Gone;

/// Separates a request that merely did not take (`Ok(None)`: the device
/// refused it or did not answer in time) from a device that is gone.
pub fn settled<T>(result: Result<T, RequestError>) -> Result<Option<T>, Gone> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(RequestError::Timeout | RequestError::Device(_)) => Ok(None),
        Err(RequestError::Io(_)) => Err(Gone),
    }
}
