//! Platform-independent transport traits and connection factory.

use std::io::{self, Read, Write};

#[cfg(windows)]
use taskmanager_windows_api::NamedPipeListener;

use super::endpoint::IpcEndpoint;

/// Unified trait representing a bi-directional, thread-safe, and unbounded byte stream.
pub trait TransportStream: Read + Write + Send + 'static {
    /// Attempt to duplicate the stream handle for independent concurrent read and write operations.
    fn try_clone(&self) -> io::Result<Box<dyn TransportStream>>;
}

impl TransportStream for Box<dyn TransportStream> {
    fn try_clone(&self) -> io::Result<Box<dyn TransportStream>> {
        (**self).try_clone()
    }
}

#[cfg(not(windows))]
impl TransportStream for std::os::unix::net::UnixStream {
    fn try_clone(&self) -> io::Result<Box<dyn TransportStream>> {
        let cloned = self.try_clone()?;
        Ok(Box::new(cloned))
    }
}

#[cfg(windows)]
impl TransportStream for std::fs::File {
    fn try_clone(&self) -> io::Result<Box<dyn TransportStream>> {
        let cloned = self.try_clone()?;
        Ok(Box::new(cloned))
    }
}

impl TransportStream for std::net::TcpStream {
    fn try_clone(&self) -> io::Result<Box<dyn TransportStream>> {
        let cloned = self.try_clone()?;
        Ok(Box::new(cloned))
    }
}

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

#[cfg(windows)]
struct WindowsNamedPipeListenerAdapter(NamedPipeListener);

#[cfg(windows)]
impl TransportListener for WindowsNamedPipeListenerAdapter {
    fn accept(&self) -> io::Result<Box<dyn TransportStream>> {
        let file = self.0.accept()?;
        Ok(Box::new(file))
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
        IpcEndpoint::Windows(pipe_name) => {
            let listener = NamedPipeListener::bind(pipe_name).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Named pipe bind failed: {e:?}"),
                )
            })?;
            Ok(Box::new(WindowsNamedPipeListenerAdapter(listener)))
        }
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
