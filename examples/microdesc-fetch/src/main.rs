use anyhow::Result;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use std::net::SocketAddr;
use tor_linkspec::OwnedChanTarget;
use tor_llcrypto::pk::rsa::RsaIdentity;

/// Minimal test: connect directly to a bridge's ORPort via TCP,
/// create a one-hop circuit (CREATE_FAST), then open multiple
/// BEGIN_DIR streams sequentially to test flow control.
///
/// Stream 1: fetch /tor/server/authority (small, ~2KB)
/// Stream 2: fetch /tor/status-vote/current/consensus-microdesc (~900KB compressed)
/// Stream 3: fetch /tor/server/authority again (should work if flow control is OK)

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tor_proto=info".parse().unwrap()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: microdesc-fetch <host:port> <rsa-fingerprint> [num-streams]");
        eprintln!("Example: microdesc-fetch 127.0.0.1:8888 C483C466A286FF696FD7E7A057ED8DD3BE752952 5");
        std::process::exit(1);
    }

    let addr: SocketAddr = args[1].parse()?;
    let fingerprint = &args[2];
    let num_streams: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(5);

    let rsa_bytes: Vec<u8> = (0..fingerprint.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&fingerprint[i..i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let rsa_id = RsaIdentity::from_bytes(&rsa_bytes)
        .ok_or_else(|| anyhow::anyhow!("Invalid RSA fingerprint"))?;

    eprintln!("Target: {} ({})", addr, fingerprint);
    eprintln!("Will open {} sequential streams", num_streams);

    // Use arti-client to bootstrap (gets us a circmgr)
    let config = arti_client::TorClientConfig::default();
    eprintln!("Bootstrapping...");
    let tor_client = arti_client::TorClient::create_bootstrapped(config).await?;

    let circmgr = tor_client.circmgr();

    let target = OwnedChanTarget::builder()
        .addrs(vec![addr.into()])
        .rsa_identity(rsa_id)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build target: {}", e))?;

    eprintln!("Opening one-hop circuit to bridge...");
    let tunnel = circmgr.get_or_launch_dir_specific(target).await?;

    let requests = vec![
        ("/tor/server/authority", "bridge descriptor"),
        ("/tor/status-vote/current/consensus-microdesc", "consensus"),
        ("/tor/server/authority", "bridge descriptor (post-consensus)"),
        ("/tor/server/authority", "bridge descriptor (4th stream)"),
        ("/tor/server/authority", "bridge descriptor (5th stream)"),
    ];

    for (i, (path, label)) in requests.iter().take(num_streams).enumerate() {
        eprintln!("\n--- Stream {} ({}) ---", i + 1, label);

        let mut stream = tunnel.begin_dir_stream().await?;
        let request = format!(
            "GET {} HTTP/1.0\r\nAccept-Encoding: identity\r\n\r\n",
            path
        );
        eprintln!("Sending GET {} ({} bytes)", path, request.len());
        stream.write_all(request.as_bytes()).await?;
        stream.flush().await?;

        let start = std::time::Instant::now();
        let mut total = 0usize;
        let mut chunk = vec![0u8; 65536];
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                stream.read(&mut chunk),
            )
            .await
            {
                Ok(Ok(0)) => {
                    eprintln!("  EOF: {} bytes in {:?}", total, start.elapsed());
                    break;
                }
                Ok(Ok(n)) => {
                    total += n;
                    if total <= n || total % (100 * 1024) < n {
                        eprintln!("  received {} bytes ({:?})", total, start.elapsed());
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("  ERROR after {} bytes: {}", total, e);
                    break;
                }
                Err(_) => {
                    eprintln!("  TIMEOUT after {} bytes in {:?}", total, start.elapsed());
                    break;
                }
            }
        }
    }

    eprintln!("\nDone!");
    Ok(())
}