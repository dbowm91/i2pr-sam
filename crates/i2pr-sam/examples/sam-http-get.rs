use std::{env, net::SocketAddr, time::Duration};

use i2pr_sam::{ClientConfig, SamClient, SessionDestination};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = env::args()
        .nth(1)
        .unwrap_or_else(|| "i2p-projekt.i2p".to_owned());
    let endpoint: SocketAddr = "127.0.0.1:7656".parse()?;
    let mut config = ClientConfig::new(endpoint);
    config.control_timeout = Duration::from_secs(120);
    let client = SamClient::connect(config).await?;
    println!("sam=connected target={target}");
    let destination = client.lookup_destination(&target).await?;
    println!("name_resolution=ok");
    let session = client
        .create_stream_session(&SessionDestination::Transient, "sam-http-probe", &[])
        .await?;
    println!("session=created identity={}", session.identity().is_some());
    let mut stream = session.connect(destination.as_str(), None, None).await?;
    let request = format!(
        "GET / HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\nUser-Agent: i2pr-sam-live-probe\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await?;
    stream.flush().await?;
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(60), stream.read_to_end(&mut response)).await??;
    let header_end = response.windows(4).position(|w| w == b"\r\n\r\n");
    let header = header_end
        .map(|end| String::from_utf8_lossy(&response[..end]).into_owned())
        .unwrap_or_else(|| String::from_utf8_lossy(&response).into_owned());
    let status_line = header.lines().next().unwrap_or("<empty>");
    println!("response_bytes={}", response.len());
    println!("response_header={status_line}");
    let successful_http = response.starts_with(b"HTTP/1.")
        && status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .is_some_and(|code| (200..300).contains(&code));
    if successful_http {
        println!("http_response=valid");
    } else {
        println!("http_response=not_successful");
        return Err("I2P destination did not return an HTTP 2xx response".into());
    }
    session.close().await;
    Ok(())
}
