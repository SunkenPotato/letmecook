mod recipe;
pub mod user;
mod utils;

use rocket_db_pools::Database;
use user::UserModule;
use utils::RocketExt;

static KEY: [u8; 512] = *include_bytes!("../key");

#[derive(Database)]
#[database("lmc")]
pub struct AppDB(rocket_db_pools::sqlx::PgPool);

#[rocket::launch]
fn rocket() -> _ {
    #[cfg(not(test))]
    log4rs::init_file("log4rs.yaml", Default::default()).unwrap();

    rocket::build().attach(AppDB::init()).add::<UserModule>()
}
