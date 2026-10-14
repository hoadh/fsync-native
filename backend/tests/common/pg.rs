//! Throwaway Postgres container shared by all tests of one test binary.

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, TestcontainersError,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tokio::sync::OnceCell;

/// Container id for the atexit cleanup (testcontainers-rs has no reaper; statics are never dropped).
static CONTAINER_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

extern "C" fn remove_container() {
    if let Some(id) = CONTAINER_ID.get() {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", "-v", id])
            .stdout(std::process::Stdio::null())
            .status();
    }
}

type Pg = (ContainerAsync<GenericImage>, String);
static PG: OnceCell<Pg> = OnceCell::const_new();

pub async fn admin_url() -> &'static str {
    let (_c, url) = PG
        .get_or_init(|| async {
            let (c, port) = start_with_port().await;
            let _ = CONTAINER_ID.set(c.id().to_string());
            // SAFETY: registers a plain `extern "C" fn` with no captured state.
            unsafe { libc::atexit(remove_container) };
            (c, format!("postgres://postgres:test@127.0.0.1:{port}"))
        })
        .await;
    url
}

async fn start() -> ContainerAsync<GenericImage> {
    GenericImage::new("postgres", "17")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_PASSWORD", "test")
        .start()
        .await
        .expect("start postgres container (is Docker running?)")
}

/// Root cause of the old `PortNotExposed` flake (reproduced 1 in ~20 starts while other builds
/// load Docker Desktop): the container runs and postgres is ready, but Docker never published
/// the port (`inspect` shows `"5432/tcp": []` for good). Waiting longer does not help, so a
/// container without a mapping is thrown away and a fresh one is started. Any other error
/// fails immediately.
pub async fn start_with_port() -> (ContainerAsync<GenericImage>, u16) {
    const ATTEMPTS: u32 = 4;
    for attempt in 1..=ATTEMPTS {
        let c = start().await;
        // The mapping is normally there at once; allow a short grace for a slow daemon.
        for _ in 0..10 {
            match c.get_host_port_ipv4(5432).await {
                Ok(port) => return (c, port),
                Err(TestcontainersError::PortNotExposed { .. }) => {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
                Err(e) => panic!("pg port lookup failed: {e:?}"),
            }
        }
        eprintln!(
            "postgres container has no published port (attempt {attempt}/{ATTEMPTS}), restarting"
        );
        let _ = c.rm().await;
    }
    panic!("docker never published the postgres port in {ATTEMPTS} attempts");
}
