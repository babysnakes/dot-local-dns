use anyhow::{anyhow, Error};
use clap::Parser;
use hickory_resolver::config::{ConnectionConfig, NameServerConfig, ResolverConfig};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::Resolver;
use rand::Rng;
use rand_regex::Regex;
use std::net::Ipv4Addr;

/// Send multiple concurrent A record queries for generated hosts within the provided domain.
///
/// Fails on the first error!
#[derive(Parser)]
struct Args {
    /// The top-level domain to generate hosts for
    #[arg(long, default_value = "loc")]
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
        Resolver::builder_with_config(config, TokioRuntimeProvider::default()).build()?;
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
    resolver: &Resolver<TokioRuntimeProvider>,
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
    let mut conn = ConnectionConfig::udp();
    conn.port = 2053;
    let name_server = NameServerConfig::new(Ipv4Addr::LOCALHOST.into(), false, vec![conn]);
    ResolverConfig::from_parts(None, vec![], vec![name_server])
}
