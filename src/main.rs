use std::sync::Arc;

use axum::{Extension, Router};
use log::info;
use sqlx::postgres::PgPoolOptions;
use tokio::{net::TcpListener, signal};

static KEY: [u8; 512] = *include_bytes!("../key");

pub struct AppDB(pub sqlx::PgPool);

impl AppDB {
    async fn new(url: &str) -> Result<Self, sqlx::Error> {
        PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map(|v| Self(v))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let startup_time = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string();

    #[cfg(not(test))]
    log4rs::init_file("log4rs.yaml", Default::default()).unwrap();

    info!("Starting server");
    info!("Process ID: {}", std::process::id());

    let db_conn_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL environment variable should be set");
    info!("Attempting connection to database with environment variable URL...");
    let db_conn = Arc::new(AppDB::new(&db_conn_url).await?);
    info!("Connection OK");

    let router = Router::new().layer(Extension(db_conn));

    let listener = TcpListener::bind("127.0.0.1:8000").await?;

    info!("Starting listener");

    tokio::select! {
        _ = signal::ctrl_c() => {}
        _ = axum::serve(listener, router) => {}
    }

    tokio::fs::rename("log/latest.log", format!("log/{startup_time}.log")).await?;

    Ok(())
}
