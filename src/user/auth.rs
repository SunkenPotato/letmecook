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

#[derive(Responder)]
#[response(status = 200)]
struct Authentication(String);

#[derive(Debug, Responder)]
#[response(status = 403)]
enum AuthenticationError<'r> {
    Invalid(&'r str),
    Missing(&'r str),
}

#[derive(Debug, Responder)]
enum LoginError<'r> {
    AuthErr(AuthenticationError<'r>),
    NotFound(String),
    Other(String),
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Authentication {
    type Error = AuthenticationError<'r>;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match req.headers().get_one("Authentication") {
            None => Outcome::Error((
                Status::Unauthorized,
                AuthenticationError::Missing("Authentication header is not present"),
            )),
            Some(key) if validate_auth_key(key).is_ok() => {
                Outcome::Success(Authentication(key.into()))
            }
            Some(_) => rocket::outcome::Outcome::Error((
                Status::BadRequest,
                AuthenticationError::Invalid("Authentication expired"),
            )),
        }
    }
}

fn validate_auth_key(key: &str) -> Result<TokenData<Claims>, Error> {
    let validation = Validation::new(jsonwebtoken::Algorithm::HS512);

    decode::<Claims>(key, &DecodingKey::from_secret(&*crate::KEY), &validation)
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: i32,
    exp: i64,
}

impl Claims {
    fn new(sub: i32) -> Result<String, Error> {
        let claims = Self {
            sub,
            exp: Utc::now().timestamp() + 10800,
        };

        let header = Header::new(jsonwebtoken::Algorithm::HS512);
        let token = encode(&header, &claims, &EncodingKey::from_secret(&*crate::KEY));

        token
    }
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct User<'r> {
    username: &'r str,
    password: &'r str,
}

#[post("/", data = "<user>")]
pub(super) async fn create_user(user: Json<User<'_>>, mut db: Connection<AppDB>) -> Status {
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

#[post("/login", data = "<cred>")]
pub(crate) async fn login<'r>(
    cred: Json<User<'_>>,
    mut db: Connection<AppDB>,
) -> Result<(Status, Authentication), (Status, LoginError<'r>)> {
    // get user
    let Some(user) = query!("select * from users where name=$1", cred.username)
        .fetch_one(&mut **db)
        .await
        .ok()
    else {
        return Err((
            Status::NotFound,
            LoginError::NotFound(format!("User {} not found", cred.username)),
        ));
    };

    if user.deleted {
        return Err((
            Status::NotFound,
            LoginError::NotFound(format!("User {} not found", cred.username)),
        ));
    }

    match password_auth::verify_password(cred.password, &user.hash) {
        Ok(()) => match Claims::new(user.id) {
            Ok(v) => return Ok((Status::Ok, Authentication(v))),
            Err(e) => {
                return {
                    error!(
                        "Could not generate JWT for \"{}\" (id: {}): {}",
                        user.name, user.id, e
                    );

                    Err((
                        Status::InternalServerError,
                        LoginError::Other("Failed to generate JWT".into()),
                    ))
                };
            }
        },
        Err(e) => match e {
            VerifyError::PasswordInvalid => {
                return Err((
                    Status::Forbidden,
                    LoginError::AuthErr(AuthenticationError::Invalid("Invalid password")),
                ));
            }
            VerifyError::Parse(e) => {
                error!(
                    "An error occurred while parsing \"{}\"'s (id: {}) hash: {}",
                    user.name, user.id, e
                );

                return Err((
                    Status::InternalServerError,
                    LoginError::Other("Failed to parse stored hash".into()),
                ));
            }
        },
    }
}

#[get("/login")]
pub(crate) async fn verify_token(_auth: Authentication) {}

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

        let claims = validate_auth_key(&response.into_string().await.unwrap()).unwrap();
        assert_eq!(claims.claims.sub, DEV_ID);
    }
}
