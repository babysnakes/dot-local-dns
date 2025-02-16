mod dns;

fn main() {
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(dns::dns_server(53));
    let _ = dbg!(result);
}
