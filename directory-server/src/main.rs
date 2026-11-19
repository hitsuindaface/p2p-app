use std::io;

use p2p_directory_server::directory;

#[tokio::main]
async fn main() -> io::Result<()> {
    let port = match std::env::args().nth(1) {
        Some(p) => p.parse().map_err(io::Error::other)?,
        None => directory::DEFAULT_PORT,
    };
    println!("directory server on udp {port}");
    tokio::select! {
        r = directory::serve(port) => r,
        _ = shutdown() => Ok(()),
    }
}

// As PID 1 in a container nothing handles SIGTERM unless we do.
async fn shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = term.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
