//! Cross-platform framing and transport layer for TaskForest IPC.

pub mod channel;
pub mod endpoint;
pub mod framed;

pub use channel::{TransportListener, TransportStream, bind_transport, connect_transport};
pub use endpoint::IpcEndpoint;
pub use framed::{FramedReader, FramedWriter};
