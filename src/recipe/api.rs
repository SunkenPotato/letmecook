use std::path::Path;

use log::error;
use rocket::{
    delete, get,
    http::Status,
    post,
    serde::json::Json,
    tokio::{
        fs::{self, File},
        io::{AsyncReadExt, AsyncWriteExt},
    },
};
use rocket_db_pools::Connection;
use sqlx::{Postgres, QueryBuilder, Row, postgres::PgRow};
use thiserror::Error;

use crate::{
    AppDB,
    recipe::{RECIPE_FOLDER_PATH, ResponseRecipeMeta},
    user::{REQUEST_GUARD_INCONSISTENCY, auth::Authorization},
};

use super::{AbsoluteRecipe, RequestRecipe, ResponseRecipe};

#[derive(Debug, Error)]
enum RecipeReadError {
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("I/O Error: {0}")]
    IoError(#[from] std::io::Error),
}

// TODO: possibly hash recipe to check for duplicates...?
// use UserError instead, but make it generic
#[post("/", data = "<recipe>")]
pub async fn create_recipe(
    auth: Authorization,
    mut db: Connection<AppDB>,
    recipe: Json<RequestRecipe>,
) -> Status {
    let claims = auth.validate().expect("validated token");

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
        "insert into recipes (name, author, description) values ($1, $2, $3) returning id",
        recipe.meta.name,
        claims.claims.sub,
        recipe.meta.description
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

    match file
        .write(&serde_json::to_vec(&recipe.recipe).unwrap())
        .await
    {
        Err(e) => {
            error!("Could not write recipe to file: {e}");
            return Status::InternalServerError;
        }
        _ => (),
    };

    Status::NoContent
}

// use some specialized struct
#[get("/<id>")]
pub async fn get_recipe(
    mut db: Connection<AppDB>,
    id: i32,
) -> Result<Json<Vec<ResponseRecipe>>, Status> {
    let mut query = QueryBuilder::<Postgres>::new("select * from recipes where deleted = false");
    query.push(" and id = ").push_bind(id);

    let query = query.build();

    let recipe_metas = map_rows_to_recipe_meta(
        query
            .fetch_all(&mut **db)
            .await
            .inspect_err(|e| error!("Error while trying to get recipes: {e}"))
            .map_err(|_| Status::InternalServerError)?
            .into_iter(),
    );

    let mut recipes = vec![];

    for (id, meta) in recipe_metas {
        let recipe = read_recipe(format!("{RECIPE_FOLDER_PATH}{}", id))
            .await
            .inspect_err(|e| error!("Error while trying to read recipe JSON: {e}"))
            .map_err(|_| Status::InternalServerError)?;

        recipes.push(ResponseRecipe { meta, recipe })
    }

    Ok(Json(recipes))
}

fn map_rows_to_recipe_meta<I: Iterator<Item = PgRow>>(
    iter: I,
) -> impl Iterator<Item = (i32, ResponseRecipeMeta)> {
    iter.map(|e| {
        (
            e.get("id"),
            ResponseRecipeMeta {
                name: e.get("name"),
                author: e.get("author"),
                description: e.get("description"),
            },
        )
    })
}

async fn read_recipe(path: impl AsRef<Path>) -> Result<AbsoluteRecipe, RecipeReadError> {
    let mut file = File::open(path)
        .await
        .map_err(|e| RecipeReadError::IoError(e))?;
    let mut buf = String::new();

    file.read_to_string(&mut buf).await?;

    Ok(serde_json::from_str(&buf).map_err(|e| RecipeReadError::JsonError(e))?)
}

#[delete("/<recipe_id>")]
pub async fn delete_recipe(
    mut db: Connection<AppDB>,
    recipe_id: i32,
    auth: Authorization,
) -> Status {
    let claims = match auth.validate() {
        Ok(v) => v,
        Err(e) => {
            error!("{REQUEST_GUARD_INCONSISTENCY} {e}");

            return Status::InternalServerError;
        }
    };

    let user_id = claims.claims.sub;

    match sqlx::query!(
        "update recipes set deleted = true from users where recipes.author = users.id and users.id = $1 and recipes.id = $2",
        user_id,
        recipe_id
    )
    .execute(&mut **db)
    .await
    {
        Ok(v) => match v.rows_affected() == 1 {
            true => {
                let _ = fs::remove_file(format!("{RECIPE_FOLDER_PATH}{recipe_id}")).await;
                Status::NoContent
            },
            false => Status::NotFound,
        },
        Err(e) => {
            error!("Error occurred while trying to pseudo-delete recipe: {e}");

            Status::InternalServerError
        }
    }
}
