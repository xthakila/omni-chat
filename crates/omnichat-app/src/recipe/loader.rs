use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::model::Recipe;

/// Returns the default directories to scan for Ferdium-compatible recipes.
pub fn default_recipe_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // 1. Bundled recipes next to the binary.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let bundled = parent.join("recipes");
            if bundled.is_dir() {
                dirs.push(bundled);
            }
            // Also check sibling of the project dir.
            if let Some(grandparent) = parent.parent() {
                let sibling = grandparent.join("recipes");
                if sibling.is_dir() {
                    dirs.push(sibling);
                }
            }
        }
    }

    // 2. User recipes in data dir.
    if let Some(data) = dirs_next::data_dir() {
        let user_recipes = data.join("omnichat").join("recipes");
        if user_recipes.is_dir() {
            dirs.push(user_recipes);
        }
    }

    // 3. Ferdium recipes if available.
    if let Some(data) = dirs_next::data_dir() {
        let ferdium_recipes = data.join("Ferdium").join("recipes");
        if ferdium_recipes.is_dir() {
            dirs.push(ferdium_recipes);
        }
    }

    // 4. Development: look in the workspace root's recipes/ folder.
    let cwd_recipes = PathBuf::from("recipes");
    if cwd_recipes.is_dir() {
        dirs.push(cwd_recipes.canonicalize().unwrap_or(cwd_recipes));
    }

    dirs
}

/// Scan directories for Ferdium-compatible recipe packages.
pub fn scan_recipes(dirs: &[PathBuf]) -> Vec<Recipe> {
    let mut recipes = Vec::new();
    let mut seen: HashMap<String, bool> = HashMap::new();

    for dir in dirs {
        info!("Scanning recipes in: {}", dir.display());
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("Cannot read recipe dir {}: {e}", dir.display());
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            match load_recipe(&path) {
                Ok(recipe) => {
                    if seen.contains_key(&recipe.id) {
                        debug!("Skipping duplicate recipe: {}", recipe.id);
                        continue;
                    }
                    seen.insert(recipe.id.clone(), true);
                    debug!("Loaded recipe: {} ({})", recipe.name, recipe.id);
                    recipes.push(recipe);
                }
                Err(e) => {
                    debug!("Skipping {}: {e}", path.display());
                }
            }
        }
    }

    info!("Total recipes loaded: {}", recipes.len());
    recipes
}

/// Load a single recipe from a directory containing package.json.
fn load_recipe(dir: &Path) -> Result<Recipe, String> {
    let pkg_path = dir.join("package.json");
    if !pkg_path.exists() {
        return Err("No package.json".into());
    }

    let content = std::fs::read_to_string(&pkg_path)
        .map_err(|e| format!("Cannot read {}: {e}", pkg_path.display()))?;

    let pkg: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid JSON: {e}"))?;

    let id = pkg["id"]
        .as_str()
        .or_else(|| pkg["name"].as_str())
        .ok_or("No id or name")?
        .to_string();

    let name = pkg["name"]
        .as_str()
        .unwrap_or(&id)
        .to_string();

    let version = pkg["version"].as_str().unwrap_or("0.0.0").to_string();
    let description = pkg["description"].as_str().unwrap_or("").to_string();

    let config = &pkg["config"];

    let service_url = config["serviceURL"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let has_direct_messages = config["hasDirectMessages"].as_bool().unwrap_or(true);
    let has_indirect_messages = config["hasIndirectMessages"].as_bool().unwrap_or(false);
    let has_notification_sound = config["hasNotificationSound"].as_bool().unwrap_or(false);
    let has_team_id = config["hasTeamId"].as_bool().unwrap_or(false);
    let has_custom_url = config["hasCustomUrl"].as_bool().unwrap_or(false);
    let has_hosted_option = config["hasHostedOption"].as_bool().unwrap_or(false);
    let url_input_prefix = config["urlInputPrefix"].as_str().unwrap_or("").to_string();
    let url_input_suffix = config["urlInputSuffix"].as_str().unwrap_or("").to_string();
    let disable_web_security = config["disablewebsecurity"].as_bool().unwrap_or(false);
    let message = config["message"].as_str().unwrap_or("").to_string();

    // Load webview.js if it exists.
    let webview_js_path = dir.join("webview.js");
    let webview_js = if webview_js_path.exists() {
        std::fs::read_to_string(&webview_js_path).ok()
    } else {
        None
    };

    // Load darkmode.css if it exists.
    let darkmode_css_path = dir.join("darkmode.css");
    let darkmode_css = if darkmode_css_path.exists() {
        std::fs::read_to_string(&darkmode_css_path).ok()
    } else {
        None
    };

    Ok(Recipe {
        id,
        name,
        version,
        description,
        path: dir.to_string_lossy().to_string(),
        service_url,
        has_direct_messages,
        has_indirect_messages,
        has_notification_sound,
        has_team_id,
        has_custom_url,
        has_hosted_option,
        url_input_prefix,
        url_input_suffix,
        disable_web_security,
        message,
        webview_js,
        darkmode_css,
    })
}
