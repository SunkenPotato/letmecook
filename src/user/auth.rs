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
pub struct Authorization(pub String);

impl Authorization {
    pub fn validate(&self) -> Result<TokenData<Claims>, Error> {
        let validation = Validation::new(jsonwebtoken::Algorithm::HS512);

        decode::<Claims>(&self.0, &DecodingKey::from_secret(&crate::KEY), &validation)
    }
}

#[derive(Debug, Responder)]
#[response(status = 403)]
pub enum AuthenticationError<'r> {
    #[response(status = 403)]
    Invalid(&'r str),
    #[response(status = 401)]
    Missing(&'r str),
}

// TODO: rename to UserError
#[derive(Debug, Responder)]
pub(super) enum LoginError<'r> {
    AuthErr(AuthenticationError<'r>),
    #[response(status = 404)]
    NotFound(String),
    #[response(status = 500)]
    InternalServerError(()),
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Authorization {
    type Error = AuthenticationError<'r>;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match req.headers().get_one("Authorization") {
            None => Outcome::Error((
                Status::Unauthorized,
                AuthenticationError::Missing("Authorization header is not present"),
            )),
            Some(key) if Authorization(key.into()).validate().is_ok() => {
                Outcome::Success(Authorization(key.into()))
            }
            Some(_) => rocket::outcome::Outcome::Error((
                Status::BadRequest,
                AuthenticationError::Invalid("Authorization expired"),
            )),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: i32,
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
pub async fn login<'r>(
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

                Err(LoginError::InternalServerError(()))
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

                Err(LoginError::InternalServerError(()))
            }
        },
    }
}

#[get("/login")]
pub async fn verify_token(_auth: Authorization) {}
