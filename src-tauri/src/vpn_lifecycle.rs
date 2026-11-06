//! Required-VPN transports are pinned before connect and drained before route cleanup.
use crate::{state::Shared, vpn::BoundVpn};
use anyhow::{anyhow, ensure, Context, Result};
use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tauri::AppHandle;
use tokio::{
    net::{TcpSocket, TcpStream},
    sync::watch,
};

#[derive(Default)]
pub(crate) struct Runtime {
    generations: Mutex<HashMap<(String, u64), Arc<Generation>>>,
}
struct Generation {
    status: watch::Sender<(bool, usize)>,
}
impl Default for Generation {
    fn default() -> Self {
        Self {
            status: watch::channel((false, 0)).0,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Lease {
    bound: BoundVpn,
    generation: Arc<Generation>,
}
pub(crate) struct Registration(Arc<Generation>);
impl Drop for Registration {
    fn drop(&mut self) {
        self.0.status.send_modify(|(_, active)| *active -= 1);
    }
}
impl Generation {
    fn register(self: &Arc<Self>) -> Result<Registration> {
        let mut accepted = false;
        self.status.send_modify(|(stopping, active)| {
            if !*stopping {
                *active += 1;
                accepted = true;
            }
        });
        ensure!(accepted, "Required VPN generation is stopping");
        Ok(Registration(self.clone()))
    }
    fn stop(&self) {
        self.status.send_modify(|(stopping, _)| *stopping = true);
    }
    async fn cancelled(&self) {
        let mut status = self.status.subscribe();
        loop {
            if status.borrow_and_update().0 {
                return;
            }
            if status.changed().await.is_err() {
                return;
            }
        }
    }
    async fn drained(&self) -> Result<()> {
        let mut status = self.status.subscribe();
        loop {
            if status.borrow_and_update().1 == 0 {
                return Ok(());
            }
            status.changed().await?;
        }
    }
}
impl Lease {
    pub(crate) fn acquire(app: &AppHandle, state: &Shared, bound: BoundVpn) -> Result<Self> {
        let generation = state
            .vpn
            .lifecycle
            .generations
            .lock()
            .map_err(|_| anyhow!("VPN lifecycle lock"))?
            .entry((bound.id.clone(), bound.generation))
            .or_default()
            .clone();
        ensure!(
            crate::vpn::bound_active(app, state, &bound),
            "Required VPN is no longer connected"
        );
        let lease = Self { bound, generation };
        drop(lease.register()?);
        Ok(lease)
    }
    pub(crate) fn register(&self) -> Result<Registration> {
        self.generation.register()
    }
    pub(crate) fn is_stopping(&self) -> bool {
        self.generation.status.borrow().0
    }
    pub(crate) async fn cancelled(&self) {
        self.generation.cancelled().await;
    }
    pub(crate) async fn run<T>(&self, work: impl Future<Output = Result<T>>) -> Result<T> {
        let _registration = self.register()?;
        tokio::select! {
            biased;
            _ = self.cancelled() => Err(anyhow!("Required VPN disconnected")),
            result = work => result,
        }
    }
    pub(crate) fn spawn(
        &self,
        work: impl Future<Output = ()> + Send + 'static,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let registration = self.register()?;
        let lease = self.clone();
        Ok(tokio::spawn(async move {
            let _registration = registration;
            tokio::select! { biased; _ = lease.cancelled() => {}, _ = work => {} }
        }))
    }
    pub(crate) async fn connect(
        &self,
        app: &AppHandle,
        state: &Shared,
        host: &str,
        port: u16,
    ) -> Result<crate::connections::BoxStream> {
        self.run(async {
            ensure!(
                crate::vpn::bound_active(app, state, &self.bound),
                "Required VPN lease expired before DNS"
            );
            let addresses = tokio::net::lookup_host((host, port))
                .await
                .context("Target DNS lookup failed")?;
            let mut last_error = anyhow!("Target has no address supported by the required VPN");
            for address in addresses {
                if !self
                    .bound
                    .address_families
                    .iter()
                    .any(|family| family == if address.is_ipv4() { "ipv4" } else { "ipv6" })
                {
                    continue;
                }
                ensure!(
                    crate::vpn::bound_active(app, state, &self.bound),
                    "Required VPN lease expired before TCP connect"
                );
                match connect_pinned(&self.bound, address).await {
                    Ok(stream) => {
                        let (client, mut relay) = tokio::io::duplex(64 * 1024);
                        self.spawn(async move {
                            let mut stream = stream;
                            let _ = tokio::io::copy_bidirectional(&mut relay, &mut stream).await;
                        })?;
                        return Ok(Box::new(client) as crate::connections::BoxStream);
                    }
                    Err(error) => last_error = error,
                }
            }
            Err(last_error)
                .context("Required VPN TCP connection failed; no unbound fallback was attempted")
        })
        .await
    }
}
pub(crate) async fn cancelled(lease: &Option<Lease>) {
    if let Some(lease) = lease {
        lease.cancelled().await
    } else {
        std::future::pending().await
    }
}
pub(crate) async fn run<T>(
    lease: &Option<Lease>,
    work: impl Future<Output = Result<T>>,
) -> Result<T> {
    if let Some(lease) = lease {
        lease.run(work).await
    } else {
        work.await
    }
}
pub(crate) async fn quiesce(state: &Shared, id: &str, generation: u64) -> Result<()> {
    let generation = {
        let mut generations = state
            .vpn
            .lifecycle
            .generations
            .lock()
            .map_err(|_| anyhow!("VPN lifecycle lock"))?;
        let entry = generations
            .entry((id.to_string(), generation))
            .or_default()
            .clone();
        // Seal while holding the registry lock, before the first await or new registration.
        entry.stop();
        entry
    };
    generation.drained().await
}

async fn connect_pinned(bound: &BoundVpn, address: SocketAddr) -> Result<TcpStream> {
    let socket = if address.is_ipv4() {
        TcpSocket::new_v4()?
    } else {
        TcpSocket::new_v6()?
    };
    pin_interface(&socket, bound, address.is_ipv4())
        .context("Cannot bind target TCP socket to required VPN interface")?;
    Ok(socket.connect(address).await?)
}
#[cfg(windows)]
fn pin_interface(socket: &TcpSocket, bound: &BoundVpn, ipv4: bool) -> Result<()> {
    use std::os::windows::io::AsRawSocket;
    #[link(name = "ws2_32")]
    unsafe extern "system" {
        fn setsockopt(socket: usize, level: i32, option: i32, value: *const u8, length: i32)
            -> i32;
        fn WSAGetLastError() -> i32;
    }
    ensure!(bound.interface_index > 0, "Invalid VPN interface index");
    // IP_UNICAST_IF takes network byte order; IPV6_UNICAST_IF takes host byte order.
    let index = if ipv4 {
        bound.interface_index.to_be()
    } else {
        bound.interface_index
    };
    let result = unsafe {
        setsockopt(
            socket.as_raw_socket() as usize,
            if ipv4 { 0 } else { 41 },
            31,
            (&index as *const u32).cast(),
            4,
        )
    };
    if result != 0 {
        return Err(std::io::Error::from_raw_os_error(unsafe { WSAGetLastError() }).into());
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn pin_interface(socket: &TcpSocket, bound: &BoundVpn, _ipv4: bool) -> Result<()> {
    use std::os::fd::AsRawFd;
    let name = std::ffi::CString::new(
        bound
            .interface_name
            .as_deref()
            .context("VPN interface name missing")?,
    )?;
    ensure!(
        bound.interface_index > 0
            && unsafe { libc::if_nametoindex(name.as_ptr()) } == bound.interface_index,
        "VPN interface identity changed"
    );
    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_BINDTODEVICE,
            name.as_ptr().cast(),
            name.as_bytes_with_nul().len() as libc::socklen_t,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn pin_interface(socket: &TcpSocket, bound: &BoundVpn, ipv4: bool) -> Result<()> {
    use std::os::fd::AsRawFd;
    ensure!(ipv4, "Required VPN IPv6 binding is unavailable on macOS");
    ensure!(bound.interface_index > 0, "Invalid VPN interface index");
    let index = bound.interface_index;
    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::IPPROTO_IP,
            25, /* IP_BOUND_IF */
            (&index as *const u32).cast(),
            std::mem::size_of_val(&index) as libc::socklen_t,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    fn bound(id: &str, generation: u64) -> BoundVpn {
        BoundVpn {
            id: id.into(),
            generation,
            interface_index: u32::MAX,
            interface_name: Some("tt-invalid0".into()),
            interface_luid: None,
            interface_guid: None,
            address_families: vec!["ipv4".into()],
            addresses: vec![],
            owner_scope: "fixture".into(),
            team_epoch: None,
        }
    }
    pub(crate) fn test_lease(state: &Shared, id: &str, generation: u64) -> Lease {
        let generation_state = state
            .vpn
            .lifecycle
            .generations
            .lock()
            .unwrap()
            .entry((id.into(), generation))
            .or_default()
            .clone();
        Lease {
            bound: bound(id, generation),
            generation: generation_state,
        }
    }
    #[tokio::test]
    async fn stopping_seals_generation_and_waits_for_auth_and_copy_cleanup() {
        let state = Shared::default();
        let lease = test_lease(&state, "fixture", 1);
        let auth = {
            let lease = lease.clone();
            tokio::spawn(async move { lease.run(std::future::pending::<Result<()>>()).await })
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let copy = lease
            .spawn(async move {
                let _listener = listener;
                std::future::pending::<()>().await;
            })
            .unwrap();
        tokio::task::yield_now().await;
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            quiesce(&state, "fixture", 1),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(auth.await.unwrap().is_err());
        copy.await.unwrap();
        assert!(tokio::net::TcpListener::bind(address).await.is_ok());
        assert!(lease.register().is_err());
        assert!(test_lease(&state, "fixture", 2).register().is_ok());
        quiesce(&state, "fixture", 1).await.unwrap();
        assert!(test_lease(&state, "fixture", 2).register().is_ok());
    }
    #[tokio::test]
    async fn quiesce_waits_for_registered_cleanup_before_acknowledging() {
        let state = Shared::default();
        let lease = test_lease(&state, "cleanup", 1);
        let registration = lease.register().unwrap();
        let closing = state.clone();
        let task = tokio::spawn(async move { quiesce(&closing, "cleanup", 1).await });
        tokio::task::yield_now().await;
        assert!(lease.is_stopping());
        assert!(lease.register().is_err());
        assert!(!task.is_finished());
        drop(registration);
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn failed_binding_never_connects_to_the_target() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let socket = TcpSocket::new_v4().unwrap();
        let mut missing_interface = bound("fixture", 1);
        missing_interface.interface_index = 0;
        assert!(pin_interface(&socket, &missing_interface, true).is_err());
        assert!(connect_pinned(&missing_interface, listener.local_addr().unwrap()).await.is_err());
        let result = connect_pinned(&bound("fixture", 1), listener.local_addr().unwrap()).await;
        assert!(result.is_err());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    }
    #[cfg(target_os = "linux")]
    fn namespace_bound(interface: &str) -> (BoundVpn, SocketAddr) {
        assert_ne!(
            std::fs::read_link("/proc/self/ns/net").unwrap(),
            std::fs::read_link("/proc/1/ns/net").unwrap(),
            "Run only in a disposable network namespace"
        );
        assert_eq!(
            std::env::var("TERMTERM_VPN_SOCKET_NAMESPACE_TEST").as_deref(),
            Ok("1")
        );
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let capabilities = status.lines().find_map(|line|line.strip_prefix("CapEff:\t")).unwrap();
        assert_eq!(u64::from_str_radix(capabilities,16).unwrap() & ((1<<12)|(1<<13)),0,"Fixture must drop NET_ADMIN and NET_RAW before exercising sockets");
        let mut lease = bound("namespace", 1);
        lease.interface_name = Some(interface.into());
        let name = std::ffi::CString::new(interface).unwrap();
        lease.interface_index = unsafe { libc::if_nametoindex(name.as_ptr()) };
        assert!(lease.interface_index > 0);
        (lease, "198.18.0.2:28173".parse().unwrap())
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires scripts/test-vpn-bound-sockets.sh disposable namespace fixture"]
    async fn linux_namespace_bound_interface_connects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (lease, address) = namespace_bound("tt-out0");
        let mut stream = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            connect_pinned(&lease, address),
        )
        .await
        .unwrap()
        .unwrap();
        stream.write_all(b"pin").await.unwrap();
        let mut reply = [0; 3];
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            stream.read_exact(&mut reply),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&reply, b"pin");
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires scripts/test-vpn-bound-sockets.sh disposable namespace fixture"]
    async fn linux_namespace_wrong_interface_has_no_fallback() {
        let (lease, address) = namespace_bound("tt-vpn0");
        match tokio::time::timeout(
            std::time::Duration::from_millis(750),
            connect_pinned(&lease, address),
        )
        .await
        {
            Err(_) | Ok(Err(_)) => {}
            Ok(Ok(_)) => {
                panic!("Required VPN socket escaped through the reachable physical interface")
            }
        }
    }
}
