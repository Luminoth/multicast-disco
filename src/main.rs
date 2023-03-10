mod options;

use std::net::Ipv4Addr;
use std::sync::Arc;

use clap::Parser;
use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use serde::{Deserialize, Serialize};
use tokio::{net::UdpSocket, sync::Semaphore, task::JoinSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConnectionInfo {
    sender: Ipv4Addr,

    host: String,
    port: u16,
    // ...
}

struct Discovery {
    broadcast_interface: Ipv4Addr,
    broadcast_group: Ipv4Addr,
    broadcast_port: u16,

    sync: Arc<Semaphore>,
}

impl Discovery {
    fn new(
        broadcast_interface: Ipv4Addr,
        broadcast_group: Ipv4Addr,
        broadcast_port: u16,
        sync: Arc<Semaphore>,
    ) -> Self {
        Self {
            broadcast_interface,
            broadcast_group,
            broadcast_port,
            sync,
        }
    }

    // server / sender
    async fn broadcast(self, host: String, port: u16) -> anyhow::Result<()> {
        // bind to any port (nothing should be sending to the broadcaster)
        let addr = format!("{}:0", self.broadcast_interface);
        let socket = UdpSocket::bind(&addr).await?;
        println!("Broadcasting on: {}", socket.local_addr()?);

        let socket_ref = socket2::SockRef::from(&socket);
        println!(
            "Default multicast interface: {}",
            socket_ref.multicast_if_v4()?
        );
        socket_ref.set_multicast_if_v4(&self.broadcast_interface)?;

        // join the multicast group (IP_ADD_MEMBERSHIP)
        println!(
            "Joining multicast group {} on interface {}",
            self.broadcast_group, self.broadcast_interface
        );
        socket.join_multicast_v4(self.broadcast_group, self.broadcast_interface)?;

        // increase the multicast TTL (IP_MULTICAST_TTL) so we can go through tunnels
        socket.set_multicast_ttl_v4(32)?;

        let info = serde_json::to_string(&ConnectionInfo {
            sender: self.broadcast_interface,
            host,
            port,
        })?;

        // broadcast our connection info to the multicast group:port
        let broadcast_addr = format!("{}:{}", self.broadcast_group, self.broadcast_port);
        loop {
            println!(
                "Broadcasting connection info to {} on {} ...",
                broadcast_addr, self.broadcast_interface
            );
            socket.send_to(info.as_bytes(), &broadcast_addr).await?;

            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }

    // client / listener
    async fn listen(self) -> anyhow::Result<()> {
        let socket = {
            let _lock = self.sync.acquire().await?;

            // bind to the broadcast port
            let addr = format!("{}:{}", self.broadcast_interface, self.broadcast_port);
            println!("Attempting to listen on {}", addr);
            let socket = UdpSocket::bind(&addr).await?;
            println!("Listening on: {}", socket.local_addr()?);

            socket
        };

        let socket_ref = socket2::SockRef::from(&socket);
        println!(
            "Default multicast interface: {}",
            socket_ref.multicast_if_v4()?
        );
        socket_ref.set_multicast_if_v4(&self.broadcast_interface)?;

        // allow multiple listeners
        socket_ref.set_reuse_address(true)?;
        //socket_ref.set_reuse_port(true)?;
        socket.broadcast()?;

        // join the multicast group (IP_ADD_MEMBERSHIP)
        println!(
            "Joining multicast group {} on interface {}",
            self.broadcast_group, self.broadcast_interface
        );
        socket.join_multicast_v4(self.broadcast_group, self.broadcast_interface)?;

        let mut buf = vec![0; 1024];
        loop {
            let (len, raddr) = socket.recv_from(&mut buf).await?;
            let message = std::str::from_utf8(&buf[..len])?;
            println!(
                "Received {} bytes from {} on {}: {}",
                len, raddr, self.broadcast_interface, message
            );

            let info: ConnectionInfo = serde_json::from_str(message)?;
            println!("Connection Info: {:?}", info);
        }
    }
}

async fn run_client(broadcast_group: Ipv4Addr, broadcast_port: u16) -> anyhow::Result<()> {
    let sync = Arc::new(Semaphore::new(1));

    #[allow(unused_mut)]
    let mut discos = vec![Discovery::new(
        "0.0.0.0".parse().unwrap(),
        broadcast_group,
        broadcast_port,
        sync.clone(),
    )];

    // TODO: this doesn't seem to pick up any of the broadcasts
    /*let network_interfaces = NetworkInterface::show().unwrap();
    for itf in network_interfaces.iter() {
        if let Some(Addr::V4(addr)) = itf.addr {
            // ignore link local addresses
            if addr.ip.is_link_local() {
                continue;
            }

            println!("Discovered interface {} ({})", itf.name, addr.ip);
            discos.push(Discovery::new(
                addr.ip,
                broadcast_group,
                broadcast_port,
                sync.clone(),
            ));
        }
    }*/

    let mut set = JoinSet::new();
    for disco in discos {
        set.spawn(disco.listen());
    }

    while let Some(res) = set.join_next().await {
        res??;
    }

    Ok(())
}

async fn run_server(
    host: String,
    port: u16,
    broadcast_group: Ipv4Addr,
    broadcast_port: u16,
) -> anyhow::Result<()> {
    let mut discos = vec![];

    let network_interfaces = NetworkInterface::show().unwrap();
    for itf in network_interfaces.iter() {
        if let Some(Addr::V4(addr)) = itf.addr {
            // ignore link local addresses
            if addr.ip.is_link_local() {
                continue;
            }

            println!("Discovered interface {} ({})", itf.name, addr.ip);
            discos.push(Discovery::new(
                addr.ip,
                broadcast_group,
                broadcast_port,
                Arc::new(Semaphore::new(1)),
            ));
        }
    }

    let mut set = JoinSet::new();
    for disco in discos {
        set.spawn(disco.broadcast(host.clone(), port));
    }

    while let Some(res) = set.join_next().await {
        res??;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = options::Options::parse();

    match options.command {
        options::Commands::Client {
            broadcast_group,
            broadcast_port,
        } => run_client(broadcast_group, broadcast_port).await?,
        options::Commands::Server {
            host,
            port,
            broadcast_group,
            broadcast_port,
        } => run_server(host, port, broadcast_group, broadcast_port).await?,
    }

    Ok(())
}
