use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use urlencoding::encode as urlencode;

use crate::llm::{self, LlmConfig, Message, ToolCall, ToolDefinition, ToolFunction};

// --- Public types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub reply: String,
    pub suggestion: Option<String>,
    pub patches: Vec<Patch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentProgressEvent {
    status: String,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentToolCallEvent {
    tool: String,
    arguments: String,
    result_preview: String,
}

// --- Tool definitions ---

fn build_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "read_file".to_string(),
                description: "Read a UTF-8 file from the project. Returns up to 20000 chars."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path relative to project root" }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "list_files".to_string(),
                description: "List files under a directory in the project.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "dir": { "type": "string", "description": "Relative directory path (optional, defaults to root)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "propose_patch".to_string(),
                description:
                    "Propose a full file rewrite. Does NOT write to disk. Returns a patch for user confirmation."
                        .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path relative to project root" },
                        "content": { "type": "string", "description": "New file content" }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_compile_log".to_string(),
                description: "Return the latest LaTeX compile log from the project.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "arxiv_search".to_string(),
                description: "Search arXiv papers by query. Returns titles, abstracts, and IDs."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "max_results": { "type": "integer", "description": "Max results (1-10, default 5)" }
                    },
                    "required": ["query"]
                }),
            },
        },
    ]
}

// --- Tool execution ---

async fn execute_tool(
    tool_name: &str,
    arguments: &str,
    project_dir: &Path,
    patches: &mut Vec<Patch>,
) -> Result<String, String> {
    let args: serde_json::Value = serde_json::from_str(arguments)
        .map_err(|e| format!("Invalid tool arguments: {}", e))?;

    match tool_name {
        "read_file" => {
            let rel_path = args["path"]
                .as_str()
                .ok_or("Missing 'path' argument")?;
            let abs_path = safe_join(project_dir, rel_path)?;
            let content = tokio::fs::read_to_string(&abs_path)
                .await
                .map_err(|e| format!("Failed to read {}: {}", rel_path, e))?;
            // Truncate to 20000 chars like the JS version
            Ok(content.chars().take(20000).collect())
        }
        "list_files" => {
            let dir = args["dir"].as_str().unwrap_or("");
            let root = if dir.is_empty() {
                project_dir.to_path_buf()
            } else {
                safe_join(project_dir, dir)?
            };
            let files = list_files_recursive(&root, "").await?;
            Ok(serde_json::json!({ "files": files }).to_string())
        }
        "propose_patch" => {
            let rel_path = args["path"]
                .as_str()
                .ok_or("Missing 'path' argument")?;
            let new_content = args["content"]
                .as_str()
                .ok_or("Missing 'content' argument")?
                .to_string();
            let abs_path = safe_join(project_dir, rel_path)?;
            let old_content = tokio::fs::read_to_string(&abs_path)
                .await
                .unwrap_or_default();

            patches.push(Patch {
                file_path: rel_path.to_string(),
                old_content,
                new_content,
            });

            Ok(format!(
                "Patch prepared for {}. Awaiting user confirmation.",
                rel_path
            ))
        }
        "get_compile_log" => {
            // Try to read the latest compile log from the project's build directory
            let build_dir = project_dir.join(".prism").join("build");
            let mut log_content = String::new();

            if build_dir.exists() {
                // Find any .log file in the build directory
                if let Ok(entries) = std::fs::read_dir(&build_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("log") {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                log_content = content;
                                break;
                            }
                        }
                    }
                }
            }

            if log_content.is_empty() {
                Ok("No compile log found.".to_string())
            } else {
                // Return last 8000 chars
                let start = log_content.len().saturating_sub(8000);
                Ok(log_content[start..].to_string())
            }
        }
        "arxiv_search" => {
            let query = args["query"]
                .as_str()
                .ok_or("Missing 'query' argument")?;
            let max_results = args["max_results"]
                .as_u64()
                .unwrap_or(5)
                .min(10)
                .max(1);

            let url = format!(
                "https://export.arxiv.org/api/query?search_query=all:{}&start=0&max_results={}",
                urlencode(query),
                max_results
            );

            let client = reqwest::Client::new();
            let resp = client
                .get(&url)
                .header("User-Agent", "latexlabs/1.0")
                .send()
                .await
                .map_err(|e| format!("arXiv search failed: {}", e))?;

            if !resp.status().is_success() {
                return Err(format!("arXiv returned status {}", resp.status()));
            }

            let xml = resp
                .text()
                .await
                .map_err(|e| format!("Failed to read arXiv response: {}", e))?;

            Ok(parse_arxiv_xml(&xml))
        }
        _ => Err(format!("Unknown tool: {}", tool_name)),
    }
}

// --- Agent loop ---

