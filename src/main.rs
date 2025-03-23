pub mod user;
mod utils;

use std::sync::LazyLock;

use rand::Rng;
use rocket_db_pools::Database;
use user::UserModule;
use utils::RocketExt;

// TODO: use include! for a constant key
static KEY: LazyLock<[u8; 512]> = LazyLock::new(|| {
    let mut rand = rand::rng();
    let mut seq = [0u8; 512];
    rand.fill(&mut seq);

    seq
});

#[derive(Database)]
#[database("lmc")]
pub struct AppDB(rocket_db_pools::sqlx::PgPool);

#[rocket::launch]
fn rocket() -> _ {
    rocket::build().attach(AppDB::init()).add::<UserModule>()
}
