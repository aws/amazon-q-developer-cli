use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use chrono::DateTime;

use super::repository::Repository;

/// Formats a timestamp string to a more user-friendly format
///
/// Converts from RFC3339 format to a simpler "YYYY-MM-DD HH:MM:SS" format
fn format_timestamp(timestamp: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp) {
        // Format as YYYY-MM-DD HH:MM:SS
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        // Return the original timestamp if parsing fails
        timestamp.to_string()
    }
}

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
            max-width: 1200px;
            margin: 0 auto;
        }
        h1, h2, h3 { 
            color: #2c3e50; 
            margin-top: 20px;
        }
        .history-container { 
            margin-bottom: 40px;
            background-color: white;
            border-radius: 8px;
            padding: 20px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.05);
        }
        .history-table {
            width: 100%;
            border-collapse: collapse;
            font-size: 14px;
        }
        .history-table th {
            text-align: left;
            padding: 10px;
            border-bottom: 2px solid #eee;
            color: #2c3e50;
            font-weight: bold;
        }
        .history-table td {
            padding: 10px;
            border-bottom: 1px solid #eee;
            vertical-align: middle;
        }
        .history-table tr:hover {
            background-color: #f5f5f5;
        }
        .graph-column {
            width: 50px;
            position: relative;
        }
        .graph-node {
            width: 12px;
            height: 12px;
            border-radius: 50%;
            background-color: #3498db;
            display: inline-block;
            margin-right: 10px;
            border: 2px solid white;
            box-shadow: 0 0 0 1px #3498db;
        }
        .message-column {
            width: 70%;
        }
        .timestamp-column {
            width: 15%;
            color: #7f8c8d;
            font-size: 0.9em;
            text-align: right;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }
        .step-line {
            display: flex;
            align-items: center;
            flex-wrap: nowrap;
            gap: 10px;
        }
        .step-message {
            flex-grow: 1;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            max-width: 500px;
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
            white-space: nowrap;
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
            white-space: nowrap;
        }
        .tag.tool-use { background-color: #2ecc71; }
        .tag.reasoning { background-color: #9b59b6; }
        .tag.response { background-color: #e74c3c; }
        .tag.user-input { background-color: #f39c12; }
        .tag.checkpoint { background-color: #f1c40f; }
        
        .user-node .graph-node {
            background-color: #2980b9;
            width: 16px;
            height: 16px;
        }
        .tool-node .graph-node {
            background-color: #2ecc71;
        }
        .reasoning-node .graph-node {
            background-color: #9b59b6;
        }
        .response-node .graph-node {
            background-color: #e74c3c;
        }
        .checkpoint-node .graph-node {
            background-color: #f1c40f;
            border-radius: 0;
            transform: rotate(45deg);
        }
        
        .graph-line {
            position: absolute;
            width: 2px;
            background-color: #ddd;
            left: 8px;
            top: 0;
            bottom: 0;
            z-index: 0;
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

    // Add history container and table
    html.push_str(
        r#"
        <div class="history-container">
            <table class="history-table">
                <thead>
                    <tr>
                        <th class="graph-column"></th>
                        <th class="message-column">Message</th>
                        <th class="timestamp-column">Timestamp</th>
                    </tr>
                </thead>
                <tbody>
"#,
    );

    // Add rows for each step
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

        // Determine node type for styling
        let node_class = if is_user_step {
            "user-node"
        } else if tags.iter().any(|t| t.contains("checkpoint")) {
            "checkpoint-node"
        } else if action.is_some() {
            "tool-node"
        } else if reasoning.is_some() {
            "reasoning-node"
        } else if response.is_some() {
            "response-node"
        } else {
            ""
        };

        // Create row HTML
        html.push_str(&format!(
            r#"
            <tr class="{}" id="step-{}">
                <td class="graph-column">
                    <div class="graph-line"></div>
                    <div class="graph-node"></div>
                </td>
                <td class="message-column">
        "#,
            node_class, step.id
        ));

        // Start of single line content
        html.push_str(r#"<div class="step-line">"#);

        // Add tags first
        if !tags.is_empty() {
            for tag in tags {
                let tag_class = match tag.to_lowercase().as_str() {
                    "tool-use" => "tool-use",
                    "reasoning" => "reasoning",
                    "response" => "response",
                    "user-input" => "user-input",
                    "checkpoint" => "checkpoint",
                    _ => "",
                };
                html.push_str(&format!(r#"<span class="tag {}">{}</span>"#, tag_class, tag));
            }
        }

        // Add step content/message
        if is_user_step {
            html.push_str(&format!(
                r#"<span class="step-message">User: {}</span>"#,
                instruction.unwrap_or("")
            ));
        } else if let Some(action) = action {
            let action_name = action.name.as_deref().unwrap_or("");
            let action_type = &action.action_type;

            // Create a more informative message for tool use steps
            let message = if action_type == "tool_use" {
                // Extract the tool name and create a more descriptive message
                let tool_name = action_name.split_whitespace().next().unwrap_or(action_name);

                // Try to extract a more meaningful description from parameters
                let description = if let Some(path) = action.parameters.get("path").and_then(|v| v.as_str()) {
                    let path_short = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);

                    if tool_name == "fs_write" || tool_name == "fs_read" {
                        format!(
                            "{}: {} {}",
                            tool_name,
                            path_short,
                            action.parameters.get("command").and_then(|v| v.as_str()).unwrap_or("")
                        )
                    } else {
                        format!("{}: {}", tool_name, path_short)
                    }
                } else if let Some(command) = action.parameters.get("command").and_then(|v| v.as_str()) {
                    let cmd_preview = command.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
                    if cmd_preview.len() < command.len() {
                        format!("{}: {}...", tool_name, cmd_preview)
                    } else {
                        format!("{}: {}", tool_name, cmd_preview)
                    }
                } else {
                    format!("{}", tool_name)
                };

                description
            } else {
                format!("{} {}", action_type, action_name)
            };

            html.push_str(&format!(r#"<span class="step-message">{}</span>"#, message));
        } else if reasoning.is_some() {
            html.push_str(r#"<span class="step-message">Reasoning</span>"#);
        } else if response.is_some() {
            html.push_str(r#"<span class="step-message">Response</span>"#);
        } else {
            html.push_str(&format!(r#"<span class="step-message">{}</span>"#, category));
        }

        // Add toggle button for details
        let detail_id = format!("details-{}", step.id);
        html.push_str(&format!(
            r#"<button class="step-toggle" onclick="toggleDetails('{}')">Show Details</button>"#,
            detail_id
        ));

        // End of single line content
        html.push_str("</div>");

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
        html.push_str("</td>"); // Close message column

        // Add timestamp column
        html.push_str(&format!(
            r#"
            <td class="timestamp-column">{}</td>
        "#,
            format_timestamp(timestamp)
        ));

        html.push_str("</tr>"); // Close row
    }

    // Close table and container
    html.push_str(
        r#"
                </tbody>
            </table>
        </div>  <!-- Close history-container -->
    </div>  <!-- Close container -->
</body>
</html>
"#,
    );

    Ok(html)
}
