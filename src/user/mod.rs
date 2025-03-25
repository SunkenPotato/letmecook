#![allow(private_interfaces)]

use auth::{Authorization, LoginError, validate_auth_key};
use log::error;
use rocket::{get, http::Status, post, routes, serde::json::Json};
use rocket_db_pools::Connection;
use serde::{Deserialize, Serialize};
use sqlx::query;

use crate::{AppDB, utils::Module};

mod auth;

pub struct UserModule;

impl Module for UserModule {
    const BASE_PATH: &str = "/user";

    fn routes() -> Vec<rocket::Route> {
        routes![
            create_user, // TODO: move to separate file
            auth::login,
            auth::verify_token,
            retrieve_user // move
        ]
    }
}

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub(super) struct User<'r> {
    pub(super) password: &'r str,
    pub(super) username: &'r str,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct UserResponse {
    name: String,
    id: i32,
    created_at: i64,
}

#[get("/")]
pub(crate) async fn retrieve_user<'r>(
    auth: Authorization,
    mut db: Connection<AppDB>,
) -> Result<Json<UserResponse>, (Status, LoginError<'r>)> {
    let claims = match validate_auth_key(&auth.0) {
        Ok(v) => v,
        Err(e) => {
            error!("Inconsistency between request guard and this call: {e}");
            return Err((
                Status::InternalServerError,
                LoginError::Other("Could not verify authorization header".into()),
            ));
        }
    };

    let uid = claims.claims.sub;

    let Some(user) = query!("select * from users where id = $1", uid)
        .fetch_one(&mut **db)
        .await
        .ok()
    else {
        return Err((
            Status::NotFound,
            LoginError::NotFound("User not found".into()),
        ));
    };

    Ok(Json(UserResponse {
        name: user.name,
        id: user.id,
        created_at: user.createdat.as_utc().unix_timestamp(),
    }))
}

#[post("/", data = "<user>")]
async fn create_user(user: Json<User<'_>>, mut db: Connection<AppDB>) -> Status {
    let user_exists = query!(
        "select exists(select (1) from users where name = $1)",
        user.username
    )
    .fetch_one(&mut **db)
    .await
    .ok();

    if user_exists.unwrap().exists.unwrap() {
        return Status::Conflict;
    }

    let hash = password_auth::generate_hash(user.password);

    let created = query!(
        "insert into users (name, hash) values ($1, $2)",
        user.username,
        hash
    )
    .execute(&mut **db)
    .await;

    match created {
        Ok(_) => Status::Created,
        Err(e) => {
            error!("Error while creating user: {e}");
            Status::InternalServerError
        }
    }
}
