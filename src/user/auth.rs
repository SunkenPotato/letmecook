#![allow(private_interfaces)]

use chrono::Utc;
use jsonwebtoken::{
    DecodingKey, EncodingKey, Header, TokenData, Validation, decode, encode, errors::Error,
};
use log::error;
use password_auth::VerifyError;
use rocket::{
    Request, get,
    http::Status,
    post,
    request::{FromRequest, Outcome},
    response::Responder,
    serde::json::Json,
};
use rocket_db_pools::Connection;
use serde::{Deserialize, Serialize};
use sqlx::query;

use crate::AppDB;

use super::User;

#[derive(Responder)]
#[response(status = 200)]
pub(super) struct Authorization(pub(super) String);

#[derive(Debug, Responder)]
#[response(status = 403)]
pub(super) enum AuthenticationError<'r> {
    #[response(status = 403)]
    Invalid(&'r str),
    #[response(status = 401)]
    Missing(&'r str),
}

#[derive(Debug, Responder)]
pub(super) enum LoginError<'r> {
    AuthErr(AuthenticationError<'r>),
    #[response(status = 404)]
    NotFound(String),
    #[response(status = 500)]
    Other(String),
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Authorization {
    type Error = AuthenticationError<'r>;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match req.headers().get_one("Authentication") {
            None => Outcome::Error((
                Status::Unauthorized,
                AuthenticationError::Missing("Authentication header is not present"),
            )),
            Some(key) if validate_auth_key(key).is_ok() => {
                Outcome::Success(Authorization(key.into()))
            }
            Some(_) => rocket::outcome::Outcome::Error((
                Status::BadRequest,
                AuthenticationError::Invalid("Authentication expired"),
            )),
        }
    }
}

pub(super) fn validate_auth_key(key: &str) -> Result<TokenData<Claims>, Error> {
    let validation = Validation::new(jsonwebtoken::Algorithm::HS512);

    decode::<Claims>(key, &DecodingKey::from_secret(&crate::KEY), &validation)
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Claims {
    pub(super) sub: i32,
    pub(super) exp: i64,
}

impl Claims {
    fn create_jwt(sub: i32) -> Result<String, Error> {
        let claims = Self {
            sub,
            exp: Utc::now().timestamp() + 10800, // 3h
        };

        let header = Header::new(jsonwebtoken::Algorithm::HS512);
        encode(&header, &claims, &EncodingKey::from_secret(&crate::KEY))
    }
}

#[post("/login", data = "<cred>")]
pub(crate) async fn login<'r>(
    cred: Json<User<'_>>,
    mut db: Connection<AppDB>,
) -> Result<Authorization, LoginError<'r>> {
    let Some(user) = query!(
        "select * from users where name=$1 and deleted = false",
        cred.username
    )
    .fetch_one(&mut **db)
    .await
    .ok() else {
        return Err(LoginError::NotFound(format!(
            "User {} not found",
            cred.username
        )));
    };

    match password_auth::verify_password(cred.password, &user.hash) {
        Ok(()) => match Claims::create_jwt(user.id) {
            Ok(v) => Ok(Authorization(v)),
            Err(e) => {
                error!(
                    "Could not generate JWT for \"{}\" (id: {}): {}",
                    user.name, user.id, e
                );

                Err(LoginError::Other("Failed to generate JWT".into()))
            }
        },
        Err(e) => match e {
            VerifyError::PasswordInvalid => Err(LoginError::AuthErr(AuthenticationError::Invalid(
                "Invalid password",
            ))),
            VerifyError::Parse(e) => {
                error!(
                    "An error occurred while parsing \"{}\"'s (id: {}) hash: {}",
                    user.name, user.id, e
                );

                Err(LoginError::Other("Failed to parse stored hash".into()))
            }
        },
    }
}

#[get("/login")]
pub(crate) async fn verify_token(_auth: Authorization) {} // body isn't required since the request guard checks for validity

#[cfg(test)]
mod tests {
    const DEV_ID: i32 = 1;

    use rocket::{async_test, http::Status, local::blocking::Client, uri};

    use crate::{rocket, user::auth::validate_auth_key};

    #[test]
    fn create_user() {
        let client = Client::tracked(rocket()).unwrap();

        let response = client
            .post(uri!("/api/user/"))
            .body(r#"{ "username": "sp", "password": "hello" }"#)
            .dispatch();

        assert!(response.status() == Status::Created || response.status() == Status::Conflict);
    }

    #[async_test]
    async fn login() {
        use rocket::local::asynchronous::Client;

        let client = Client::tracked(rocket()).await.unwrap();

        let response = client
            .post(uri!("/api/user/login"))
            .body(r#"{ "username": "sp", "password": "hello" }"#)
            .dispatch()
            .await;

        assert_eq!(response.status(), Status::Ok, "login failed");

        let key = response.into_string().await.unwrap();

        let claims = validate_auth_key(&key).unwrap();
        assert_eq!(claims.claims.sub, DEV_ID);
    }
}
