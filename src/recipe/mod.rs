mod api;

use rocket::routes;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;

use crate::utils::Module;
use sqlx::Row;

const RECIPE_FOLDER_PATH: &str = "storage/recipes";

fn recipe_path<T: std::fmt::Display>(id: T) -> String {
    format!("{}/{}", RECIPE_FOLDER_PATH, id)
}

fn recipe_image_path<T: std::fmt::Display>(name: T) -> String {
    format!("storage/images/{}", name)
}

fn create_image_url(file_uuid: impl AsRef<str>) -> String {
    format!("/cdn/images/{}", file_uuid.as_ref())
}

pub struct RecipeModule;

impl Module for RecipeModule {
    const BASE_PATH: &str = "/recipe";

    fn routes() -> Vec<rocket::Route> {
        routes![
            api::create_recipe,
            api::get_recipe,
            api::delete_recipe,
            api::search
        ]
    }
}

#[derive(Serialize, Deserialize)]
struct RequestRecipe {
    meta: RequestRecipeMeta,
    recipe: AbsoluteRecipe,
}

#[derive(Serialize, Deserialize)]
struct ResponseRecipe {
    meta: ResponseRecipeMeta,
    recipe: AbsoluteRecipe,
}

impl TryFrom<PgRow> for ResponseRecipeMeta {
    type Error = sqlx::Error;

    fn try_from(value: PgRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.try_get("id")?,
            name: value.try_get("name")?,
            description: value.try_get("description")?,
            author: value.try_get("author")?,
            image: value.try_get("image")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct RequestRecipeMeta {
    name: String,
    description: String,
}

#[derive(Serialize, Deserialize)]
struct ResponseRecipeMeta {
    name: String,
    description: String,
    author: i32,
    id: i32,
    image: String,
}

#[derive(Serialize, Deserialize)]
struct AbsoluteRecipe {
    #[serde(rename = "preparationTime")]
    preparation_time: u64,
    #[serde(rename = "cookingTime")]
    cooking_time: u64,
    ingredients: Vec<Ingredient>,
    steps: Vec<RecipeStep>,
}

#[derive(Serialize, Deserialize)]
enum Quantity {
    #[serde(rename = "l")]
    Liters(f32),
    #[serde(rename = "g")]
    Grams(u32),
    #[serde(rename = "n")]
    Count(u32),
}

#[derive(Serialize, Deserialize)]
struct Ingredient {
    name: String,
    quantity: Quantity,
}

#[derive(Serialize, Deserialize)]
struct RecipeStep(String); // this isn't worth parsing any further
