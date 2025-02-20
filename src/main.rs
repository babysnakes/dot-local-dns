use crate::dns::DnsServer;

mod constants;
mod dns;

fn main() {
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let dns_server = DnsServer::new(53).await?;
            dns_server.run().await
        });
    let _ = dbg!(result);
}
