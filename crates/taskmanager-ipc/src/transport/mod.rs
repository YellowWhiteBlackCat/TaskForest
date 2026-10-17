//! Cross-platform framing and transport layer for TaskForest IPC.

pub mod endpoint;
pub mod framed;

pub use endpoint::IpcEndpoint;
pub use framed::{FramedReader, FramedWriter};
