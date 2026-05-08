use anyhow::Result;
use ix_transport::LocalEndpoint;
use tokio::net::UnixListener;

#[allow(dead_code)]
pub struct LocalServer {
    pub endpoint: LocalEndpoint,
}

impl LocalServer {
    #[cfg(unix)]
    pub fn bind(endpoint: LocalEndpoint) -> Result<UnixListener> {
        let path = endpoint.as_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(UnixListener::bind(path)?)
    }

    #[cfg(windows)]
    pub fn bind(
        endpoint: LocalEndpoint,
    ) -> Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        use tokio::net::windows::named_pipe::ServerOptions;
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&endpoint.0)?;
        Ok(server)
    }
}