const MAX_ITERATIONS: usize = 15;

/// Run the tool-calling agent loop.
///
/// Emits events: "agent-progress", "agent-tool-call", "agent-complete"
#[tauri::command]
pub async fn run_agent(
    app: tauri::AppHandle,
    task: String,
    prompt: String,
    selection: Option<String>,
    content: Option<String>,
    project_dir: String,
    main_file: String,
    llm_config: LlmConfig,
) -> Result<AgentResult, String> {
    let project_path = Path::new(&project_dir);
    if !project_path.exists() {
        return Err(format!("Project directory not found: {}", project_dir));
    }

    let _ = app.emit(
        "agent-progress",
        AgentProgressEvent {
            status: "starting".to_string(),
            message: "Initializing agent...".to_string(),
        },
    );

    let system_prompt = [
        "You are a LaTeX paper assistant for LaTeX-Labs.",
        "You can read files and propose patches via tools, and you may call tools multiple times.",
        "If a request affects multiple files (e.g., sections + bib), inspect and update all relevant files.",
        "You can use arxiv_search to find papers.",
        "Never assume writes are applied; use propose_patch and wait for user confirmation.",
        "Be concise. Provide a short summary in the final response.",
    ]
    .join(" ");

    // Build user input
    let mut user_parts = vec![format!("Task: {}", if task.is_empty() { "polish" } else { &task })];
    if !main_file.is_empty() {
        user_parts.push(format!("Active file: {}", main_file));
    }
    if !prompt.is_empty() {
        user_parts.push(format!("User prompt: {}", prompt));
    }
    if let Some(ref sel) = selection {
        if !sel.is_empty() {
            user_parts.push(format!("Selection:\n{}", sel));
        }
    }
    if let Some(ref ctx) = content {
        if !ctx.is_empty() {
            // Truncate large content
            let truncated: String = ctx.chars().take(10000).collect();
            user_parts.push(format!("File content:\n{}", truncated));
        }
    }

    let tools = build_tool_definitions();
    let mut patches: Vec<Patch> = Vec::new();

    // Initialize conversation
    let mut messages: Vec<Message> = vec![
        Message {
            role: "system".to_string(),
            content: serde_json::Value::String(system_prompt.to_string()),
        },
        Message {
            role: "user".to_string(),
            content: serde_json::Value::String(user_parts.join("\n\n")),
        },
    ];

    for iteration in 0..MAX_ITERATIONS {
        let _ = app.emit(
            "agent-progress",
            AgentProgressEvent {
                status: "thinking".to_string(),
                message: format!("Agent iteration {}/{}", iteration + 1, MAX_ITERATIONS),
            },
        );

        let assistant_msg =
            llm::call_llm_with_tools(&llm_config, messages.clone(), &tools, 0.2).await?;

        // Check for tool calls
        let tool_calls: Vec<ToolCall> = if let Some(tc) = assistant_msg.get("tool_calls") {
            serde_json::from_value(tc.clone()).unwrap_or_default()
        } else {
            vec![]
        };

        // Add assistant message to conversation
        messages.push(Message {
            role: "assistant".to_string(),
            content: assistant_msg.clone(),
        });

        if tool_calls.is_empty() {
            // Agent is done — extract final reply
            let reply = assistant_msg["content"]
                .as_str()
                .unwrap_or("")
                .to_string();

            let suggestion = if !patches.is_empty() {
                Some(format!(
                    "{} file(s) modified: {}",
                    patches.len(),
                    patches
                        .iter()
                        .map(|p| p.file_path.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            } else {
                None
            };

            let _ = app.emit(
                "agent-complete",
                serde_json::json!({
                    "reply": reply,
                    "patch_count": patches.len(),
                }),
            );

            return Ok(AgentResult {
                reply,
                suggestion,
                patches,
            });
        }

        // Execute each tool call
        for tc in &tool_calls {
            let _ = app.emit(
                "agent-tool-call",
                AgentToolCallEvent {
                    tool: tc.function.name.clone(),
                    arguments: tc.function.arguments.clone(),
                    result_preview: String::new(),
                },
            );

            let result =
                execute_tool(&tc.function.name, &tc.function.arguments, project_path, &mut patches)
                    .await;

            let tool_result = match result {
                Ok(output) => output,
                Err(err) => format!("Error: {}", err),
            };

            // Add tool result to conversation
            messages.push(Message {
                role: "tool".to_string(),
                content: serde_json::json!({
                    "tool_call_id": tc.id,
                    "content": tool_result,
                }),
            });
        }
    }

    // Max iterations reached
    let _ = app.emit(
        "agent-complete",
        serde_json::json!({
            "reply": "Agent reached maximum iterations.",
            "patch_count": patches.len(),
        }),
    );

    Ok(AgentResult {
        reply: "Agent reached maximum iterations without completing.".to_string(),
        suggestion: None,
        patches,
    })
}

// --- Utility functions ---

/// Safely join a base directory with a relative path, preventing path traversal.
fn safe_join(base: &Path, relative: &str) -> Result<PathBuf, String> {
    let cleaned = relative.replace('\\', "/");
    let rel = Path::new(&cleaned);

    // Reject absolute paths and paths with ..
    if rel.is_absolute() {
        return Err(format!("Absolute paths are not allowed: {}", relative));
    }
    for component in rel.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(format!("Path traversal is not allowed: {}", relative));
        }
    }

    let joined = base.join(rel);
    Ok(joined)
}

/// Recursively list files in a directory, returning relative paths.
async fn list_files_recursive(root: &Path, prefix: &str) -> Result<Vec<String>, String> {
    let mut files = Vec::new();

    let mut entries = tokio::fs::read_dir(root)
        .await
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read entry: {}", e))?
    {
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files/directories
        if name.starts_with('.') {
            continue;
        }

        let rel_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", prefix, name)
        };

        let file_type = entry
            .file_type()
            .await
            .map_err(|e| format!("Failed to get file type: {}", e))?;

        if file_type.is_dir() {
            let sub_files = list_files_recursive(&entry.path(), &rel_path).await?;
            files.extend(sub_files);
        } else {
            files.push(rel_path);
        }
    }

    Ok(files)
}

