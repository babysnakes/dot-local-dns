use clap::Parser;
use std::io;
use tokio::net::UdpSocket;

/// Send UDP packets with the specified size to the default development port
#[derive(Parser)]
struct Args {
    /// Number of packets to send
    count: usize,
    /// Size of each packet in bytes
    #[arg(default_value = "1")]
    packet_size: usize,
}

async fn send_multiple_udp_packets(count: usize, packet_size: usize) -> io::Result<()> {
    // Create a UDP socket bound to any available port on localhost
    let socket = UdpSocket::bind("127.0.0.1:0").await?;

    // Create a buffer filled with ones
    let data = vec![1u8; packet_size];

    // Send 'count' number of packets
    for _i in 0..count {
        let _bytes_sent = socket.send_to(&data, "127.0.0.1:2053").await?;
    }

    println!(
        "Successfully sent {} packets of {} bytes each to 127.0.0.1:2053",
        count, packet_size
    );
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();

    send_multiple_udp_packets(args.count, args.packet_size).await?;
    Ok(())
}
