//! Binary wire protocol definitions and frame serialization.

pub mod error;
pub mod frame;
pub mod mask;
pub mod message;

pub use error::{IpcError, IpcResult};
pub use frame::{FRAME_MAGIC, Frame, FrameHeader, HEADER_LEN, MAX_PAYLOAD_LEN, PROTOCOL_VERSION};
pub use mask::SubscriptionMask;
pub use message::{ClientMessage, DaemonMessage, Opcode};
