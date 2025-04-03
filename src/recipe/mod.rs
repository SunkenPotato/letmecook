mod api;

use rocket::routes;
use serde::{Deserialize, Serialize};

use crate::utils::Module;

const RECIPE_FOLDER_PATH: &str = "recipes/";

pub struct RecipeModule;

impl Module for RecipeModule {
    const BASE_PATH: &str = "/recipe";

    fn routes() -> Vec<rocket::Route> {
        routes![api::create_recipe, api::get_recipe]
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
