mod protocol;

use protocol::*;
use std::net::{Ipv4Addr, SocketAddr};
use tokio::net::UdpSocket;
use windows_sys::Win32::Foundation::{BOOL, FALSE};

const TOP_LEVEL_DOMAIN: &str = ".local";

pub async fn dns_server(port: u16) -> Result<()> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let socket = mk_udp_socket(&addr).await?;
    println!("Listening on: localhost:{}", port);
    loop {
        let mut req_buffer = BytePacketBuffer::new();
        let (_len, peer) = socket.recv_from(&mut req_buffer.buf).await?;
        let request = DnsPacket::from_buffer(&mut req_buffer).await?;
        let mut response = lookup(&request)?;
        let mut res_buffer = BytePacketBuffer::new();
        response.write(&mut res_buffer)?;
        let pos = res_buffer.pos();
        let data = res_buffer.get_range(0, pos)?;
        let _ = socket.send_to(data, peer).await?;
    }
}

fn lookup(request: &DnsPacket) -> Result<DnsPacket> {
    // dbg!{request};
    let id = &request.header.id;
    // trace!("received query (id: {}): {:?}", &id, &request);
    let mut response = DnsPacket::new();
    response.header.response = true;
    response.header.id = *id;
    response.header.recursion_desired = request.header.recursion_desired;

    if request.questions.is_empty() {
        response.header.rescode = ResultCode::NOTIMP;
        return Ok(response);
    }

    let query = &request.questions[0];
    response.questions.push(query.clone());

    if request.header.response {
        // warn!("received response as question (id: {})", &id);
        response.header.rescode = ResultCode::NOTIMP;
        return Ok(response);
    }

    if request.header.opcode != 0 {
        // warn!("received non-zero opcode (id: {})", &id);
        response.header.rescode = ResultCode::NOTIMP;
        return Ok(response);
    }

    if !query.name.ends_with(TOP_LEVEL_DOMAIN) {
        // warn!("unsupported domain (id: {}): {}", &id, &query.name);
        response.header.rescode = ResultCode::SERVFAIL;
        return Ok(response);
    }

    match &query.qtype {
        QueryType::A => {
            let record = DnsRecord::A {
                addr: Ipv4Addr::LOCALHOST,
                domain: query.name.to_string(),
                ttl: 0,
            };
            response.answers.push(record);
        }
        QueryType::AAAA | QueryType::CNAME | QueryType::MX | QueryType::NS | QueryType::SOA => {
            // debug!("received request for undefined query type: {:?}", &query);
            response.header.rescode = ResultCode::NOERROR;
        }
        QueryType::UNKNOWN(_x) => {
            // warn!("received query of unsupported type ({}): {:?}", x, &query);
            response.header.rescode = ResultCode::SERVFAIL;
        }
    }
    // debug!("response is: {:#?}", &response);
    // dbg!{&response};
    Ok(response)
}

#[cfg(not(windows))]
async fn mk_udp_socket(addr: SocketAddr) -> std::io::Result<UdpSocket> {
    UdpSocket::bind(addr).await
}

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
            &mut enable as *mut _ as _,
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

#[cfg(test)]
mod tests {
    use super::lookup;
    use super::protocol::*;
    use std::net::Ipv4Addr;

    macro_rules! lookup_tests {
        ($name:ident, $query_packet:expr, $response_code:expr, $extra_tests:expr) => {
            #[test]
            fn $name() {
                let response = lookup($query_packet).unwrap();
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
        assert_eq!(
          response.answers[0],
          DnsRecord::A {
            domain: "sub.domain.local".to_string(),
            addr: Ipv4Addr::LOCALHOST,
            ttl: 0
          }
        );
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

    fn packet_with_question(name: String, query_type: QueryType) -> DnsPacket {
        let mut packet = DnsPacket::new();
        packet.header.id = 10;
        packet.questions.push(DnsQuestion::new(name, query_type));
        packet.clone()
    }
}
