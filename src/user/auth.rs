use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode, errors::Error};
use rocket::{
    Request,
    http::Status,
    request::{FromRequest, Outcome},
};
use serde::{Deserialize, Serialize};

struct Authentication<'r>(&'r str);

#[derive(Debug)]
enum AuthenticationError {
    Invalid,
    Missing,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Authentication<'r> {
    type Error = AuthenticationError;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match req.headers().get_one("Authentication") {
            None => Outcome::Error((Status::BadRequest, AuthenticationError::Missing)),
            Some(key) if validate_auth_key(key) => Outcome::Success(Authentication(key)),
            Some(_) => {
                rocket::outcome::Outcome::Error((Status::BadRequest, AuthenticationError::Invalid))
            }
        }
    }
}

fn validate_auth_key(key: &str) -> bool {
    let validation = Validation::new(jsonwebtoken::Algorithm::HS512);

    decode::<Claims>(key, &DecodingKey::from_secret(&*crate::KEY), &validation).is_ok()
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: u32,
    exp: i64,
}

impl Claims {
    fn new(sub: u32) -> Result<String, Error> {
        let claims = Self {
            sub,
            exp: Utc::now().timestamp() + 10800,
        };

        let header = Header::new(jsonwebtoken::Algorithm::HS512);
        let token = encode(&header, &claims, &EncodingKey::from_secret(&*crate::KEY));

        dbg!(token)
    }
}
