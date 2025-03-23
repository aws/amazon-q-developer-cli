use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use tracing::{
    debug,
    error,
    info,
};

use super::repository::Repository;

/// Generates an HTML visualization of the agent trajectory
pub fn generate_visualization(repo: &Repository, output_dir: &Path) -> Result<PathBuf, String> {
    let output_path = output_dir.join("trajectory.html");

    // Create HTML content
    let html_content = generate_html_visualization(repo)?;

    // Write to file
    if let Err(e) = fs::write(&output_path, html_content) {
        return Err(format!("Failed to write visualization file: {}", e));
    }

    Ok(output_path)
}

/// Generates HTML content for the visualization
fn generate_html_visualization(repo: &Repository) -> Result<String, String> {
    // Start with HTML header and styles
    let mut html = r#"<!DOCTYPE html>
<html>
<head>
    <title>Agent Trajectory Visualization</title>
    <style>
        body { 
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; 
            margin: 20px;
            background-color: #f9f9f9;
            color: #333;
            line-height: 1.6;
        }
        .container {
            max-width: 1000px;
            margin: 0 auto;
        }
        h1, h2, h3 { 
            color: #2c3e50; 
            margin-top: 20px;
        }
        .timeline-container { 
            margin-bottom: 40px;
            background-color: white;
            border-radius: 8px;
            padding: 20px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.05);
        }
        .timeline { 
            position: relative; 
            margin-left: 30px;
            padding-left: 20px;
        }
        .timeline::before { 
            content: ''; 
            position: absolute; 
            left: 0; 
            top: 0; 
            bottom: 0; 
            width: 2px; 
            background-color: #ddd; 
        }
        .step { 
            position: relative;
            margin: 20px 0;
            padding: 15px;
            border-radius: 8px;
            background-color: #f5f5f5;
            transition: all 0.3s ease;
        }
        .step:hover {
            box-shadow: 0 5px 15px rgba(0,0,0,0.1);
            transform: translateY(-2px);
        }
        .step::before { 
            content: ''; 
            position: absolute; 
            left: -29px; 
            top: 15px;
            width: 16px; 
            height: 16px; 
            border-radius: 50%; 
            background-color: #3498db; 
            border: 2px solid white;
        }
        .user-step {
            background-color: #e8f4fc;
            border-left: 4px solid #3498db;
        }
        .user-step::before {
            background-color: #2980b9;
            width: 20px;
            height: 20px;
            left: -31px;
        }
        .step-summary {
            font-weight: bold;
            color: #2c3e50;
            margin-bottom: 10px;
        }
        .step-details {
            display: none;
            background-color: white;
            padding: 15px;
            margin-top: 10px;
            border-radius: 5px;
            border: 1px solid #eee;
        }
        .step-toggle {
            background-color: #f0f0f0;
            border: none;
            padding: 5px 10px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 12px;
            color: #555;
            margin-top: 10px;
        }
        .step-toggle:hover {
            background-color: #e0e0e0;
        }
        .tag {
            display: inline-block;
            background-color: #3498db;
            color: white;
            padding: 2px 8px;
            border-radius: 12px;
            font-size: 0.8em;
            margin-right: 5px;
            margin-top: 5px;
        }
        .tag.tool-use { background-color: #2ecc71; }
        .tag.reasoning { background-color: #9b59b6; }
        .tag.response { background-color: #e74c3c; }
        .tag.user-input { background-color: #f39c12; }
        
        .timestamp {
            font-size: 0.8em;
            color: #7f8c8d;
            margin-top: 5px;
        }
        .action-summary {
            font-weight: normal;
            color: #555;
        }
    </style>
    <script>
        function toggleDetails(id) {
            var content = document.getElementById(id);
            var btn = document.querySelector(`button[onclick="toggleDetails('${id}')"]`);
            
            if (content.style.display === "block") {
                content.style.display = "none";
                btn.textContent = "Show Details";
            } else {
                content.style.display = "block";
                btn.textContent = "Hide Details";
            }
        }
    </script>
</head>
<body>
    <div class="container">
        <h1>Agent Trajectory Visualization</h1>
"#
    .to_string();

    // Get all steps sorted by timestamp
    let mut all_steps: Vec<_> = repo.steps.values().collect();
    all_steps.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    // Track the last user instruction to avoid repetition
    let mut last_user_instruction = None;

    // Add timeline container
    html.push_str(
        r#"
        <div class="timeline-container">
            <div class="timeline">
"#,
    );

    // Add steps for each step
    for step in all_steps {
        // Extract step information
        let instruction = step.user_instruction.as_deref();
        let reasoning = step.agent_reasoning.as_deref();
        let response = step.agent_response.as_deref();
        let action = step.agent_action.as_ref();
        let result = step.action_result.as_ref();
        let category = step.category.as_deref().unwrap_or("unknown");
        let tags = &step.tags;
        let timestamp = &step.timestamp;

        // Check if this is a user instruction step
        let is_user_step = instruction.is_some() && instruction != last_user_instruction;
        if is_user_step {
            last_user_instruction = instruction;
        }

        // Create step class
        let step_class = if is_user_step { "user-step" } else { "step" };

        // Create step HTML
        html.push_str(&format!(
            r#"
            <div class="{}" id="step-{}">
        "#,
            step_class, step.id
        ));

        // Add step content
        if is_user_step {
            html.push_str(&format!(
                r#"
                <div class="step-summary">User Instruction</div>
                <div>{}</div>
            "#,
                instruction.unwrap_or("")
            ));
        } else if let Some(action) = action {
            let action_name = action.name.as_deref().unwrap_or("");
            let action_type = &action.action_type;

            html.push_str(&format!(
                r#"
                <div class="step-summary">{} {}</div>
            "#,
                action_type, action_name
            ));
        } else if reasoning.is_some() {
            html.push_str(
                r#"
                <div class="step-summary">Reasoning</div>
            "#,
            );
        } else if response.is_some() {
            html.push_str(
                r#"
                <div class="step-summary">Response</div>
            "#,
            );
        } else {
            html.push_str(&format!(
                r#"
                <div class="step-summary">{}</div>
            "#,
                category
            ));
        }

        // Add timestamp
        html.push_str(&format!(
            r#"
            <div class="timestamp">{}</div>
        "#,
            timestamp
        ));

        // Add tags
        if !tags.is_empty() {
            html.push_str(r#"<div class="tags-container">"#);
            for tag in tags {
                let tag_class = tag.to_lowercase().replace(' ', "-");
                html.push_str(&format!(r#"<span class="tag {}">{}</span>"#, tag_class, tag));
            }
            html.push_str("</div>");
        }

        // Add toggle button for details
        let detail_id = format!("details-{}", step.id);
        html.push_str(&format!(
            r#"
            <button class="step-toggle" onclick="toggleDetails('{}')">Show Details</button>
        "#,
            detail_id
        ));

        // Add hidden details section
        html.push_str(&format!(
            r#"
            <div class="step-details" id="{}">
        "#,
            detail_id
        ));

        if let Some(reasoning) = reasoning {
            html.push_str(&format!(
                r#"
                <div><strong>Reasoning:</strong><br>{}</div>
            "#,
                reasoning
            ));
        }

        if let Some(action) = action {
            let parameters = serde_json::to_string_pretty(&action.parameters).unwrap_or_else(|_| "{}".to_string());
            html.push_str(&format!(
                r#"
                <div><strong>Action:</strong><br>Type: {}<br>Name: {}<br>Parameters: <pre>{}</pre></div>
            "#,
                action.action_type,
                action.name.as_deref().unwrap_or(""),
                parameters
            ));
        }

        if let Some(result) = result {
            let data = result.data.as_ref().map_or("None".to_string(), |d| {
                serde_json::to_string_pretty(d).unwrap_or_else(|_| "{}".to_string())
            });

            html.push_str(&format!(
                r#"
                <div><strong>Result:</strong><br>Success: {}<br>Data: <pre>{}</pre><br>Error: {}</div>
            "#,
                result.success,
                data,
                result.error_message.as_deref().unwrap_or("None")
            ));
        }

        if let Some(response) = response {
            html.push_str(&format!(
                r#"
                <div><strong>Response:</strong><br>{}</div>
            "#,
                response
            ));
        }

        html.push_str("</div>"); // Close details div
        html.push_str("</div>"); // Close step div
    }

    // Close timeline and container
    html.push_str(
        r#"
            </div>  <!-- Close timeline -->
        </div>  <!-- Close timeline-container -->
    </div>  <!-- Close container -->
</body>
</html>
"#,
    );

    Ok(html)
}
