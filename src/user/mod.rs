#![allow(private_interfaces)]

// TODO: add tests - nc

use auth::{Authorization, LoginError};
use log::error;
use password_auth::generate_hash;
use rocket::{Responder, delete, get, http::Status, post, put, routes, serde::json::Json};
use rocket_db_pools::Connection;
use serde::{Deserialize, Serialize};
use sqlx::query;

use crate::{AppDB, utils::Module};

pub mod auth;

pub struct UserModule;

impl Module for UserModule {
    const BASE_PATH: &str = "/user";

    fn routes() -> Vec<rocket::Route> {
        routes![
            create_user,
            auth::login,
            auth::verify_token,
            retrieve_self_user,
            update_user,
            delete_user
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
pub(crate) async fn retrieve_self_user<'r>(
    auth: Authorization,
    mut db: Connection<AppDB>,
) -> Result<Json<UserResponse>, LoginError<'r>> {
    let claims = match auth.validate() {
        Ok(v) => v,
        Err(e) => {
            error!("Inconsistency between request guard and this call: {e}");
            return Err(LoginError::Other(()));
        }
    };

    let uid = claims.claims.sub;

    let Some(user) = query!("select * from users where id = $1 and deleted = false", uid)
        .fetch_one(&mut **db)
        .await
        .ok()
    else {
        return Err(LoginError::NotFound("User not found".into()));
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

#[delete("/")]
async fn delete_user(auth: Authorization, mut db: Connection<AppDB>) -> Status {
    let claims = auth
        .validate()
        .expect("expected key already to be validated");

    match query!(
        "update users set deleted = true where id = $1 and deleted = false",
        claims.claims.sub
    )
    .execute(&mut **db)
    .await
    {
        Ok(v) => match v.rows_affected() == 1 {
            true => Status::NoContent,
            _ => Status::NotFound,
        },
        Err(e) => {
            error!("Error occurred while trying to pseudo-delete user: {e}");

            Status::InternalServerError
        }
    }
}

#[derive(Responder)]
#[response(status = 201)]
struct UserUpdateResponse(());

#[put("/", data = "<user>")]
async fn update_user<'r>(
    auth: Authorization,
    user: Json<User<'r>>,
    mut db: Connection<AppDB>,
) -> Result<UserUpdateResponse, LoginError<'r>> {
    let claims = match auth.validate() {
        Ok(claims) => claims,
        Err(e) => {
            error!("Token should be valid: {e}");
            return Err(LoginError::Other(()));
        }
    };

    let hash = generate_hash(user.password);

    match sqlx::query!(
        "update users set name = $1, hash = $2 where id=$3",
        user.username,
        hash,
        claims.claims.sub
    )
    .execute(&mut **db)
    .await
    {
        Ok(v) => match v.rows_affected() == 1 {
            true => Ok(UserUpdateResponse(())),
            false => Err(LoginError::NotFound("user not found".into())),
        },
        Err(e) => {
            error!(
                "Error while trying to update user row (id={}): {}",
                claims.claims.sub, e
            );

            Err(LoginError::Other(()))
        }
    }
}