/// Parse arXiv Atom XML response into a simplified JSON string.
/// Uses a simple regex-based parser to avoid adding an XML dependency.
fn parse_arxiv_xml(xml: &str) -> String {
    let mut papers: Vec<serde_json::Value> = Vec::new();

    // Split on <entry> tags
    for entry_block in xml.split("<entry>").skip(1) {
        let entry_end = entry_block.find("</entry>").unwrap_or(entry_block.len());
        let entry = &entry_block[..entry_end];

        let title = extract_xml_tag(entry, "title")
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();

        let summary = extract_xml_tag(entry, "summary")
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();

        let id = extract_xml_tag(entry, "id").unwrap_or_default();
        let arxiv_id = id.split('/').last().unwrap_or("").to_string();

        // Extract author names
        let mut authors: Vec<String> = Vec::new();
        for author_block in entry.split("<author>").skip(1) {
            if let Some(name) = extract_xml_tag(author_block, "name") {
                authors.push(name);
            }
        }

        papers.push(serde_json::json!({
            "title": title,
            "abstract": summary,
            "authors": authors,
            "url": id,
            "arxivId": arxiv_id,
        }));
    }

    serde_json::json!({ "papers": papers }).to_string()
}

/// Extract the text content of an XML tag.
fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start_pos = xml.find(&open)?;
    // Find the end of the opening tag (could have attributes)
    let content_start = xml[start_pos..].find('>')? + start_pos + 1;
    let end_pos = xml[content_start..].find(&close)? + content_start;
    Some(xml[content_start..end_pos].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_join_normal() {
        let base = Path::new("/project");
        let result = safe_join(base, "src/main.tex").unwrap();
        assert_eq!(result, PathBuf::from("/project/src/main.tex"));
    }

    #[test]
    fn test_safe_join_rejects_traversal() {
        let base = Path::new("/project");
        assert!(safe_join(base, "../etc/passwd").is_err());
    }

    #[test]
    fn test_safe_join_rejects_absolute() {
        let base = Path::new("/project");
        assert!(safe_join(base, "/etc/passwd").is_err());
    }

    #[test]
    fn test_extract_xml_tag() {
        let xml = "<title>Some Title</title><summary>Abstract text</summary>";
        assert_eq!(
            extract_xml_tag(xml, "title"),
            Some("Some Title".to_string())
        );
        assert_eq!(
            extract_xml_tag(xml, "summary"),
            Some("Abstract text".to_string())
        );
        assert_eq!(extract_xml_tag(xml, "missing"), None);
    }

    #[test]
    fn test_parse_arxiv_xml() {
        let xml = r#"<?xml version="1.0"?>
<feed>
<entry>
<id>http://arxiv.org/abs/2301.12345v1</id>
<title>Test Paper Title</title>
<summary>This is an abstract.</summary>
<author><name>John Doe</name></author>
<author><name>Jane Smith</name></author>
</entry>
</feed>"#;
        let result = parse_arxiv_xml(xml);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let papers = parsed["papers"].as_array().unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0]["title"], "Test Paper Title");
        assert_eq!(papers[0]["arxivId"], "2301.12345v1");
        assert_eq!(papers[0]["authors"].as_array().unwrap().len(), 2);
    }
}
