use log::error;
use rocket::{
    delete, get,
    http::Status,
    post,
    serde::json::Json,
    tokio::{fs::File, io::AsyncWriteExt},
};
use rocket_db_pools::Connection;

use crate::{
    AppDB,
    recipe::RECIPE_FOLDER_PATH,
    user::auth::{Authorization, validate_auth_key},
};

use super::Recipe;

// TODO: possibly hash recipe to check for duplicates...?
// use UserError instead, but make it generic
#[post("/", data = "<recipe>")]
pub(crate) async fn create_recipe(
    auth: Authorization,
    mut db: Connection<AppDB>,
    recipe: Json<Recipe>,
) -> Status {
    let claims = validate_auth_key(&auth.0).expect("validated token");

    match sqlx::query!(
        "select exists(select (1) from users where id = $1 and deleted = false)",
        claims.claims.sub
    )
    .fetch_one(&mut **db)
    .await
    .unwrap()
    .exists
    .unwrap()
    {
        false => return Status::NotFound, // use UserError here for more descriptive error
        _ => (),
    };

    let created_recipe = match sqlx::query!(
        "insert into recipes (name, author) values ($1, $2) returning id",
        recipe.name,
        claims.claims.sub
    )
    .fetch_one(&mut **db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            error!("Error while trying to create recipe record: {e}");

            return Status::InternalServerError;
        }
    };

    let mut file = match File::create(format!("{RECIPE_FOLDER_PATH}{}", created_recipe.id)).await {
        Ok(v) => v,
        Err(e) => {
            error!("Could not open file: {e}");

            return Status::InternalServerError;
        }
    };

    match file.write(&serde_json::to_vec(&*recipe).unwrap()).await {
        Err(e) => {
            error!("Could not write recipe to file: {e}");
            return Status::InternalServerError;
        }
        _ => (),
    };

    Status::NoContent
}

// use UserError
#[get("/<id>")]
async fn get_recipe(mut db: Connection<AppDB>, id: i64) -> Result<Json<Recipe>, Status> {
    todo!()
}

#[delete("/<id>")]
async fn delete_recipe(mut db: Connection<AppDB>, id: i64) -> Status {
    todo!()
}
