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
use sqlx::{Postgres, QueryBuilder, Row};
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
        false => return Status::NotFound,
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

#[get("/<id>")]
pub async fn get_recipe(
    mut db: Connection<AppDB>,
    id: i32,
) -> Result<Json<ResponseRecipe>, Status> {
    let mut query = QueryBuilder::<Postgres>::new("select * from recipes where deleted = false");
    query.push(" and id = ").push_bind(id);

    let query = query.build();

    let row = query
        .fetch_one(&mut **db)
        .await
        .inspect_err(|e| error!("Error while trying to get recipes: {e}"))
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => Status::NotFound,
            _ => Status::InternalServerError,
        })?;

    let recipe_id = row.get::<i32, &str>("id");
    let meta = row.try_into().expect("row should have expected fields");

    let recipe = read_recipe(format!("{RECIPE_FOLDER_PATH}{}", recipe_id))
        .await
        .inspect_err(|e| error!("Error while trying to read recipe: {e}"))
        .map_err(|_| Status::InternalServerError)?;

    Ok(Json(ResponseRecipe { meta, recipe }))
}

#[get("/search?<name>&<description>&<author>&<limit>")]
pub async fn search(
    name: Option<String>,
    description: Option<String>,
    author: Option<String>,
    limit: Option<i16>,
    mut db: Connection<AppDB>,
) -> Result<Json<Vec<ResponseRecipe>>, Status> {
    if let Some(v) = limit {
        if v > 255 {
            return Err(Status::PayloadTooLarge);
        }
    }

    let mut query = QueryBuilder::<Postgres>::new(
        "select recipes.* from recipes join users on recipes.author = users.id where true",
    );

    if let Some(v) = name {
        query
            .push(" and recipes.name like ")
            .push_bind(format!("%{v}%"));
    }

    if let Some(v) = description {
        query
            .push(" and recipes.description like ")
            .push_bind(format!("%{v}%"));
    }

    if let Some(v) = author {
        query
            .push(" and users.name like ")
            .push_bind(format!("%{v}%"));
    }

    query.push(" limit ").push_bind(limit.unwrap_or(100));

    let rows = query
        .build()
        .fetch_all(&mut **db)
        .await
        .inspect_err(|e| error!("Error while trying to get recipes: {e}"))
        .map_err(|_| Status::InternalServerError)?;

    let metas = rows
        .into_iter()
        .map(|e| {
            TryInto::<ResponseRecipeMeta>::try_into(e).expect("row should have expected fields")
        })
        .collect::<Vec<_>>();

    let mut recipes = vec![];

    for meta in metas {
        let recipe = read_recipe(format!("{RECIPE_FOLDER_PATH}{}", meta.id))
            .await
            .inspect_err(|e| error!("Error while trying to read recipe {}: {e}", meta.id))
            .map_err(|_| Status::InternalServerError)?;
        recipes.push(ResponseRecipe { meta, recipe });
    }

    Ok(Json(recipes))
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
