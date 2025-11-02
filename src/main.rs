mod recipe;
pub mod user;

use std::{
    sync::{Arc, LazyLock},
    task::{Context, Poll},
};

use axum::{
    Extension, Router,
    routing::{delete, get, post, put},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use log::{debug, info};
use sqlx::postgres::PgPoolOptions;
use tokio::{net::TcpListener, signal};
use tower::Service;
use tower_http::cors::CorsLayer;
use tower_layer::Layer;

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
        .route("/recipe", post(recipe::create))
        .route("/recipe/{id}", get(recipe::read))
        .route("/recipe/{id}", delete(recipe::delete))
        .route("/recipe/{id}", put(recipe::update))
        .route("/recipe/search", get(recipe::search))
        .layer(CorsLayer::permissive())
        .layer(LogLayer)
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

#[derive(Clone)]
pub struct LogLayer;

impl<S> Layer<S> for LogLayer {
    type Service = LogService<S>;

    fn layer(&self, service: S) -> Self::Service {
        LogService { service }
    }
}

// This service implements the Log behavior
#[derive(Clone)]
pub struct LogService<S> {
    service: S,
}

impl<S, Request> Service<Request> for LogService<S>
where
    S: Service<Request>,
    Request: std::fmt::Debug,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // Insert log statement here or other functionality
        debug!("Incoming request: {request:?}");
        self.service.call(request)
    }
}
