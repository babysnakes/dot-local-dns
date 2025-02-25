use crate::dns::LookupChannel::ARecordQuery;
use crate::dns::{DnsServer, LookupChannel, Notification};
use anyhow::Result;
use dns::Notification::*;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tokio::{io, join};

mod constants;
mod dns;

async fn console_loop(
    lookup_tx: Sender<LookupChannel>,
    notify_tx: Sender<Notification>,
) -> Result<()> {
    let stdin = io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    println!("print commands: reload or shutdown. Other strings will be interpreted as hostname to resolve...");

    while let Ok(Some(line)) = lines.next_line().await {
        if line.to_lowercase() == "shutdown" {
            notify_tx.send(Shutdown).await?;
            println!("shutdown complete");
            break;
        } else if line.to_lowercase() == "reload" {
            notify_tx.send(Reload).await?;
        } else {
            let (tx, rx) = oneshot::channel();
            let _ = lookup_tx.send(ARecordQuery(line, tx)).await;
            let ip = rx.await?;
            println!("lookup returned {ip:?}");
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut dns_server = DnsServer::new(53).await?;
    let lookup_tx = dns_server.lookup_tx.clone();
    let notify_tx = dns_server.notify_tx.clone();
    let (sout, dout) = join!(
        dns_server.run(),
        console_loop(lookup_tx.clone(), notify_tx.clone())
    );
    let _ = dbg!(sout, dout);
    Ok(())
}
