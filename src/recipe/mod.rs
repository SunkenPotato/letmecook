mod api;

use rocket::routes;
use serde::{Deserialize, Serialize};

use crate::utils::Module;

const RECIPE_FOLDER_PATH: &str = "recipes/";

pub struct RecipeModule;

impl Module for RecipeModule {
    const BASE_PATH: &str = "/recipe";

    fn routes() -> Vec<rocket::Route> {
        routes![api::create_recipe]
    }
}

#[derive(Serialize, Deserialize)]
struct Recipe {
    name: String,
    origin: Option<String>,
    #[serde(rename = "preparationTime")]
    preparation_time: u64, // in s
    #[serde(rename = "cookingTime")]
    cooking_time: u64, // in s
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

#[cfg(test)]
mod tests {
    use super::Recipe;

    #[test]
    fn parse_recipe() {
        let input = r#"
            {
                "name": "dev's delight",
                "origin": "DE",
                "preparationTime": 1200,
                "cookingTime": 1200,
                "ingredients": [
                    {
                        "name": "Tomato",
                        "quantity": { "n": 5 }
                    }
                ],
                "steps": ["...."]
            }

            "#;

        serde_json::from_str::<Recipe>(&input).unwrap();
    }
}
