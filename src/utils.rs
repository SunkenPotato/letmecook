use rocket::{Build, Request, Response, Rocket, Route, fairing::Fairing, http::Header};
use sealed_trait::sealed_trait;

pub trait Module {
    const BASE_PATH: &str;

    fn routes() -> Vec<Route>;
}

sealed_trait! {
    pub sealed trait RocketExt permits Rocket<Build> => {
        fn add<M: Module>(self) -> Self;
    }
}

impl RocketExt for Rocket<Build> {
    fn add<M: Module>(self) -> Self {
        self.mount(format!("/api{}", M::BASE_PATH), M::routes())
    }
}

pub struct CORSFairing;

#[rocket::async_trait]
impl Fairing for CORSFairing {
    fn info(&self) -> rocket::fairing::Info {
        rocket::fairing::Info {
            name: "CORS Fairing",
            kind: rocket::fairing::Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        response.set_header(Header::new("Access-Control-Allow-Origin", "*"));
        response.set_header(Header::new(
            "Access-Control-Allow-Methods",
            "POST, GET, PATCH, OPTIONS",
        ));
        response.set_header(Header::new("Access-Control-Allow-Headers", "*"));
        response.set_header(Header::new("Access-Control-Allow-Credentials", "true"));
    }
}
