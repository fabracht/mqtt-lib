pub mod manager;
#[cfg(test)]
pub mod mock;
pub mod packet_io;
pub mod tcp;
pub mod tls;

use crate::error::Result;

pub use manager::{ConnectionState, ConnectionStats, ManagerConfig, TransportManager};
pub use packet_io::{PacketIo, PacketReader, PacketWriter};
pub use tcp::{TcpConfig, TcpTransport};
pub use tls::{TlsConfig, TlsTransport};

pub trait Transport: Send + Sync {
    /// Establishes a connection
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be established
    fn connect(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Reads data into the provided buffer
    ///
    /// # Errors
    ///
    /// Returns an error if the read operation fails
    fn read(&mut self, buf: &mut [u8]) -> impl std::future::Future<Output = Result<usize>> + Send;

    /// Writes data from the provided buffer
    ///
    /// # Errors
    ///
    /// Returns an error if the write operation fails
    fn write(&mut self, buf: &[u8]) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Closes the connection
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be closed cleanly
    fn close(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Enum for different transport types
pub enum TransportType {
    Tcp(TcpTransport),
    Tls(Box<TlsTransport>),
}

impl Transport for TransportType {
    fn connect(&mut self) -> impl std::future::Future<Output = Result<()>> + Send {
        async move {
            match self {
                Self::Tcp(t) => t.connect().await,
                Self::Tls(t) => t.connect().await,
            }
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> impl std::future::Future<Output = Result<usize>> + Send {
        async move {
            match self {
                Self::Tcp(t) => t.read(buf).await,
                Self::Tls(t) => t.read(buf).await,
            }
        }
    }

    fn write(&mut self, buf: &[u8]) -> impl std::future::Future<Output = Result<()>> + Send {
        async move {
            match self {
                Self::Tcp(t) => t.write(buf).await,
                Self::Tls(t) => t.write(buf).await,
            }
        }
    }

    fn close(&mut self) -> impl std::future::Future<Output = Result<()>> + Send {
        async move {
            match self {
                Self::Tcp(t) => t.close().await,
                Self::Tls(t) => t.close().await,
            }
        }
    }
}
