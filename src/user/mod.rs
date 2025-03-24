use rocket::routes;

use crate::utils::Module;

mod auth;

pub struct UserModule;

impl Module for UserModule {
    const BASE_PATH: &str = "/user";

    fn routes() -> Vec<rocket::Route> {
        routes![
            auth::create_user, // TODO: move to separate file
            auth::login,
            auth::verify_token,
            auth::retrieve_user // move
        ]
    }
}
