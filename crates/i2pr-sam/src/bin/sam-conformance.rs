use i2pr_sam::{ClientConfig, SamClient};
use i2pr_sam_proto::{SessionStyle, SharedDialect};
use serde_json::json;
use std::{env, net::SocketAddr};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("sam-conformance: {error}");
        std::process::exit(2);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut endpoint: Option<SocketAddr> = None;
    let mut router = String::from("unspecified");
    let mut router_version = String::from("unspecified");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--endpoint" => {
                endpoint = Some(args.next().ok_or("missing --endpoint value")?.parse()?)
            }
            "--router" => router = args.next().ok_or("missing --router value")?,
            "--router-version" => {
                router_version = args.next().ok_or("missing --router-version value")?
            }
            "--help" | "-h" => {
                println!(
                    "usage: sam-conformance --endpoint HOST:PORT --router LABEL --router-version VERSION_OR_SHA"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument {arg}").into()),
        }
    }
    let endpoint = endpoint.ok_or("--endpoint is required")?;
    let client = SamClient::connect(ClientConfig::new(endpoint)).await?;
    let version = client
        .capabilities()
        .await
        .negotiated_version
        .map(|v| format!("{}.{}", v.major, v.minor));
    let mut rows = Vec::new();
    let stream_row = match client
        .create_stream_session("TRANSIENT", "codex-stream", &[])
        .await
    {
        Ok(session) => {
            session.close().await;
            json!({"feature": "stream_session", "result": "probe_ok", "notes": "SESSION CREATE and close only; peer exchange is not included"})
        }
        Err(i2pr_sam::SamError::Rejected(message)) => {
            json!({"feature": "stream_session", "result": "rejected_or_unsupported", "notes": message})
        }
        Err(error) => {
            json!({"feature": "stream_session", "result": "failed", "notes": error.to_string()})
        }
    };
    rows.push(stream_row);
    for (feature, style) in [
        ("datagram", SessionStyle::Datagram),
        ("raw", SessionStyle::Raw),
        ("datagram2", SessionStyle::Datagram2),
        ("datagram3", SessionStyle::Datagram3),
    ] {
        let id = format!("codex-{feature}");
        let row = match client
            .create_datagram_session("TRANSIENT", &id, style, &[])
            .await
        {
            Ok(session) => {
                session.close().await;
                json!({"feature": feature, "result": "probe_ok", "notes": "session create and close only; payload exchange requires a peer"})
            }
            Err(i2pr_sam::SamError::Rejected(message)) => {
                json!({"feature": feature, "result": "rejected_or_unsupported", "notes": message})
            }
            Err(error) => {
                json!({"feature": feature, "result": "failed", "notes": error.to_string()})
            }
        };
        rows.push(row);
    }
    for dialect in [SharedDialect::Master, SharedDialect::Primary] {
        let label = match dialect {
            SharedDialect::Master => "MASTER",
            SharedDialect::Primary => "PRIMARY",
        };
        let row = match client
            .create_shared_session("TRANSIENT", &format!("codex-{label}"), dialect, &[])
            .await
        {
            Ok(session) => {
                let stream_child = session
                    .add_child(
                        &format!("codex-{label}-stream"),
                        i2pr_sam_proto::SessionStyle::Stream,
                        &[],
                    )
                    .await;
                let datagram_child = if stream_child.is_ok() {
                    session
                        .add_child(
                            &format!("codex-{label}-dgram"),
                            i2pr_sam_proto::SessionStyle::Datagram,
                            &[],
                        )
                        .await
                } else {
                    Err(i2pr_sam::SamError::Rejected(
                        "STREAM child add failed".into(),
                    ))
                };
                session.close().await;
                match (stream_child, datagram_child) {
                    (Ok(_), Ok(_)) => {
                        json!({"feature": "shared_owner", "dialect": label, "result": "probe_ok", "notes": "owner create, STREAM and DATAGRAM child add, then owner close; no peer payload exchange"})
                    }
                    (Err(error), _) | (_, Err(error)) => {
                        json!({"feature": "shared_owner", "dialect": label, "result": "rejected_or_unsupported", "notes": error.to_string()})
                    }
                }
            }
            Err(i2pr_sam::SamError::Rejected(message)) => {
                json!({"feature": "shared_owner", "dialect": label, "result": "rejected_or_unsupported", "notes": message})
            }
            Err(error) => {
                json!({"feature": "shared_owner", "dialect": label, "result": "failed", "notes": error.to_string()})
            }
        };
        rows.push(row);
    }
    println!(
        "{}",
        json!({"router": router, "router_version_or_sha": router_version, "endpoint": endpoint.to_string(), "negotiated_sam_version": version, "rows": rows})
    );
    Ok(())
}
