use amzn_codewhisperer_client::types::Customization as CodewhispererCustomization;
use amzn_consolas_client::types::CustomizationSummary as ConsolasCustomization;
use fig_settings::State;
use serde::{
    Deserialize,
    Serialize,
};

const CUSTOMIZATION_STATE_KEY: &str = "api.selectedCustomization";

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Customization {
    pub arn: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Customization {
    /// Load the currently selected customization from state
    pub fn load_selected(state: &State) -> Result<Option<Self>, fig_settings::Error> {
        state.get(CUSTOMIZATION_STATE_KEY)
    }

    /// Save the currently selected customization to state
    pub fn save_selected(&self, state: &State) -> Result<(), fig_settings::Error> {
        state.set_value(CUSTOMIZATION_STATE_KEY, serde_json::to_value(self)?)
    }

    /// Delete the currently selected customization from state
    pub fn delete_selected(state: &State) -> Result<(), fig_settings::Error> {
        state.remove_value(CUSTOMIZATION_STATE_KEY)
    }
}

impl From<Customization> for CodewhispererCustomization {
    fn from(Customization { arn, name, description }: Customization) -> Self {
        CodewhispererCustomization::builder()
            .arn(arn)
            .set_name(name)
            .set_description(description)
            .build()
            .expect("Failed to build CW Customization")
    }
}

impl From<CodewhispererCustomization> for Customization {
    fn from(cw_customization: CodewhispererCustomization) -> Self {
        Customization {
            arn: cw_customization.arn,
            name: cw_customization.name,
            description: cw_customization.description,
        }
    }
}

impl From<ConsolasCustomization> for Customization {
    fn from(consolas_customization: ConsolasCustomization) -> Self {
        Customization {
            arn: consolas_customization.arn,
            name: Some(consolas_customization.customization_name),
            description: consolas_customization.description,
        }
    }
}

#[cfg(test)]
mod tests {
    use amzn_consolas_client::types::CustomizationStatus;
    use aws_smithy_types::DateTime;

    use super::*;

    #[test]
    fn test_customization_from_impls() {
        let cw_customization = CodewhispererCustomization::builder()
            .arn("arn")
            .name("name")
            .description("description")
            .build()
            .unwrap();

        let custom_from_cw: Customization = cw_customization.into();
        let cw_from_custom: CodewhispererCustomization = custom_from_cw.into();

        assert_eq!(cw_from_custom.arn, "arn");
        assert_eq!(cw_from_custom.name, Some("name".into()));
        assert_eq!(cw_from_custom.description, Some("description".into()));

        let cw_customization = CodewhispererCustomization::builder().arn("arn").build().unwrap();

        let custom_from_cw: Customization = cw_customization.into();
        let cw_from_custom: CodewhispererCustomization = custom_from_cw.into();

        assert_eq!(cw_from_custom.arn, "arn");
        assert_eq!(cw_from_custom.name, None);
        assert_eq!(cw_from_custom.description, None);

        let consolas_customization = ConsolasCustomization::builder()
            .arn("arn")
            .customization_name("name")
            .description("description")
            .status(CustomizationStatus::Activated)
            .updated_at(DateTime::from_secs(0))
            .build()
            .unwrap();

        let custom_from_consolas: Customization = consolas_customization.into();

        assert_eq!(custom_from_consolas.arn, "arn");
        assert_eq!(custom_from_consolas.name, Some("name".into()));
        assert_eq!(custom_from_consolas.description, Some("description".into()));
    }

    #[test]
    fn test_customization_save_load() {
        let state = State::new_fake();

        let value = Customization {
            arn: "arn".into(),
            name: Some("name".into()),
            description: Some("description".into()),
        };

        value.save_selected(&state).unwrap();
        let loaded_value = Customization::load_selected(&state).unwrap();
        assert_eq!(loaded_value, Some(value));

        Customization::delete_selected(&state).unwrap();
    }

    #[test]
    fn test_customization_serde() {
        let customization = Customization {
            arn: "arn".into(),
            name: Some("name".into()),
            description: Some("description".into()),
        };

        let serialized = serde_json::to_string(&customization).unwrap();
        assert_eq!(serialized, r#"{"arn":"arn","name":"name","description":"description"}"#);

        let deserialized: Customization = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, customization);

        let customization = Customization {
            arn: "arn".into(),
            name: None,
            description: None,
        };

        let serialized = serde_json::to_string(&customization).unwrap();
        assert_eq!(serialized, r#"{"arn":"arn"}"#);

        let deserialized: Customization = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, customization);
    }
}
