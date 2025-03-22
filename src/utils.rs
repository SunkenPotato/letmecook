use rocket::{Build, Rocket, Route};
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
        self.mount(M::BASE_PATH, M::routes())
    }
}
