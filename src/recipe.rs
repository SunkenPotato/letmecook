use crate::{DbPool, user::Authorization};
use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::StatusCode,
    response::Result,
};
use axum_extra::TypedHeader;
use chrono::{DateTime, Utc};
use log::error;
use serde::{Deserialize, Serialize};
use sqlx::{
    Postgres, QueryBuilder, Row,
    types::time::{OffsetDateTime, PrimitiveDateTime},
};

#[derive(Deserialize, Serialize, Debug)]
pub struct Ingredient {
    name: String,
    amount: f32,
    measurement: MeasurementType,
}

#[derive(Deserialize, Serialize, Debug)]
pub enum MeasurementType {
    #[serde(rename = "g")]
    Grams,
    #[serde(rename = "l")]
    Liters,
}

#[derive(Deserialize, Serialize)]
pub struct IncomingRecipe {
    name: String,
    description: Option<String>,
    cuisine: String,
    ingredients: Vec<Ingredient>,
    steps: Vec<String>,
    preparation_time: i32,
}

#[derive(Deserialize, Serialize)]
pub struct OutgoingRecipe {
    id: i32,
    name: String,
    description: Option<String>,
    cuisine: Option<String>,
    ingredients: Vec<Ingredient>,
    steps: Vec<String>,
    preparation_time: i32,
    created_at: chrono::DateTime<Utc>,
    author: i32,
    views: i32,
}

#[axum::debug_handler]
pub async fn create(
    Extension(db): DbPool,
    TypedHeader(auth): TypedHeader<Authorization>,
    Json(recipe): Json<IncomingRecipe>,
) -> (StatusCode, String) {
    match sqlx::query!(
        "insert into recipes (name, description, cuisine, ingredients, steps, author, preptime) values ($1, $2, $3, $4, $5, $6, $7) returning id",
        recipe.name,
        recipe.description,
        recipe.cuisine,
        serde_json::to_value(recipe.ingredients).unwrap(),
        &recipe.steps,
        auth.sub,
        recipe.preparation_time
    ).fetch_one(&db.0).await {
        Ok(v) => (StatusCode::CREATED, format!("{}", v.id)),
        Err(e) => {
            error!("Failed to create recipe in database: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create recipe".into())
        }
    }
}

