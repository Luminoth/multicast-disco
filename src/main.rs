mod options;

use std::net::Ipv4Addr;

use clap::Parser;
use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;

#[derive(Debug, Serialize, Deserialize)]
struct ConnectionInfo {
    host: String,
    port: u16,
    // ...
}

// client / listener
async fn run_client(broadcast_group: Ipv4Addr, broadcast_port: u16) -> anyhow::Result<()> {
    // bind to the broadcast port
    // TODO: does this need to be on a specific interface?
    let addr = format!("0.0.0.0:{}", broadcast_port);
    let socket = UdpSocket::bind(&addr).await?;
    println!("Listening on: {}", socket.local_addr()?);

    // allow multiple listeners
    let socket_ref = socket2::SockRef::from(&socket);
    socket_ref.set_reuse_address(true)?;
    socket.broadcast()?;

    // join the multicast group (IP_ADD_MEMBERSHIP) on every interface
    let network_interfaces = NetworkInterface::show().unwrap();
    for itf in network_interfaces.iter() {
        if let Some(addr) = itf.addr {
            match addr {
                Addr::V4(addr) => {
                    if addr.ip.is_link_local() {
                        continue;
                    }

                    println!(
                        "Joining multicast group {} on interface {}",
                        broadcast_group, addr.ip
                    );
                    socket.join_multicast_v4(broadcast_group, addr.ip)?;
                }
                _ => (),
            }
        }
    }

    let mut buf = vec![0; 1024];
    loop {
        let (len, raddr) = socket.recv_from(&mut buf).await?;
        let message = std::str::from_utf8(&buf[..len])?;
        println!("Received {} bytes from {}: {}", len, raddr, message);

        let info: ConnectionInfo = serde_json::from_str(message)?;
        println!("Connection Info: {:?}", info);

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

// server / sender
async fn run_server(
    host: String,
    port: u16,
    broadcast_group: Ipv4Addr,
    broadcast_port: u16,
) -> anyhow::Result<()> {
    // bind to any port (nothing should be sending to the server)
    // TODO: does this need to be on a specific interface?
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    println!("Broadcasting on: {}", socket.local_addr()?);

    // join the multicast group (IP_ADD_MEMBERSHIP) on every interface
    let network_interfaces = NetworkInterface::show().unwrap();
    for itf in network_interfaces.iter() {
        if let Some(addr) = itf.addr {
            match addr {
                Addr::V4(addr) => {
                    if addr.ip.is_link_local() {
                        continue;
                    }

                    println!(
                        "Joining multicast group {} on interface {}",
                        broadcast_group, addr.ip
                    );
                    socket.join_multicast_v4(broadcast_group, addr.ip)?;
                }
                _ => (),
            }
        }
    }

    // increase the multicast TTL so we can go through tunnels
    socket.set_multicast_ttl_v4(8)?;

    // broadcast our connection info to the multicast group:port
    let info = serde_json::to_string(&ConnectionInfo { host, port })?;
    let broadcast_addr = format!("{}:{}", broadcast_group, broadcast_port);
    loop {
        println!("Broadcasting connection info to {} ...", broadcast_addr);
        socket.send_to(info.as_bytes(), &broadcast_addr).await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
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
