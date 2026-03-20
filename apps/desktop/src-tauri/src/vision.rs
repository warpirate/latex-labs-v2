use crate::llm::{self, ImageContent, LlmConfig};

/// Convert an image to LaTeX using a vision-language model.
///
/// Modes: "equation", "table", "figure", "algorithm", "ocr"
#[tauri::command]
pub async fn vision_to_latex(
    image_base64: String,
    mode: String,
    custom_prompt: Option<String>,
    llm_config: LlmConfig,
) -> Result<String, String> {
    if image_base64.is_empty() {
        return Err("Missing image data".to_string());
    }

    let user_instruction = match mode.as_str() {
        "table" => "Convert the table in the image to LaTeX tabular/table. Output only the table environment (no document, no preamble, no extra text).".to_string(),
        "figure" => "Generate a LaTeX figure environment with caption and label for the content in this image. Output only the figure environment (no document, no extra text).".to_string(),
        "algorithm" => "Convert the algorithm in the image to LaTeX algorithm/algorithmic. Output only the algorithm environment (no document, no extra text).".to_string(),
        "ocr" => "Extract the text from the image and return clean LaTeX-safe text. Output only text.".to_string(),
        _ => "Convert the formula in the image to LaTeX equation environment. Output only the equation environment (no document, no extra text).".to_string(),
    };

    let text_prompt = if let Some(ref custom) = custom_prompt {
        if custom.trim().is_empty() {
            user_instruction
        } else {
            format!("{}\n\nUser note: {}", user_instruction, custom)
        }
    } else {
        user_instruction
    };

    let system =
        "You are a LaTeX conversion engine. Return only LaTeX or plain text without explanations.";

    // Detect media type from base64 header or default to png
    let media_type = if image_base64.starts_with("/9j/") {
        "image/jpeg"
    } else if image_base64.starts_with("iVBOR") {
        "image/png"
    } else if image_base64.starts_with("R0lGOD") {
        "image/gif"
    } else if image_base64.starts_with("UklGR") {
        "image/webp"
    } else {
        "image/png"
    };

    let images = vec![ImageContent {
        base64_data: image_base64,
        media_type: media_type.to_string(),
    }];

    let result = llm::call_vision_llm(&llm_config, system, &text_prompt, &images).await?;

    // Strip markdown code fences if the LLM wrapped the output
    let cleaned = strip_code_fences(&result);
    Ok(cleaned)
}

/// Remove markdown code fences from LLM output.
fn strip_code_fences(s: &str) -> String {
    let trimmed = s.trim();
    // Handle ```latex ... ``` or ```tex ... ``` or ``` ... ```
    if let Some(rest) = trimmed.strip_prefix("```") {
        // Skip the language tag on the first line
        let after_tag = if let Some(newline_pos) = rest.find('\n') {
            &rest[newline_pos + 1..]
        } else {
            rest
        };
        if let Some(content) = after_tag.strip_suffix("```") {
            return content.trim().to_string();
        }
        return after_tag.trim().to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_code_fences_latex() {
        let input = "```latex\n\\begin{equation}\nx = 1\n\\end{equation}\n```";
        assert_eq!(
            strip_code_fences(input),
            "\\begin{equation}\nx = 1\n\\end{equation}"
        );
    }

    #[test]
    fn test_strip_code_fences_bare() {
        let input = "```\n\\frac{a}{b}\n```";
        assert_eq!(strip_code_fences(input), "\\frac{a}{b}");
    }

    #[test]
    fn test_strip_code_fences_none() {
        let input = "\\begin{equation} x = 1 \\end{equation}";
        assert_eq!(strip_code_fences(input), input);
    }
}
