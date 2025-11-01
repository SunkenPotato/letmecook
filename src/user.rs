use axum::{
    Extension, Json,
    http::{HeaderName, HeaderValue, StatusCode},
};
use axum_extra::TypedHeader;
use chrono::TimeDelta;
use jsonwebtoken::{TokenData, decode, encode};
use log::error;
use serde::{Deserialize, Serialize};

use crate::{DEC_KEY, DbPool, ENC_KEY, JWT_HEADER, VALIDATION};

#[derive(Deserialize, Serialize)]
pub struct User {
    username: String,
    password: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Authorization {
    pub sub: i32,
    pub exp: i64,
}

impl axum_extra::headers::Header for Authorization {
    fn name() -> &'static axum::http::HeaderName {
        static N: HeaderName = HeaderName::from_static("authorization");
        &N
    }

    fn encode<E: Extend<axum::http::HeaderValue>>(&self, values: &mut E) {
        let jwt = encode(&JWT_HEADER, self, &ENC_KEY).unwrap();
        values.extend(HeaderValue::from_str(&jwt));
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum_extra::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        let value = values
            .next()
            .ok_or_else(axum_extra::headers::Error::invalid)?;

        let claims: TokenData<Authorization> = decode(
            value
                .to_str()
                .map_err(|_| axum_extra::headers::Error::invalid())?,
            &DEC_KEY,
            &VALIDATION,
        )
        .map_err(|_| axum_extra::headers::Error::invalid())?;

        Ok(claims.claims)
    }
}

#[axum::debug_handler]
pub async fn create(Extension(pool): DbPool, Json(user): Json<User>) -> (StatusCode, &'static str) {
    if sqlx::query!(
        "select * from users where name = $1 and deleted = false",
        user.username
    )
    .fetch_one(&pool.0)
    .await
    .is_ok()
    {
        return (StatusCode::CONFLICT, "A user with that name already exists");
    }

    let hash = password_auth::generate_hash(&user.password);

    if let Err(e) = sqlx::query!(
        "insert into users (name, hash) values ($1, $2)",
        user.username,
        hash
    )
    .execute(&pool.0)
    .await
    {
        error!("An occurred while creating a user: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create a user. Contact support.",
        );
    }

    (StatusCode::CREATED, "User created")
}

#[axum::debug_handler]
pub async fn login(Extension(pool): DbPool, Json(user): Json<User>) -> (StatusCode, String) {
    let Ok(db_user) = sqlx::query!("select * from users where name = $1", user.username)
        .fetch_one(&pool.0)
        .await
    else {
        return (
            StatusCode::NOT_FOUND,
            "Could not find a user with that name".to_string(),
        );
    };

    if password_auth::verify_password(user.password, &db_user.hash).is_err() {
        return (
            StatusCode::NOT_FOUND,
            "Could not find a user with that name".to_string(),
        );
    }

    let claims = Authorization {
        sub: db_user.id,
        exp: chrono::offset::Utc::now()
            .checked_add_signed(TimeDelta::hours(1))
            .unwrap()
            .timestamp(),
    };

    let jwt = encode(&*JWT_HEADER, &claims, &ENC_KEY).unwrap();

    (StatusCode::OK, jwt)
}

pub async fn delete(
    Extension(pool): DbPool,
    TypedHeader(jwt): TypedHeader<Authorization>,
) -> (StatusCode, &'static str) {
    match sqlx::query!(
        "update users set deleted = true where id = $1 and deleted = false",
        jwt.sub
    )
    .execute(&pool.0)
    .await
    {
        Ok(q) => {
            if q.rows_affected() == 0 {
                (StatusCode::NOT_FOUND, "User does not exist")
            } else {
                (StatusCode::OK, "Deleted user permanently")
            }
        }
        Err(e) => {
            error!("Error while trying to update user: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete user")
        }
    }
}
