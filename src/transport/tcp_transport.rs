//! Process-wide TCP transport configuration.

use std::io;
use tokio::net::TcpStream;

/// Disables Nagle buffering on accepted HTTP and WebSocket connections.
pub fn configure_low_latency_tcp(stream: &mut TcpStream) -> io::Result<()> {
    stream.set_nodelay(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn accepted_stream_uses_tcp_nodelay() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local address");
        let client = TcpStream::connect(address);
        let server = listener.accept();
        let (_client, server) = tokio::join!(client, server);
        let (mut server, _) = server.expect("accepted stream");

        configure_low_latency_tcp(&mut server).expect("configure stream");

        assert!(server.nodelay().expect("read TCP_NODELAY"));
    }
}
