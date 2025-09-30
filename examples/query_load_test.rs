use anyhow::{anyhow, Error};
use clap::Parser;
use hickory_resolver::config::{NameServerConfig, ResolverConfig};
use hickory_resolver::name_server::{GenericConnector, TokioConnectionProvider};
use hickory_resolver::proto::runtime::TokioRuntimeProvider;
use hickory_resolver::Resolver;
use rand::Rng;
use rand_regex::Regex;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

/// Send multiple concurrent A record queries for generated hosts within the provided domain.
///
/// Fails on the first error!
#[derive(Parser)]
struct Args {
    /// The top-level domain to generate hosts for
    #[arg(long, default_value = "local")]
    domain: String,
    /// Number of requests to send
    #[arg(long, short, default_value = "1000")]
    count: usize,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::parse();
    run(args).await
}

async fn run(args: Args) -> Result<(), Error> {
    let domains = generate_hostname(&args.domain, args.count);
    let chunks = split_vec_into_parts(&domains, 4);
    // let sizes = chunks.iter().map(|c| c.len()).collect::<Vec<usize>>();
    // dbg!(sizes);
    let config = mk_resolver_config();
    let resolver =
        Resolver::builder_with_config(config, TokioConnectionProvider::default()).build();
    tokio::try_join!(
        mk_resolver_worker(chunks[0], &resolver),
        mk_resolver_worker(chunks[1], &resolver),
        mk_resolver_worker(chunks[2], &resolver),
        mk_resolver_worker(chunks[3], &resolver),
    )
    .map(|_| ())
}

async fn mk_resolver_worker(
    hosts: &[String],
    resolver: &Resolver<GenericConnector<TokioRuntimeProvider>>,
) -> Result<(), Error> {
    let resolver = resolver.clone();
    for host in hosts {
        // Because of some race conditions we might get a None here
        let ips = resolver.lookup_ip(host).await?;
        if ips.iter().count() > 0 {
            print!(".");
        } else {
            return Err(anyhow!("no ip found for {host}"));
        }
    }
    Ok(())
}

fn split_vec_into_parts<T>(vec: &[T], num_parts: usize) -> Vec<&[T]> {
    let chunk_size = vec.len().div_ceil(num_parts); // Ceiling division
    vec.chunks(chunk_size).collect()
}

fn generate_hostname(domain: &str, samples: usize) -> Vec<String> {
    let pattern = format!("([a-z0-9]{{3,10}}\\.){{1,3}}{}", regex::escape(domain));
    let gen = Regex::compile(&pattern, 100).expect("Invalid regex pattern");
    let mut rng = rand::rng();

    // Sample a string that matches the regex
    (&mut rng)
        .sample_iter(&gen)
        .take(samples)
        .collect::<Vec<String>>()
}

fn mk_resolver_config() -> ResolverConfig {
    let name_server = NameServerConfig {
        socket_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 2053)),
        protocol: Default::default(),
        tls_dns_name: None,
        http_endpoint: None,
        trust_negative_responses: false,
        bind_addr: None,
    };
    ResolverConfig::from_parts(None, vec![], vec![name_server])
}
