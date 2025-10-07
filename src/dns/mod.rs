#![allow(clippy::wildcard_imports)]

mod protocol;
mod records;

use crate::prelude::*;
use failsafe::futures::CircuitBreaker;
use failsafe::Config;
use protocol::*;
pub use records::safe_open_records_file;
use std::io::Error;
use std::os::windows::io::AsRawSocket;
use std::ptr::null_mut;
use tokio::net::UdpSocket;
use tokio::select;
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::FALSE;
use windows_sys::Win32::Networking::WinSock::{WSAIoctl, SIO_UDP_CONNRESET, SOCKET};

pub struct DnsServer {
    top_level_domain: String,
    pub notify_tx: Sender<Notification>,
    port: u16,
    db_path: PathBuf,
    records: HashMap<String, Ipv4Addr>,
    notify_rx: Receiver<Notification>,
}

#[derive(Debug)]
enum Signal {
    Shutdown,
}

#[derive(Debug)]
pub enum Notification {
    Shutdown,
    Reload,
    ARecordQuery(String, oneshot::Sender<Result<Ipv4Addr>>),
    MergeRecords(PathBuf, oneshot::Sender<Result<()>>),
}

impl DnsServer {
    pub async fn new(port: u16, db_path: impl AsRef<Path>, top_level_domain: &str) -> Result<Self> {
        let db_path = db_path.as_ref().to_owned();
        let records = records::load(&db_path, top_level_domain).await?;
        let (notify_tx, notify_rx) = mpsc::channel::<Notification>(4);
        Ok(Self {
            top_level_domain: top_level_domain.to_owned(),
            notify_tx,
            port,
            db_path,
            records,
            notify_rx,
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
                        if let Some(Signal::Shutdown) = self.handle_notification(notification).await {
                            return Ok(());
                        }
                    }
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
        let records = records::load_from_file(&self.db_path, &self.top_level_domain).await?;
        self.records = records;
        info!("Records reloaded");
        Ok(())
    }

    async fn handle_notification(&mut self, notification: Notification) -> Option<Signal> {
        match notification {
            Shutdown => {
                info!("DNS server received shutdown");
                Some(Signal::Shutdown)
            }
            Reload => {
                info!("Reloading Records");
                self.reload_records()
                    .await
                    .inspect(|()| {
                        send_notification("Reloaded Records", "Reloaded records file successfully");
                    })
                    .unwrap_or_else(|e| {
                        let path = &self.db_path.to_string_lossy();
                        notify_error!("Error reloading records file ({path}): {e}");
                    });
                None
            }
            ARecordQuery(query, tx) => {
                self.handle_name_lookup(query, tx);
                None
            }
            MergeRecords(path, tx) => {
                match self.handle_merge_records(path).await {
                    Ok(()) => {
                        if tx.send(Ok(())).is_err() {
                            notify_error!("Records merged but encountered internal communication error, best to restart the app");
                        }
                    }
                    Err(e) => {
                        tx.send(Err(e)).unwrap_or_else(|e| {
                            notify_error!("Error merging records: {e:?}");
                        });
                    }
                }
                None
            }
        }
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
        let mut response = self.lookup(&request);
        let mut res_buffer = BytePacketBuffer::new();
        response.write(&mut res_buffer)?;
        let pos = res_buffer.pos();
        let data = res_buffer.get_range(0, pos)?;
        socket.send_to(data, peer).await?;
        Ok(())
    }

    fn handle_name_lookup(&self, host: String, tx: oneshot::Sender<Result<Ipv4Addr>>) {
        debug!("DNS server received lookup channel: {host}");
        let res = self.lookup_name(host);
        if tx.send(res).is_err() {
            error!("Error sending response to lookup channel");
        }
    }

    async fn handle_merge_records(&mut self, path: PathBuf) -> Result<()> {
        info!(
            "DNS server received merge records from file: {}",
            path.display()
        );
        let records = records::load_from_file(path, &self.top_level_domain).await?;
        self.records.extend(records);
        Ok(())
    }

    fn lookup_name(&self, host: String) -> Result<Ipv4Addr> {
        let mut query = DnsPacket::new();
        let question = DnsQuestion::new(host, QueryType::A);
        query.questions.push(question);
        let response = &self.lookup(&query);
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

    fn lookup(&self, request: &DnsPacket) -> DnsPacket {
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

        if !query.name.ends_with(&self.top_level_domain) {
            warn!("unsupported domain (id: {}): {}", &id, &query.name);
            response.header.rescode = ResultCode::SERVFAIL;
            return response;
        }

        match &query.qtype {
            QueryType::A => {
                let record = DnsRecord::A {
                    addr: ip_from_domain_or_default(&query.name, &self.records),
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
}

fn ip_from_domain_or_default(host: &str, domain: &HashMap<String, Ipv4Addr>) -> Ipv4Addr {
    domain
        .iter()
        .find(|&(name, _)| name == host || host.ends_with(&format!(".{name}")))
        .map_or(Ipv4Addr::LOCALHOST, |(_, ip)| *ip)
}

#[allow(clippy::cast_possible_truncation)]
async fn mk_udp_socket(addr: &SocketAddr) -> std::io::Result<UdpSocket> {
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
            &raw mut bytes_returned,
            null_mut(),
            None,
        )
    };
    if result != 0 {
        return Err(Error::last_os_error());
    }

    Ok(socket)
}

#[cfg(test)]
mod tests {
    use super::protocol::*;
    use crate::dns::records::RecordsDB;
    use crate::prelude::*;
    use std::str::FromStr;
    use tempfile::NamedTempFile;
    use tokio::join;
    use tokio::time::{sleep, timeout, Duration};

    const TOP_LEVEL: &str = ".loc";

    #[tokio::test]
    async fn normal_dns_request() {
        let mut query = packet_with_question("hello.loc".to_string(), QueryType::A);
        query.header.recursion_desired = true;
        let response = basic_query_and_validation(query, ResultCode::NOERROR, records()).await;
        assert!(response.header.recursion_desired);
        assert_eq!(
            response.questions[0].name, "hello.loc",
            "response question's name doesn't match original name"
        );
        assert_eq!(
            response.answers[0],
            DnsRecord::A {
                domain: "hello.loc".to_string(),
                addr: Ipv4Addr::LOCALHOST,
                ttl: 0
            }
        );
    }

    #[tokio::test]
    async fn subdomain_a_requests_are_supported() {
        let query = packet_with_question("sub.domain.loc".to_string(), QueryType::A);
        let response = basic_query_and_validation(query, ResultCode::NOERROR, records()).await;
        match response.answers[0] {
            DnsRecord::A {
                ref domain,
                ref addr,
                ..
            } => {
                assert_eq!(domain, "sub.domain.loc");
                assert_eq!(*addr, Ipv4Addr::LOCALHOST);
            }
            _ => panic!(
                "Did not receive DnsRecord::A (received {:?}",
                response.answers[0]
            ),
        }
    }

    #[tokio::test]
    async fn query_of_existing_record_returns_the_record() {
        let query = packet_with_question("registered.loc".to_string(), QueryType::A);
        let response = basic_query_and_validation(query, ResultCode::NOERROR, records()).await;
        match response.answers[0] {
            DnsRecord::A {
                ref domain,
                ref addr,
                ..
            } => {
                assert_eq!(domain, "registered.loc");
                assert_eq!(*addr, "192.168.0.1".parse::<Ipv4Addr>().unwrap());
            }
            _ => panic!(
                "Did not receive DnsRecord::A (received {:?}",
                response.answers[0]
            ),
        }
    }

    #[tokio::test]
    async fn query_subdomain_of_existing_record_returns_the_record() {
        let query = packet_with_question("sub.registered.loc".to_string(), QueryType::A);
        let response = basic_query_and_validation(query, ResultCode::NOERROR, records()).await;
        match response.answers[0] {
            DnsRecord::A {
                ref domain,
                ref addr,
                ..
            } => {
                assert_eq!(domain, "sub.registered.loc");
                assert_eq!(*addr, "192.168.0.1".parse::<Ipv4Addr>().unwrap());
            }
            _ => panic!(
                "Did not receive DnsRecord::A (received {:?}",
                response.answers[0]
            ),
        }
    }

    #[tokio::test]
    async fn query_name_that_ends_with_existing_record_returns_localhost() {
        let query = packet_with_question("not-registered.loc".to_string(), QueryType::A);
        let response = basic_query_and_validation(query, ResultCode::NOERROR, records()).await;
        match response.answers[0] {
            DnsRecord::A {
                ref domain,
                ref addr,
                ..
            } => {
                assert_eq!(domain, "not-registered.loc");
                assert_eq!(*addr, Ipv4Addr::LOCALHOST);
            }
            _ => panic!(
                "Did not receive DnsRecord::A (received {:?}",
                response.answers[0]
            ),
        }
    }

    #[tokio::test]
    async fn soa_requests_return_no_error_and_zero_answers() {
        let query = packet_with_question("test.loc".to_string(), QueryType::SOA);
        let response = basic_query_and_validation(query, ResultCode::NOERROR, records()).await;
        assert_eq!(response.answers.len(), 0);
    }

    #[tokio::test]
    async fn ns_requests_return_no_error_and_zero_answers() {
        let query = packet_with_question("test.loc".to_string(), QueryType::NS);
        let response = basic_query_and_validation(query, ResultCode::NOERROR, records()).await;
        assert_eq!(response.answers.len(), 0);
    }

    #[tokio::test]
    async fn packets_with_no_queries_are_not_implemented() {
        let mut query = DnsPacket::new();
        query.header.id = 1234;
        basic_query_and_validation(query, ResultCode::NOTIMP, records()).await;
    }

    #[tokio::test]
    async fn response_packets_are_not_supported() {
        let mut query = packet_with_question("test.loc".to_string(), QueryType::A);
        query.header.response = true;
        basic_query_and_validation(query, ResultCode::NOTIMP, records()).await;
    }

    #[tokio::test]
    async fn non_zero_opcode_are_not_supported() {
        let mut query = packet_with_question("test.loc".to_string(), QueryType::A);
        query.header.opcode = 1;
        basic_query_and_validation(query, ResultCode::NOTIMP, records()).await;
    }

    #[tokio::test]
    async fn does_not_accept_wrong_domain() {
        let query = packet_with_question("example.com".to_string(), QueryType::A);
        let response = basic_query_and_validation(query, ResultCode::SERVFAIL, records()).await;
        assert_eq!(response.answers.len(), 0);
    }

    #[tokio::test]
    async fn service_starts_with_no_db_file() {
        let mut dns = DnsServer::new(0, "non-existent-file", TOP_LEVEL)
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
    async fn reloading_records_updates_live_service() {
        timeout(Duration::from_secs(1), async {
            let host = "test-host.loc".to_owned();
            let mut records_file = NamedTempFile::new().unwrap();
            writeln!(records_file, "# comment").unwrap();
            let mut dns = DnsServer::new(0, records_file.path(), TOP_LEVEL)
                .await
                .unwrap();
            let notify_tx = dns.notify_tx.clone();
            let ((), dns_out) = join!(
                async move {
                    let (tx1, rx2) = oneshot::channel();
                    let _ = notify_tx.send(ARecordQuery(host.clone(), tx1)).await;
                    let ip1 = rx2.await.unwrap().unwrap();
                    assert_eq!(ip1, Ipv4Addr::LOCALHOST);
                    _ = notify_tx.send(Reload).await;
                    writeln!(records_file, "{host}:192.168.0.1").unwrap();
                    let (tx2, rx2) = oneshot::channel();
                    let _ = notify_tx.send(ARecordQuery(host, tx2)).await;
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

    #[rustfmt::skip]
    #[tokio::test]
    async fn merge_records_workflow() {
        let records = "a.host.loc:192.168.0.4\r\nb-host.loc:192.168.0.4";
        let to_merge = "c.host.loc:192.168.1.1\r\nb-host.loc:192.168.1.1";
        let mut records_file = NamedTempFile::new().unwrap();
        writeln!(records_file, "{records}").unwrap();
        let mut merged_file = NamedTempFile::new().unwrap();
        writeln!(merged_file, "{to_merge}").unwrap();
        let mut dns = DnsServer::new(0, records_file.path(), TOP_LEVEL).await.unwrap();
        let notification_tx = dns.notify_tx.clone();
        timeout(Duration::from_secs(3), async {
            let ((), dns_out) = join!(
                async move {
                    let (tx1, rx1) = oneshot::channel();
                    notification_tx.send(MergeRecords(merged_file.path().into(), tx1)).await.unwrap();
                    rx1.await.unwrap().unwrap(); // panic if it's error
                    assert_eq!(run_lookup("a.host.loc", notification_tx.clone()).await.unwrap(), Ipv4Addr::from_str("192.168.0.4").unwrap(), "records-only host should remain the same");
                    assert_eq!(run_lookup("b-host.loc", notification_tx.clone()).await.unwrap(), Ipv4Addr::from_str("192.168.1.1").unwrap(), "merge should overwrite original");
                    assert_eq!(run_lookup("c.host.loc", notification_tx.clone()).await.unwrap(), Ipv4Addr::from_str("192.168.1.1").unwrap(), "merge only host should resolve");
                    notification_tx.send(Reload).await.unwrap();
                    assert_eq!(run_lookup("b-host.loc", notification_tx.clone()).await.unwrap(), Ipv4Addr::from_str("192.168.0.4").unwrap(), "after reset original host should resolve to original ip");
                    notification_tx.send(Shutdown).await.unwrap();
                },
                dns.run(),
            );
            dns_out.unwrap();
        }).await.unwrap();
    }

    async fn basic_query_and_validation(
        query: DnsPacket,
        result: ResultCode,
        records: RecordsDB,
    ) -> DnsPacket {
        let mut ds = DnsServer::new(0, "non-existent-file", TOP_LEVEL)
            .await
            .unwrap();
        ds.records = records;
        let response = ds.lookup(&query);
        assert_eq!(query.header.id, response.header.id);
        assert_eq!(response.header.rescode, result);
        response
    }

    fn records() -> HashMap<String, Ipv4Addr> {
        HashMap::from([("registered.loc".into(), "192.168.0.1".parse().unwrap())])
    }

    fn packet_with_question(name: String, query_type: QueryType) -> DnsPacket {
        let mut packet = DnsPacket::new();
        packet.header.id = 10;
        packet.questions.push(DnsQuestion::new(name, query_type));
        packet.clone()
    }

    async fn run_lookup(host: &str, notify_tx: Sender<Notification>) -> Result<Ipv4Addr> {
        let (tx, rx) = oneshot::channel();
        notify_tx.send(ARecordQuery(host.into(), tx)).await?;
        rx.await?
    }
}
