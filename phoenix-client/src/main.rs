use std::time::SystemTime;

use serde_json::json;
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration};
use sysinfo::System;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let addr = "127.0.0.1:8080";
    let mut stream = TcpStream::connect(addr).await?;
    println!("Connected to server at {}", addr);

    let mut system = System::new_all();

    loop {
        system.refresh_all();
        
        let stats = json!({
            "cpu_usage": system.global_cpu_usage(),
            "memory_used": system.used_memory(),
            "memory_total": system.total_memory(),
            "timestamp": SystemTime::now(),
        });

        let stats_string = stats.to_string() + "\n";

        if let Err(e) = stream.write_all(stats_string.as_bytes()).await {
            eprintln!("Failed to send data: {}", e);
            break;
        }
        println!("Sent: {}", stats_string.trim());

        sleep(Duration::from_secs(10)).await;
    }

    Ok(())
}
