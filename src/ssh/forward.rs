//! Forwarded unix socket RAII guard — the bollard bridge B2 consumes.
//!
//! [`ForwardedSocket`] opens a local→remote unix-socket forward over an SSH
//! session, secures it to 0600 before handing the path out, and removes the
//! socket on drop or explicit close.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use openssh::{ForwardType, Session, Socket};

use crate::synapse::HostConfig;

use super::REMOTE_DOCKER_SOCKET;

/// Local path for a host's forwarded docker socket: `/tmp/synapse2-{host}-{pid}.sock`.
///
/// Host names may contain hyphens, so the startup sweep parses the pid from the
/// RIGHT (`rsplit_once('-')`). Keep this format in sync with [`sweep_stale_sockets`].
pub fn forward_socket_path(host: &HostConfig) -> PathBuf {
    PathBuf::from(format!(
        "/tmp/synapse2-{}-{}.sock",
        host.name,
        std::process::id()
    ))
}

/// An RAII guard over a forwarded unix socket. On drop it removes the socket
/// file (sync, best-effort) and detaches an async `close_port_forward`. Prefer
/// the explicit [`ForwardedSocket::close`] for the happy path.
pub struct ForwardedSocket {
    pub(super) session: Arc<Session>,
    pub(super) local_path: PathBuf,
    pub(super) closed: bool,
}

impl ForwardedSocket {
    /// Open a local→remote unix-socket forward bridging `local_path` to the
    /// remote docker socket. The socket is chmod 0600 before this returns so it
    /// is never world-connectable.
    pub async fn open(session: Arc<Session>, local_path: PathBuf) -> Result<Self> {
        // Remove any leftover socket at this path (a prior run with our pid that
        // crashed) so the bind does not silently fail.
        if local_path.exists() {
            let _ = std::fs::remove_file(&local_path);
        }

        session
            .request_port_forward(
                ForwardType::Local,
                Socket::UnixSocket {
                    path: local_path.as_path().into(),
                },
                Socket::UnixSocket {
                    path: Path::new(REMOTE_DOCKER_SOCKET).into(),
                },
            )
            .await
            .with_context(|| format!("request unix-socket forward at {}", local_path.display()))?;

        // native-mux may create the listener slightly after the request returns.
        // Poll briefly, then lock it down to 0600 BEFORE handing the path out.
        secure_socket(&local_path).await?;

        Ok(Self {
            session,
            local_path,
            closed: false,
        })
    }

    /// Local socket path to hand to a bollard client (`unix://{path}`).
    pub fn path(&self) -> &Path {
        &self.local_path
    }

    /// Explicit async teardown — preferred over relying on `Drop`.
    pub async fn close(mut self) -> Result<()> {
        self.closed = true;
        let result = self
            .session
            .close_port_forward(
                ForwardType::Local,
                Socket::UnixSocket {
                    path: self.local_path.as_path().into(),
                },
                Socket::UnixSocket {
                    path: Path::new(REMOTE_DOCKER_SOCKET).into(),
                },
            )
            .await;
        let _ = std::fs::remove_file(&self.local_path);
        result.with_context(|| format!("close port forward {}", self.local_path.display()))
    }
}

impl Drop for ForwardedSocket {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        // Sync best-effort file removal — Drop cannot await.
        let _ = std::fs::remove_file(&self.local_path);

        // Detach the async port-forward teardown if a runtime is available.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let session = Arc::clone(&self.session);
            let path = self.local_path.clone();
            handle.spawn(async move {
                let _ = session
                    .close_port_forward(
                        ForwardType::Local,
                        Socket::UnixSocket {
                            path: path.as_path().into(),
                        },
                        Socket::UnixSocket {
                            path: Path::new(REMOTE_DOCKER_SOCKET).into(),
                        },
                    )
                    .await;
            });
        }
    }
}

/// Wait (briefly) for the socket to appear, then chmod 0600.
///
/// SECURITY (security-sentinel, MEDIUM): a world-readable socket would let
/// other local users connect to the remote docker daemon. The 0600 must
/// land before the path is exposed to bollard.
///
/// Uses `tokio::fs::set_permissions` (non-blocking) so this async function
/// does not park a Tokio worker thread on a blocking syscall (P-L6).
pub(super) async fn secure_socket(local_path: &Path) -> Result<()> {
    const MAX_WAIT: Duration = Duration::from_secs(2);
    const POLL: Duration = Duration::from_millis(20);
    let deadline = Instant::now() + MAX_WAIT;
    loop {
        if local_path.exists() {
            tokio::fs::set_permissions(local_path, std::fs::Permissions::from_mode(0o600))
                .await
                .with_context(|| format!("chmod 0600 forwarded socket {}", local_path.display()))?;
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!(
                "forwarded socket {} did not appear within {}ms",
                local_path.display(),
                MAX_WAIT.as_millis()
            );
        }
        tokio::time::sleep(POLL).await;
    }
}
