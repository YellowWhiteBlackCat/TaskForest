//! Platform-independent transport traits and connection factory.

use std::io::{self, Read, Write};

use super::endpoint::IpcEndpoint;

/// Unified trait representing a bi-directional, thread-safe, and unbounded byte stream.
pub trait TransportStream: Read + Write + Send + 'static {}

impl<T: Read + Write + Send + 'static> TransportStream for T {}

/// Unified trait representing an incoming connection listener for IPC servers.
pub trait TransportListener: Send + 'static {
    /// Accept the next incoming client connection stream non-blockingly or blockingly.
    fn accept(&self) -> io::Result<Box<dyn TransportStream>>;
}

#[cfg(not(windows))]
struct UnixListenerAdapter(std::os::unix::net::UnixListener);

#[cfg(not(windows))]
impl TransportListener for UnixListenerAdapter {
    fn accept(&self) -> io::Result<Box<dyn TransportStream>> {
        let (stream, _) = self.0.accept()?;
        Ok(Box::new(stream))
    }
}

/// Establish an active client connection to the designated IPC endpoint.
pub fn connect_transport(endpoint: &IpcEndpoint) -> io::Result<Box<dyn TransportStream>> {
    match endpoint {
        #[cfg(not(windows))]
        IpcEndpoint::Unix(path) => {
            let stream = std::os::unix::net::UnixStream::connect(path)?;
            Ok(Box::new(stream))
        }
        #[cfg(windows)]
        IpcEndpoint::Windows(pipe_name) => {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(pipe_name)?;
            Ok(Box::new(file))
        }
        #[cfg(windows)]
        IpcEndpoint::Unix(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Unix Domain Sockets unsupported on this Windows target",
        )),
        #[cfg(not(windows))]
        IpcEndpoint::Windows(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows Named Pipes unsupported on Unix targets",
        )),
    }
}

/// Bind a listening server transport to the designated IPC endpoint.
pub fn bind_transport(endpoint: &IpcEndpoint) -> io::Result<Box<dyn TransportListener>> {
    match endpoint {
        #[cfg(not(windows))]
        IpcEndpoint::Unix(path) => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::remove_file(path);
            let listener = std::os::unix::net::UnixListener::bind(path)?;
            Ok(Box::new(UnixListenerAdapter(listener)))
        }
        #[cfg(windows)]
        IpcEndpoint::Windows(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows Named Pipe server listening requires SCM or dedicated overlapped pipe handle",
        )),
        #[cfg(windows)]
        IpcEndpoint::Unix(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Unix Domain Sockets unsupported on Windows",
        )),
        #[cfg(not(windows))]
        IpcEndpoint::Windows(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows Named Pipes unsupported on Unix",
        )),
    }
}
