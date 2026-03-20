use std::path::Path;

use serde::{Deserialize, Serialize};

/// Project metadata stored in `.latexlabs/project.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub trashed: bool,
    #[serde(default = "default_timestamp")]
    pub created_at: String,
    #[serde(default = "default_timestamp")]
    pub updated_at: String,
}

fn default_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Import a ZIP project by extracting it to the target directory.
///
/// Returns the path to the extracted project.
#[tauri::command]
pub async fn import_zip_project(zip_path: String, target_dir: String) -> Result<String, String> {
    let zip_file = Path::new(&zip_path);
    if !zip_file.exists() {
        return Err(format!("ZIP file not found: {}", zip_path));
    }

    let target = Path::new(&target_dir);

    let zip_path_clone = zip_path.clone();
    let target_clone = target.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&zip_path_clone)
            .map_err(|e| format!("Failed to open ZIP file: {}", e))?;

        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Failed to read ZIP archive: {}", e))?;

        // Determine if the ZIP has a single root directory
        let has_root_dir = detect_zip_root_dir(&mut archive);

        std::fs::create_dir_all(&target_clone)
            .map_err(|e| format!("Failed to create target directory: {}", e))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;

            let raw_name = file.name().to_string();

            // If the ZIP has a single root dir, strip it
            let rel_path = if let Some(ref root) = has_root_dir {
                if let Some(stripped) = raw_name.strip_prefix(root) {
                    stripped.to_string()
                } else {
                    raw_name.clone()
                }
            } else {
                raw_name.clone()
            };

            if rel_path.is_empty() {
                continue;
            }

            // Security: reject paths with .. components
            if rel_path.contains("..") {
                continue;
            }

            let out_path = target_clone.join(&rel_path);

            if file.is_dir() {
                let _ = std::fs::create_dir_all(&out_path);
            } else {
                if let Some(parent) = out_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let mut outfile = std::fs::File::create(&out_path)
                    .map_err(|e| format!("Failed to create file {}: {}", rel_path, e))?;
                std::io::copy(&mut file, &mut outfile)
                    .map_err(|e| format!("Failed to extract {}: {}", rel_path, e))?;
            }
        }

        Ok::<String, String>(target_clone.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("Extract task panicked: {}", e))?
}

/// Detect if a ZIP archive has a single root directory containing all files.
fn detect_zip_root_dir(archive: &mut zip::ZipArchive<std::fs::File>) -> Option<String> {
    if archive.is_empty() {
        return None;
    }

    let mut root: Option<String> = None;

    for i in 0..archive.len() {
        let file = match archive.by_index(i) {
            Ok(f) => f,
            Err(_) => return None,
        };
        let name = file.name();

        // Get the first path component
        let first_component = name.split('/').next().unwrap_or("");
        if first_component.is_empty() {
            continue;
        }

        match &root {
            None => root = Some(format!("{}/", first_component)),
            Some(r) => {
                if !name.starts_with(r.as_str()) {
                    return None; // Multiple root entries
                }
            }
        }
    }

    root
}

/// Write or update project metadata.
#[tauri::command]
pub async fn update_project_meta(
    project_dir: String,
    meta: ProjectMeta,
) -> Result<(), String> {
    let meta_dir = Path::new(&project_dir).join(".latexlabs");
    std::fs::create_dir_all(&meta_dir)
        .map_err(|e| format!("Failed to create .latexlabs directory: {}", e))?;

    let meta_path = meta_dir.join("project.json");

    // Update the updated_at timestamp
    let mut meta = meta;
    meta.updated_at = chrono::Utc::now().to_rfc3339();

    let json = serde_json::to_string_pretty(&meta)
        .map_err(|e| format!("Failed to serialize project meta: {}", e))?;

    tokio::fs::write(&meta_path, json)
        .await
        .map_err(|e| format!("Failed to write project meta: {}", e))?;

    Ok(())
}

/// Read project metadata.
#[tauri::command]
pub async fn get_project_meta(project_dir: String) -> Result<ProjectMeta, String> {
    let meta_path = Path::new(&project_dir)
        .join(".latexlabs")
        .join("project.json");

    if !meta_path.exists() {
        // Return default metadata based on the directory name
        let name = Path::new(&project_dir)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled Project")
            .to_string();

        return Ok(ProjectMeta {
            name,
            tags: Vec::new(),
            archived: false,
            trashed: false,
            created_at: default_timestamp(),
            updated_at: default_timestamp(),
        });
    }

    let content = tokio::fs::read_to_string(&meta_path)
        .await
        .map_err(|e| format!("Failed to read project meta: {}", e))?;

    let meta: ProjectMeta = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse project meta: {}", e))?;

    Ok(meta)
}
