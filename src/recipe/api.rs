use std::path::Path;

use log::{error, info};
use rocket::{
    Data, delete, get,
    http::{ContentType, Status},
    post,
    serde::json::Json,
    tokio::{
        fs::{self, File},
        io::AsyncReadExt,
    },
};
use rocket_db_pools::Connection;
use rocket_multipart_form_data::{
    MultipartFormData, MultipartFormDataField, MultipartFormDataOptions,
};
use sqlx::{Executor, Postgres, QueryBuilder, Row};
use thiserror::Error;

use crate::{
    AppDB,
    recipe::ResponseRecipeMeta,
    user::{REQUEST_GUARD_INCONSISTENCY, auth::Authorization},
};

use super::{
    AbsoluteRecipe, RequestRecipe, ResponseRecipe, create_image_url, recipe_image_path, recipe_path,
};

#[derive(Debug, Error)]
enum RecipeReadError {
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("I/O Error: {0}")]
    IoError(#[from] std::io::Error),
}

async fn rollback_recipe_insertion<'e, T>(db: &mut T, id: i32)
where
    for<'exec> &'exec mut T: Executor<'exec, Database = Postgres>,
{
    let query = format!("DELETE FROM recipes WHERE id = {}", id);
    match sqlx::query(&query).execute(db).await {
        Ok(_) => info!("Successfully rolled back recipe insertion"),
        Err(e) => error!("Failed to rollback recipe insertion: {}", e),
    }
}

// TODO: possibly hash recipe to check for duplicates...?
// use UserError instead, but make it generic
#[post("/", data = "<data>")]
pub async fn create_recipe<'r>(
    auth: Authorization,
    mut db: Connection<AppDB>,
    data: Data<'_>,
    content_type: &ContentType,
) -> Result<Json<ResponseRecipe>, (Status, &'r str)> {
    // get id
    let claims = auth.validate().expect("Failed to validate authorization");
    let author = claims.claims.sub;

    // multipart form data options with field "image" (file) and "recipe" (text (jsonz))
    let opts = MultipartFormDataOptions::with_multipart_form_data_fields(vec![
        MultipartFormDataField::file("image")
            .content_type_by_string(Some(rocket_multipart_form_data::mime::IMAGE_PNG))
            .unwrap(),
        MultipartFormDataField::text("recipe")
            .content_type(Some(rocket_multipart_form_data::mime::APPLICATION_JSON)),
    ]);

    let mut data = match MultipartFormData::parse(content_type, data, opts).await {
        Ok(v) => v,
        Err(e) => {
            info!("{e}");
            return Err((Status::BadRequest, "Could not parse multipart form data"));
        }
    };

    let image = &match data.files.remove("image") {
        Some(image) => image,
        None => return Err((Status::BadRequest, "Missing 'image' in multipart form data")),
    }[0];

    let recipe: RequestRecipe = match data.texts.remove("recipe") {
        Some(v) => match serde_json::from_str(&v[0].text) {
            Ok(v) => v,
            Err(_) => return Err((Status::BadRequest, "Invalid JSON")),
        },
        None => {
            return Err((
                Status::BadRequest,
                "Missing 'recipe' in multipart form data",
            ));
        }
    };

    let file_uuid = uuid::Uuid::new_v4();

    let record = match sqlx::query!("insert into recipes (name, author, description, image) values ($1, $2, $3, $4) returning *",
        recipe.meta.name, author, recipe.meta.description, file_uuid.to_string())
        .fetch_one(&mut **db)
        .await {
        Ok(v) => v,
        Err(e) => {
            error!("Could not insert create record for recipe: {e}");

            return Err((Status::InternalServerError, "Internal Server Error"));
        },
    };

    let serialized = serde_json::to_string(&recipe.recipe).unwrap();

    let recipe_path = recipe_path(file_uuid);
    match fs::write(&recipe_path, serialized).await {
        Err(e) => {
            error!("Could not write recipe file: {e}");
            rollback_recipe_insertion(&mut **db, record.id).await;

            return Err((Status::InternalServerError, "Internal Server Error"));
        }
        _ => (),
    };

    if let Err(e) = fs::copy(&image.path, recipe_image_path(file_uuid)).await {
        error!("Could not write recipe image file: {e}");
        rollback_recipe_insertion(&mut **db, record.id).await;
        if let Err(e) = fs::remove_file(recipe_path).await {
            error!("Could not remove recipe file: {e}");
        }

        return Err((Status::InternalServerError, "Internal Server Error"));
    }

    let meta = ResponseRecipeMeta {
        id: record.id,
        name: record.name,
        author: record.author,
        description: record.description,
        image: create_image_url(record.image),
    };

    Ok(Json(ResponseRecipe {
        meta,
        recipe: recipe.recipe,
    }))
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

    let recipe = read_recipe(recipe_path(recipe_id))
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
        let recipe = read_recipe(recipe_path(meta.id))
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
                let _ = fs::remove_file(recipe_path(recipe_id)).await;
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
