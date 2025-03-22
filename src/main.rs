pub mod user;
mod utils;

use rocket_db_pools::Database;
use user::UserModule;
use utils::RocketExt;

#[derive(Database)]
#[database("lmc")]
pub struct AppDB(rocket_db_pools::sqlx::PgPool);

#[rocket::launch]
fn rocket() -> _ {
    rocket::build().attach(AppDB::init()).add::<UserModule>()
}
