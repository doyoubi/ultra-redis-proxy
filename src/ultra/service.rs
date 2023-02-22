use crate::ultra::io::{IOGroupHandle, new_io_group};
use std::sync::atomic::{AtomicUsize, Ordering};
use dashmap::DashMap;
use std::net::SocketAddr;
use tokio::net::{TcpSocket, TcpStream};
use std::time::Duration;
use crate::common::Config;

pub async fn run_ultra_service(config: Config) {
    tracing::info!("config: {:?}", config);
    let svc = Service::new(config);
    if let Err(err) = svc.run().await {
        tracing::error!("service exited {}", err);
    }
}

pub struct Service {
    config: Config,
    group_id_gen: AtomicUsize,
    groups: DashMap<usize, IOGroupHandle>,
}

impl Service {
    fn new(config: Config) -> Self {
        Self {
            config,
            group_id_gen: AtomicUsize::new(0),
            groups: Default::default(),
        }
    }

    async fn run(&self) -> anyhow::Result<()> {
        let address: SocketAddr = self.config.proxy_address.parse()?;
        let socket = TcpSocket::new_v4()?;
        socket.set_reuseport(true)?;
        socket.set_reuseaddr(true)?;
        socket.bind(address)?;

        let listener = socket.listen(1024)?;
        loop {
            let (sock, client_addr) = listener.accept().await?;
            if let Err(err) = self.add_to_group(sock, client_addr).await {
                tracing::error!("failed to add to group: {}", err);
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }

    async fn add_to_group(&self, mut sock: TcpStream, client_addr: SocketAddr) -> anyhow::Result<()> {
        loop {
            for entry in self.groups.iter() {
                let group_handle = entry.value();
                if group_handle.get_session_count() > self.config.group_sessions {
                    continue;
                }
                if let Err(c) = group_handle.add_session(sock).await {
                    tracing::error!("failed to assign group. try next one.");
                    sock = c;
                    continue;
                }
                tracing::info!("new session {} added to group {}", client_addr, group_handle.get_id());
                return Ok(());
            }

            let group_id = self.group_id_gen.fetch_add(1, Ordering::Relaxed);
            let (group_handle, group) = new_io_group(group_id, self.config.backend_address.clone()).await?;
            self.groups.insert(group_id, group_handle);

            tracing::info!("created a new group {}", group_id);
            tokio::spawn(async move {
                if let Err(err) = group.run().await {
                    tracing::error!("group {} exited {}", group_id, err);
                } else {
                    tracing::info!("group {} exited", group_id);
                }
            });
        }
    }
}
