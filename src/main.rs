extern crate rocket;

mod recipe;
pub mod user;
mod utils;

use log::info;
use recipe::RecipeModule;
use rocket::tokio::fs;
use rocket_db_pools::Database;
use thiserror::Error;
use user::UserModule;
use utils::{CORSFairing, RocketExt};

static KEY: [u8; 512] = *include_bytes!("../key");

#[derive(Database)]
#[database("lmc")]
pub struct AppDB(rocket_db_pools::sqlx::PgPool);

#[derive(Debug, Error)]
pub enum Error {
    #[error("Rocket error: {0}")]
    Rocket(#[from] rocket::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[rocket::main]
async fn main() -> Result<(), Error> {
    let startup_time = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string();

    #[cfg(not(test))]
    log4rs::init_file("log4rs.yaml", Default::default()).unwrap();

    info!("Starting server");
    info!("Process ID: {}", std::process::id());

    rocket::build()
        .attach(AppDB::init())
        .attach(CORSFairing)
        .add::<UserModule>()
        .add::<RecipeModule>()
        .ignite()
        .await?
        .launch()
        .await?;

    fs::rename("log/latest.log", format!("log/{startup_time}.log")).await?;

    info!("Shutting down");

    Ok(())
}
