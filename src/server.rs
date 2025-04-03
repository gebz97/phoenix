use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader};
use serde::Deserialize;
use tokio_postgres::{NoTls, Client};
use config::{Config, File};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::SystemTime;

#[derive(Debug, Deserialize)]
struct SystemStats {
    cpu_usage: f32,
    memory_used: i64,
    memory_total: i64,
    timestamp: SystemTime,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    dbname: String,
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    // Load configuration
    let config = Config::builder()
        .add_source(File::with_name("server"))
        .build()
        .expect("Failed to load config")
        .try_deserialize::<DatabaseConfig>()
        .expect("Invalid config format");

    let db_url = format!(
        "host={} port={} user={} password={} dbname={}",
        config.host, config.port, config.user, config.password, config.dbname
    );

    // Connect to PostgreSQL
    let (client, connection) = tokio_postgres::connect(&db_url, NoTls)
        .await
        .expect("Failed to connect to database");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Database connection error: {}", e);
        }
    });

    let client = Arc::new(client);
    let buffer = Arc::new(Mutex::new(Vec::new())); // Use async Mutex

    // Start TCP listener
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080");

    // Run the main loop
    main_loop(client, buffer, listener).await?;

    Ok(())
}

async fn store_data(client: &Client, stats_batch: &[SystemStats]) -> Result<(), tokio_postgres::Error> {
    if stats_batch.is_empty() {
        return Ok(());
    }

    let mut query = String::from("INSERT INTO server.system_stats (cpu_usage, memory_used, memory_total, timestamp) VALUES ");
    let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();

    for (i, stats) in stats_batch.iter().enumerate() {
        let idx = i * 4;
        query.push_str(&format!("(${}, ${}, ${}, ${})", idx + 1, idx + 2, idx + 3, idx + 4));
        if i < stats_batch.len() - 1 {
            query.push_str(", ");
        }
        params.push(&stats.cpu_usage);
        params.push(&stats.memory_used);
        params.push(&stats.memory_total);
        params.push(&stats.timestamp);
    }

    client.execute(&query, &params).await?;
    Ok(())
}

async fn main_loop(client: Arc<Client>, buffer: Arc<Mutex<Vec<SystemStats>>>, listener: TcpListener) -> tokio::io::Result<()> {
    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from {}", addr);
        let client = Arc::clone(&client);
        let buffer = Arc::clone(&buffer);

        tokio::spawn(async move {
            let reader = BufReader::new(socket);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<SystemStats>(&line) {
                    Ok(stats) => {
                        println!("[{}] Received: {:?}", addr, stats);

                        let mut buf = buffer.lock().await;
                        buf.push(stats);

                        if buf.len() >= 10 {
                            let batch = buf.drain(..).collect::<Vec<_>>();
                            drop(buf);

                            if let Err(e) = store_data(&client, &batch).await {
                                eprintln!("Failed to store batch data: {}", e);
                            }
                        }
                    }
                    Err(e) => eprintln!("Invalid JSON from {}: {}", addr, e),
                }
            }

            // Flush remaining data on disconnect
            let mut buf = buffer.lock().await;
            if !buf.is_empty() {
                let batch = buf.drain(..).collect::<Vec<_>>();
                drop(buf); // Release lock before awaiting

                if let Err(e) = store_data(&client, &batch).await {
                    eprintln!("Failed to store remaining data: {}", e);
                }
            }

            println!("Client {} disconnected", addr);
        });
    }
}
