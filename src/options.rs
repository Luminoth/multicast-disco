use std::net::Ipv4Addr;

use clap::{Parser, Subcommand};

#[derive(Debug, PartialEq, Eq, Subcommand)]
#[command(author, version, about, long_about = None)]
pub enum Commands {
    Client {
        #[arg(long, default_value = "239.0.0.123")]
        broadcast_group: Ipv4Addr,

        #[arg(long, default_value_t = 6772)]
        broadcast_port: u16,
    },
    Server {
        #[arg(long, default_value = "localhost")]
        host: String,

        #[arg(short, long, default_value_t = 1234)]
        port: u16,

        #[arg(long, default_value = "239.0.0.123")]
        broadcast_group: Ipv4Addr,

        #[arg(long, default_value_t = 6772)]
        broadcast_port: u16,
    },
}

#[derive(Debug, Parser)]
#[clap(name = "multicaster")]
pub struct Options {
    #[clap(subcommand)]
    pub command: Commands,
}
