mod user;

use std::sync::{Arc, LazyLock};

use axum::{
    Extension, Router,
    routing::{delete, post},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use log::info;
use sqlx::postgres::PgPoolOptions;
use tokio::{net::TcpListener, signal};

static RAW_KEY: &[u8] = include_bytes!("../key");
static ENC_KEY: LazyLock<EncodingKey> = LazyLock::new(|| EncodingKey::from_secret(&RAW_KEY));
static DEC_KEY: LazyLock<DecodingKey> = LazyLock::new(|| DecodingKey::from_secret(&RAW_KEY));
static VALIDATION: LazyLock<Validation> = LazyLock::new(Validation::default);
static JWT_HEADER: LazyLock<Header> = LazyLock::new(Header::default);

type DbPool = Extension<Arc<AppDB>>;

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

    let router = Router::new()
        .route("/user", post(user::create))
        .route("/user", delete(user::delete))
        .route("/user/login", post(user::login))
        .layer(Extension(db_conn));

    let listener = TcpListener::bind("127.0.0.1:8000").await?;

    info!("Starting listener");

    tokio::select! {
        _ = signal::ctrl_c() => {}
        _ = axum::serve(listener, router) => {}
    }

    tokio::fs::rename("log/latest.log", format!("log/{startup_time}.log")).await?;

    Ok(())
}