#[axum::debug_handler]
pub async fn read(
    Extension(db): DbPool,
    Path(id): Path<i32>,
) -> Result<Json<OutgoingRecipe>, StatusCode> {
    match sqlx::query!(
        "select id, createdat as created_at, name, description, cuisine, ingredients, steps, author, views, preptime as preparation_time from recipes where id = $1 and deleted = false",
        id
    )
    .fetch_one(&db.0)
    .await
    .ok()
    {
        Some(v) => {
            let ingredients = match serde_json::from_value(v.ingredients) {
                Ok(v) => v,
                Err(e) => {
                    error!("Failed to parse database ingredient map: {e}");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            };
            let out = OutgoingRecipe {
                id: v.id,
                name: v.name,
                cuisine: v.cuisine,
                description: v.description,
                ingredients,
                steps: v.steps,
                preparation_time: v.preparation_time,
                created_at: DateTime::from_timestamp(v.created_at.as_utc().unix_timestamp(), 0).unwrap(),
                author: v.author, views: v.views
            };

            sqlx::query!("update recipes set views = views + 1 where id = $1", id).execute(&db.0).await.unwrap();

            Ok(Json(out))
        },
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn update(
    Extension(db): DbPool,
    Path(id): Path<i32>,
    TypedHeader(auth): TypedHeader<Authorization>,
    Json(recipe): Json<IncomingRecipe>,
) -> (StatusCode, &'static str) {
    let now = OffsetDateTime::now_utc();

    let queried_recipe = match sqlx::query!("select deleted, author from recipes where id = $1", id)
        .fetch_one(&db.0)
        .await
    {
        Ok(v) => v,
        Err(sqlx::Error::RowNotFound) => return (StatusCode::NOT_FOUND, "No such recipe"),
        Err(e) => {
            error!("Error while querying database: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
        }
    };

    if queried_recipe.deleted {
        return (StatusCode::NOT_FOUND, "No such recipe");
    }

    if queried_recipe.author != auth.sub {
        return (StatusCode::FORBIDDEN, "Forbidden");
    }

    match sqlx::query!(
        "update recipes set name = $1, description = $2, cuisine = $3, ingredients = $4, steps = $5, preptime = $6, editedat = $7 where id = $8",
        recipe.name,
        recipe.description,
        recipe.cuisine,
        serde_json::to_value(recipe.ingredients).unwrap(),
        &recipe.steps,
        recipe.preparation_time,
        PrimitiveDateTime::new(now.date(), now.time()),
        id
    ).execute(&db.0).await {
        Ok(_) => (StatusCode::OK, "Updated recipe"),
        Err(e) => {
            error!("Error while updating recipe: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
        }
    }
}

pub async fn delete(
    Path(id): Path<i32>,
    TypedHeader(auth): TypedHeader<Authorization>,
    Extension(db): DbPool,
) -> (StatusCode, &'static str) {
    let recipe = match sqlx::query!("select deleted, author from recipes where id = $1", id)
        .fetch_one(&db.0)
        .await
    {
        Ok(v) => v,
        Err(sqlx::Error::RowNotFound) => return (StatusCode::NOT_FOUND, "No such recipe"),
        Err(e) => {
            error!("Error while querying database: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
        }
    };

    if recipe.deleted {
        return (StatusCode::NOT_FOUND, "No such recipe");
    }

    if recipe.author != auth.sub {
        return (StatusCode::FORBIDDEN, "Forbidden");
    }

    match sqlx::query!("update recipes set deleted=true where id=$1", id)
        .execute(&db.0)
        .await
    {
        Ok(_) => (StatusCode::CREATED, "Deleted recipe"),
        Err(e) => {
            error!("Error while deleting recipe: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
        }
    }
}

pub async fn suggest() {}

#[derive(Deserialize)]
pub struct SearchQuery {
    page: Option<usize>,
    per_page: Option<usize>,
    author: Option<i32>,
    cuisine: Option<i32>,
    name: Option<String>,
    max_ptime: Option<i32>,
}

#[derive(Serialize)]
pub struct LightweightRecipe {
    id: i32,
    author: i32,
    name: String,
    description: Option<String>,
    createdat: DateTime<Utc>,
}

pub async fn search(
    Extension(db): DbPool,
    Query(search_query): Query<SearchQuery>,
) -> Result<Json<Vec<LightweightRecipe>>, StatusCode> {
    let mut query: QueryBuilder<'_, Postgres> = QueryBuilder::new(
        "select id, author, name, description, createdat from recipes where deleted=false",
    );

    let mut empty_query = true;

    if let Some(author) = search_query.author {
        query.push(" and author = ").push_bind(author);
        empty_query = false;
    }

    if let Some(cuisine) = search_query.cuisine {
        query
            .push(" and cuisine ilike '%' || ")
            .push_bind(cuisine)
            .push(" || '%'");
        empty_query = false;
    }

    if let Some(name) = search_query.name {
        query
            .push(" and name ilike '%' || ")
            .push_bind(name)
            .push(" || '%'");
        empty_query = false;
    }

    if let Some(max_ptime) = search_query.max_ptime {
        query.push(" and preptime <= ").push_bind(max_ptime);
        empty_query = false;
    }

    if empty_query {
        return Err(StatusCode::NO_CONTENT);
    }

    let mut results = match query.build().fetch_all(&db.0).await {
        Ok(v) => v,
        Err(e) => {
            error!("Error while querying database: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    dbg!(&results);

    if results.is_empty() {
        return Err(StatusCode::NO_CONTENT);
    }

    let per_page = search_query.per_page.unwrap_or(10);
    let page = search_query.page.unwrap_or_default();

    let start = (per_page * page).min(results.len().saturating_sub(per_page));
    let stop = (start + per_page).min(results.len());

    return Ok(Json(
        results
            .drain(start..stop)
            .map(|v| LightweightRecipe {
                id: v.get("id"),
                author: v.get("author"),
                name: v.get("name"),
                description: v.get("description"),
                createdat: DateTime::from_timestamp(
                    v.get::<'_, PrimitiveDateTime, _>("createdat")
                        .as_utc()
                        .unix_timestamp(),
                    0,
                )
                .unwrap(),
            })
            .collect(),
    ));
}
