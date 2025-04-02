use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader};
use serde::Deserialize;
use tokio_postgres::{NoTls, Client};
use config::{Config, File};
use std::sync::Arc;
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

    // Start TCP listener
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080");

    // Run the main loop
    main_loop(client, listener).await?;

    Ok(())
}

async fn store_data(client: &Client,
        stats: &SystemStats) -> Result<(), tokio_postgres::Error> {
    client.execute(
        "INSERT INTO server.system_stats (cpu_usage, memory_used, memory_total, timestamp)
         VALUES ($1, $2, $3, $4)",
        &[
            &stats.cpu_usage,
            &stats.memory_used,
            &stats.memory_total,
            &stats.timestamp,
        ],
    ).await?;
    Ok(())
}

async fn main_loop(client: Arc<Client>,
        listener: TcpListener) -> tokio::io::Result<()> {
    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from {}", addr);
        let client = Arc::clone(&client);

        tokio::spawn(async move {
            let reader = BufReader::new(socket);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<SystemStats>(&line) {
                    Ok(stats) => {
                        println!("[{}] Received: {:?}", addr, stats);

                        if let Err(e) = store_data(&client, &stats).await {
                            eprintln!("Failed to store data: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Invalid JSON from {}: {}", addr, e),
                }
            }
            println!("Client {} disconnected", addr);
        });
    }
}
