//! Legacy utility functions.

use aws_smithy_types::{
    Document,
    Number as SmithyNumber,
};

pub fn serde_value_to_document(value: serde_json::Value) -> Document {
    match value {
        serde_json::Value::Null => Document::Null,
        serde_json::Value::Bool(bool) => Document::Bool(bool),
        serde_json::Value::Number(number) => {
            if let Some(num) = number.as_u64() {
                Document::Number(SmithyNumber::PosInt(num))
            } else if number.as_i64().is_some_and(|n| n < 0) {
                Document::Number(SmithyNumber::NegInt(number.as_i64().unwrap()))
            } else {
                Document::Number(SmithyNumber::Float(number.as_f64().unwrap_or_default()))
            }
        },
        serde_json::Value::String(string) => Document::String(string),
        serde_json::Value::Array(vec) => Document::Array(vec.into_iter().map(serde_value_to_document).collect()),
        serde_json::Value::Object(map) => {
            Document::Object(map.into_iter().map(|(k, v)| (k, serde_value_to_document(v))).collect())
        },
    }
}

pub mod issue {
    use anstream::{
        eprintln,
        println,
    };
    use crossterm::style::Stylize;
    use eyre::Result;

    use crate::constants::GITHUB_ISSUES_URL;
    use crate::os::Os;
    use crate::os::diagnostics::Diagnostics;
    use crate::util::system_info::is_remote;

    const TEMPLATE_NAME: &str = "1_bug_report_template.yml";

    pub struct IssueCreator {
        pub title: Option<String>,
        pub expected_behavior: Option<String>,
        pub actual_behavior: Option<String>,
        pub steps_to_reproduce: Option<String>,
        pub additional_environment: Option<String>,
    }

    impl IssueCreator {
        pub async fn create_url(&self, os: &Os) -> Result<url::Url> {
            println!("Heading over to GitHub...");

            let warning = |text: &String| {
                format!("<This will be visible to anyone. Do not include personal or sensitive information>\n\n{text}")
            };
            let diagnostics = Diagnostics::new(&os.env).await;

            let os_str = match &diagnostics.system_info.os {
                Some(os) => os.to_string(),
                None => "None".to_owned(),
            };

            let diagnostic_info = match diagnostics.user_readable() {
                Ok(diagnostics) => diagnostics,
                Err(err) => {
                    eprintln!("Error getting diagnostics: {err}");
                    "Error occurred while generating diagnostics".to_owned()
                },
            };

            let environment = match &self.additional_environment {
                Some(env) => format!("{diagnostic_info}\n{env}"),
                None => diagnostic_info,
            };

            let mut params = Vec::new();
            params.push(("template", TEMPLATE_NAME.to_string()));
            params.push(("os", os_str));
            params.push(("environment", warning(&environment)));

            if let Some(t) = self.title.clone() {
                params.push(("title", t));
            }
            if let Some(t) = self.expected_behavior.as_ref() {
                params.push(("expected", warning(t)));
            }
            if let Some(t) = self.actual_behavior.as_ref() {
                params.push(("actual", warning(t)));
            }
            if let Some(t) = self.steps_to_reproduce.as_ref() {
                params.push(("reproduce", warning(t)));
            }

            let url = url::Url::parse_with_params(GITHUB_ISSUES_URL, params.iter())?;

            if is_remote() || crate::util::open::open_url_async(url.as_str()).await.is_err() {
                println!("Issue Url: {}", url.as_str().underlined());
            }

            Ok(url)
        }
    }
}
