#![allow(clippy::wildcard_imports)]

mod protocol;
mod records;

use super::shared::*;
use anyhow::{anyhow, Result};
use failsafe::futures::CircuitBreaker;
use failsafe::Config;
use log::{debug, error, info, trace, warn};
use protocol::*;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use tokio::net::UdpSocket;
use tokio::select;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::oneshot;
use windows_sys::Win32::Foundation::{BOOL, FALSE};

pub struct DnsServer {
    pub notify_tx: Sender<Notification>,
    #[allow(dead_code)] // todo: clear after actually using.
    pub lookup_tx: Sender<LookupChannel>,
    port: u16,
    db_path: PathBuf,
    records: HashMap<String, Ipv4Addr>,
    notify_rx: Receiver<Notification>,
    lookup_rx: Receiver<LookupChannel>,
}

#[derive(Debug)]
pub enum LookupChannel {
    #[allow(dead_code)] // todo: clear after actually using.
    ARecordQuery(String, oneshot::Sender<Result<Ipv4Addr>>),
}

#[derive(Debug)]
pub enum Notification {
    Shutdown,
    Reload,
}

impl DnsServer {
    pub async fn new(port: u16, db_path: Option<PathBuf>) -> Result<Self> {
        let db_path = db_path.unwrap_or(records::default_db_path()?);
        let records = records::try_from_file(&db_path).await?;
        let (notify_tx, notify_rx) = mpsc::channel::<Notification>(1);
        let (lookup_tx, lookup_rx) = mpsc::channel::<LookupChannel>(4);
        Ok(Self {
            notify_tx,
            lookup_tx,
            port,
            db_path,
            records,
            notify_rx,
            lookup_rx,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let socket = mk_udp_socket(&addr).await?;
        info!("Listening on: localhost:{}", self.port);
        let circuit_breaker = Config::new().build();
        loop {
            let mut req_buffer = BytePacketBuffer::new();
            select! {
                biased;
                notification = self.notify_rx.recv() => {
                    debug!("DNS server received notification: {notification:?}");
                    if let Some(notification) = notification {
                        match notification {
                            Notification::Shutdown => {
                                info!("DNS server received shutdown");
                                return Ok(());
                            },
                            Notification::Reload => {
                                info!("Reloading Records");
                                self.reload_records().await.unwrap_or_else(|e| {
                                    let path = &self.db_path.to_string_lossy();
                                    notify_error!("Error reloading records file ({path}): {e}");
                                });
                            }
                        }
                    }
                }
                message = self.lookup_rx.recv() => {
                    self.handle_name_lookup(message);
                }
                received = socket.recv_from(&mut req_buffer.buf) => {
                    let handler = self.handle_request(received, &mut req_buffer, &socket);
                    match circuit_breaker.call(handler).await {
                        Ok(()) => {},
                        Err(failsafe::Error::Inner(e)) => {
                            error!("DNS server error: {e}");
                        },
                        Err(failsafe::Error::Rejected) => {
                            error!("Circuit breaker rejected");
                            return Err(anyhow!("Multiple Errors on DNS Server! Quitting! Check the logs!"));
                        }
                    }
                }
            }
        }
    }

    async fn reload_records(&mut self) -> Result<()> {
        let records = records::load_from_file(&self.db_path).await?;
        self.records = records;
        info!("Records reloaded");
        Ok(())
    }

    #[allow(clippy::similar_names)]
    async fn handle_request(
        &mut self,
        received: std::io::Result<(usize, SocketAddr)>,
        req_buffer: &mut BytePacketBuffer,
        socket: &UdpSocket,
    ) -> Result<()> {
        let (_len, peer) = received?;
        let request = DnsPacket::from_buffer(req_buffer).await?;
        let mut response = lookup(&request, &self.records);
        let mut res_buffer = BytePacketBuffer::new();
        response.write(&mut res_buffer)?;
        let pos = res_buffer.pos();
        let data = res_buffer.get_range(0, pos)?;
        socket.send_to(data, peer).await?;
        Ok(())
    }

    fn handle_name_lookup(&self, message: Option<LookupChannel>) {
        debug!("DNS server received lookup channel: {message:?}");
        if let Some(LookupChannel::ARecordQuery(host, tx)) = message {
            let res = lookup_name(host, &self.records);
            if tx.send(res).is_err() {
                error!("Error sending response to lookup channel");
            }
        }
    }
}

fn lookup(request: &DnsPacket, domain: &HashMap<String, Ipv4Addr>) -> DnsPacket {
    let id = &request.header.id;
    trace!("received query (id: {}): {:?}", &id, &request);
    let mut response = DnsPacket::new();
    response.header.response = true;
    response.header.id = *id;
    response.header.recursion_desired = request.header.recursion_desired;

    if request.questions.is_empty() {
        response.header.rescode = ResultCode::NOTIMP;
        return response;
    }

    let query = &request.questions[0];
    response.questions.push(query.clone());

    if request.header.response {
        warn!("received response as question (id: {})", &id);
        response.header.rescode = ResultCode::NOTIMP;
        return response;
    }

    if request.header.opcode != 0 {
        warn!("received non-zero opcode (id: {})", &id);
        response.header.rescode = ResultCode::NOTIMP;
        return response;
    }

    if !query.name.ends_with(TOP_LEVEL_DOMAIN) {
        warn!("unsupported domain (id: {}): {}", &id, &query.name);
        response.header.rescode = ResultCode::SERVFAIL;
        return response;
    }

    match &query.qtype {
        QueryType::A => {
            let record = DnsRecord::A {
                addr: ip_from_domain_or_default(&query.name, domain),
                domain: query.name.to_string(),
                ttl: 0,
            };
            response.answers.push(record);
        }
        QueryType::AAAA | QueryType::CNAME | QueryType::MX | QueryType::NS | QueryType::SOA => {
            debug!("received request for undefined query type: {:?}", &query);
            response.header.rescode = ResultCode::NOERROR;
        }
        QueryType::UNKNOWN(x) => {
            warn!("received query of unsupported type ({}): {:?}", x, &query);
            response.header.rescode = ResultCode::SERVFAIL;
        }
    }
    debug!("response is: {:#?}", &response);
    response
}

fn lookup_name(host: String, domain: &HashMap<String, Ipv4Addr>) -> Result<Ipv4Addr> {
    let mut query = DnsPacket::new();
    let question = DnsQuestion::new(host, QueryType::A);
    query.questions.push(question);
    let response = lookup(&query, domain);
    let result_code = ResultCode::from_num(response.header.opcode);
    if response.answers.is_empty() {
        return Err(anyhow!(
            "DNS responded with no answers and code: {result_code:?}"
        ));
    }
    match response.answers.first() {
        Some(DnsRecord::A { ref addr, .. }) => Ok(*addr),
        _ => Err(anyhow!("DNS responded with")),
    }
}

fn ip_from_domain_or_default(host: &str, domain: &HashMap<String, Ipv4Addr>) -> Ipv4Addr {
    domain
        .iter()
        .find(|&(name, _)| name == host || host.ends_with(&format!(".{name}")))
        .map_or(Ipv4Addr::LOCALHOST, |(_, ip)| *ip)
}

#[allow(clippy::cast_possible_truncation)]
#[cfg(windows)]
async fn mk_udp_socket(addr: &SocketAddr) -> std::io::Result<UdpSocket> {
    use std::io::Error;
    use std::os::windows::io::AsRawSocket;
    use std::ptr::null_mut;
    use windows_sys::Win32::Networking::WinSock::{WSAIoctl, SIO_UDP_CONNRESET, SOCKET};

    let socket = UdpSocket::bind(addr).await?;
    let handle = socket.as_raw_socket() as SOCKET;
    let mut enable: BOOL = FALSE;
    let mut bytes_returned: u32 = 0;
    let result = unsafe {
        WSAIoctl(
            handle,
            SIO_UDP_CONNRESET,
            std::ptr::from_mut(&mut enable) as _,
            size_of_val(&enable) as _,
            null_mut(),
            0,
            &mut bytes_returned,
            null_mut(),
            None,
        )
    };
    if result != 0 {
        return Err(Error::last_os_error());
    }

    Ok(socket)
}

#[allow(clippy::match_on_vec_items)]
#[cfg(test)]
mod tests {
    use super::protocol::*;
    use super::{lookup, DnsServer};
    use crate::dns::LookupChannel::ARecordQuery;
    use crate::dns::Notification::{Reload, Shutdown};
    use std::collections::HashMap;
    use std::net::Ipv4Addr;
    use tokio::join;
    use tokio::sync::oneshot;
    use tokio::time::{sleep, timeout, Duration};

    macro_rules! lookup_tests {
        ($name:ident, $query_packet:expr, $response_code:expr, $extra_tests:expr) => {
            #[test]
            fn $name() {
                let response = lookup($query_packet, &records());
                // a few common tests
                assert_eq!($query_packet.header.id, response.header.id);
                assert_eq!(response.header.rescode, $response_code);
                // provided test function
                $extra_tests(&response);
            }
        };
    }

    lookup_tests! {
        normal_dns_request,
        {
            let mut packet = packet_with_question("hello.local".to_string(), QueryType::A);
            packet.header.recursion_desired = true;
            &packet.clone()
        },
        ResultCode::NOERROR,
        |response: &DnsPacket| {
            assert!(response.header.recursion_desired);
            assert_eq!(
                response.questions[0].name, "hello.local",
                "response question's name doesn't match original name"
            );
            assert_eq!(
                response.answers[0],
                DnsRecord::A {
                    domain: "hello.local".to_string(),
                    addr: Ipv4Addr::LOCALHOST,
                    ttl: 0
                }
            );
        }
    }

    lookup_tests! {
        subdomain_a_requests_are_supported,
        &packet_with_question("sub.domain.local".to_string(), QueryType::A),
        ResultCode::NOERROR,
        |response: &DnsPacket| {
            match response.answers[0] {
                DnsRecord::A { ref domain, ref addr, .. } => {
                    assert_eq!(domain, "sub.domain.local");
                    assert_eq!(*addr, Ipv4Addr::LOCALHOST);
                }
                _ => panic!("Did not receive DnsRecord::A (received {:?}", response.answers[0]),
            }
        }
    }

    lookup_tests! {
        query_of_existing_record_returns_the_record,
        &packet_with_question("registered.local".to_string(), QueryType::A),
        ResultCode::NOERROR,
        |response: &DnsPacket| {
            match response.answers[0] {
                DnsRecord::A { ref domain, ref addr, .. } => {
                    assert_eq!(domain, "registered.local");
                    assert_eq!(*addr, "192.168.0.1".parse::<Ipv4Addr>().unwrap());
                }
                _ => panic!("Did not receive DnsRecord::A (received {:?}", response.answers[0]),
            }
        }
    }

    lookup_tests! {
        query_subdomain_of_existing_record_returns_the_record,
        &packet_with_question("sub.registered.local".to_string(), QueryType::A),
        ResultCode::NOERROR,
        |response: &DnsPacket| {
            match response.answers[0] {
                DnsRecord::A { ref domain, ref addr, .. } => {
                    assert_eq!(domain, "sub.registered.local");
                    assert_eq!(*addr, "192.168.0.1".parse::<Ipv4Addr>().unwrap());
                }
                _ => panic!("Did not receive DnsRecord::A (received {:?}", response.answers[0]),
            }
        }
    }

    lookup_tests! {
        query_name_that_ends_with_existing_record_returns_localhost,
        &packet_with_question("not-registered.local".to_string(), QueryType::A),
        ResultCode::NOERROR,
        |response: &DnsPacket| {
            match response.answers[0] {
                DnsRecord::A { ref domain, ref addr, .. } => {
                    assert_eq!(domain, "not-registered.local");
                    assert_eq!(*addr, Ipv4Addr::LOCALHOST);
                }
                _ => panic!("Did not receive DnsRecord::A (received {:?}", response.answers[0]),
            }
        }
    }

    lookup_tests! {
        soa_requests_return_no_error_and_zero_answers,
        &packet_with_question("test.local".to_string(), QueryType::SOA),
        ResultCode::NOERROR,
        |response: &DnsPacket| {
            assert_eq!(response.answers.len(), 0);
        }
    }

    lookup_tests! {
        ns_requests_return_no_error_and_zero_answers,
        &packet_with_question("test.local".to_string(), QueryType::NS),
        ResultCode::NOERROR,
        |response: &DnsPacket| {
            assert_eq!(response.answers.len(), 0);
        }
    }

    lookup_tests! {
        packets_with_no_queries_are_not_implemented,
        {
            let mut packet = DnsPacket::new();
            packet.header.id = 1234;
            &packet.clone()
        },
        ResultCode::NOTIMP,
        |_| {}
    }

    lookup_tests! {
        response_packets_are_not_supported,
        {
            let mut packet = packet_with_question("test.local".to_string(), QueryType::A);
            packet.header.response = true;
            &packet.clone()
        },
        ResultCode::NOTIMP,
        |_| {}
    }

    lookup_tests! {
        non_zero_opcode_are_not_supported,
        {
            let mut packet = packet_with_question("test.local".to_string(), QueryType::A);
            packet.header.opcode = 1;
            &packet.clone()
        },
        ResultCode::NOTIMP,
        |_| {}
    }

    lookup_tests! {
        does_not_accept_wrong_domain,
        &packet_with_question("example.com".to_string(), QueryType::A),
        ResultCode::SERVFAIL,
        |response: &DnsPacket| {
            assert_eq!(response.answers.len(), 0);
        }
    }

    fn records() -> HashMap<String, Ipv4Addr> {
        HashMap::from([("registered.local".into(), "192.168.0.1".parse().unwrap())])
    }

    fn packet_with_question(name: String, query_type: QueryType) -> DnsPacket {
        let mut packet = DnsPacket::new();
        packet.header.id = 10;
        packet.questions.push(DnsQuestion::new(name, query_type));
        packet.clone()
    }

    #[tokio::test]
    async fn service_starts_with_no_db_file() {
        let mut dns = DnsServer::new(0, Some("non-existent-file".into()))
            .await
            .unwrap();
        let notify_tx = dns.notify_tx.clone();
        let ((), dns_out) = join!(
            async move {
                sleep(Duration::from_millis(100)).await;
                _ = notify_tx.send(Shutdown).await;
            },
            dns.run(),
        );
        dns_out.unwrap(); // assert did not return error.
    }

    #[tokio::test]
    async fn reloading_records_fails_if_db_file_is_missing() {
        timeout(Duration::from_millis(200), async {
            let mut dns = DnsServer::new(0, Some("non-such-file".into()))
                .await
                .unwrap();
            let notify_tx = dns.notify_tx.clone();
            let ((), dns_out) = join!(
                async move {
                    sleep(Duration::from_millis(30)).await;
                    _ = notify_tx.send(Reload).await;
                },
                dns.run(),
            );
            match dns_out {
                Err(e) if e.to_string().contains("os error 2") => {}
                val => panic!(
                    "reloading non existent file should have produced an error. got: {val:?}"
                ),
            }
        })
        .await
        .unwrap(); // panic on timeout
    }

    #[tokio::test]
    async fn reloading_records_updates_live_service() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        timeout(Duration::from_secs(1), async {
            let host = "test-host.local".to_owned();
            let mut records_file = NamedTempFile::new().unwrap();
            writeln!(records_file, "# comment").unwrap();
            let mut dns = DnsServer::new(0, Some(records_file.path().into()))
                .await
                .unwrap();
            let notify_tx = dns.notify_tx.clone();
            let lookup_tx = dns.lookup_tx.clone();
            let ((), dns_out) = join!(
                async move {
                    let (tx1, rx2) = oneshot::channel();
                    let _ = lookup_tx.send(ARecordQuery(host.clone(), tx1)).await;
                    let ip1 = rx2.await.unwrap().unwrap();
                    assert_eq!(ip1, Ipv4Addr::LOCALHOST);
                    _ = notify_tx.send(Reload).await;
                    writeln!(records_file, "{host}:192.168.0.1").unwrap();
                    let (tx2, rx2) = oneshot::channel();
                    let _ = lookup_tx.send(ARecordQuery(host, tx2)).await;
                    let ip2 = rx2.await.unwrap().unwrap();
                    assert_eq!(ip2, "192.168.0.1".parse::<Ipv4Addr>().unwrap());
                    notify_tx.send(Shutdown).await.unwrap();
                },
                dns.run(),
            );
            dns_out.unwrap();
        })
        .await
        .unwrap(); // panic on timeout
    }
}
