//! Throwaway Postgres container shared by all tests of one test binary.

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
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
            let c = GenericImage::new("postgres", "17")
                .with_exposed_port(5432.tcp())
                .with_wait_for(WaitFor::message_on_stderr(
                    "database system is ready to accept connections",
                ))
                .with_env_var("POSTGRES_PASSWORD", "test")
                .start()
                .await
                .expect("start postgres container (is Docker running?)");
            let _ = CONTAINER_ID.set(c.id().to_string());
            // SAFETY: registers a plain `extern "C" fn` with no captured state.
            unsafe { libc::atexit(remove_container) };
            let port = host_port(&c).await;
            (c, format!("postgres://postgres:test@127.0.0.1:{port}"))
        })
        .await;
    url
}

/// Docker Desktop can publish port mappings shortly after the container reports ready;
/// retry briefly instead of failing on `PortNotExposed`.
async fn host_port(c: &ContainerAsync<GenericImage>) -> u16 {
    let mut last = None;
    for _ in 0..25 {
        match c.get_host_port_ipv4(5432).await {
            Ok(port) => return port,
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    panic!("pg port not exposed: {last:?}");
}
