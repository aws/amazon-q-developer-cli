// integration_checkpoint_3.rs
// Integration checkpoint for the enhanced topic extractor

use crate::conversation::Conversation;
use crate::topic_extractor::basic;
use crate::topic_extractor::enhanced;
use crate::topic_extractor::advanced;
use crate::filename_generator;
use crate::save_config;
use crate::commands::save;

/// Integration checkpoint for the enhanced topic extractor
///
/// This function verifies that all components work together correctly
pub fn run_integration_checkpoint() -> Result<(), String> {
    println!("Running integration checkpoint 3...");
    
    // Create a test conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI".to_string())
        .add_assistant_message("Sure, what do you want to know about Amazon Q CLI?".to_string(), None)
        .add_user_message("How do I save conversations automatically?".to_string())
        .add_assistant_message("You can use the /save command without specifying a filename.".to_string(), None)
        .add_user_message("That sounds great! Can you show me an example?".to_string())
        .add_assistant_message("Sure, just type `/save` and the conversation will be saved with an automatically generated filename.".to_string(), None);
    
    // Test basic topic extraction
    let (basic_main_topic, basic_sub_topic, basic_action_type) = basic::extract_topics(&conversation);
    println!("Basic topic extraction:");
    println!("  Main topic: {}", basic_main_topic);
    println!("  Sub-topic: {}", basic_sub_topic);
    println!("  Action type: {}", basic_action_type);
    
    // Test enhanced topic extraction
    let (enhanced_main_topic, enhanced_sub_topic, enhanced_action_type) = enhanced::extract_topics(&conversation);
    println!("Enhanced topic extraction:");
    println!("  Main topic: {}", enhanced_main_topic);
    println!("  Sub-topic: {}", enhanced_sub_topic);
    println!("  Action type: {}", enhanced_action_type);
    
    // Test advanced topic extraction
    let (advanced_main_topic, advanced_sub_topic, advanced_action_type) = advanced::extract_topics(&conversation);
    println!("Advanced topic extraction:");
    println!("  Main topic: {}", advanced_main_topic);
    println!("  Sub-topic: {}", advanced_sub_topic);
    println!("  Action type: {}", advanced_action_type);
    
    // Test filename generation with different extractors
    let basic_filename = filename_generator::generate_filename_with_extractor(&conversation, &basic::extract_topics);
    let enhanced_filename = filename_generator::generate_filename_with_extractor(&conversation, &enhanced::extract_topics);
    let advanced_filename = filename_generator::generate_filename_with_extractor(&conversation, &advanced::extract_topics);
    
    println!("Filename generation:");
    println!("  Basic: {}", basic_filename);
    println!("  Enhanced: {}", enhanced_filename);
    println!("  Advanced: {}", advanced_filename);
    
    // Test save command with different extractors
    let config = save_config::SaveConfig::new();
    
    // Test with basic extractor
    let basic_save_result = save::handle_save_command_with_extractor(
        &Vec::new(),
        &conversation,
        &config,
        &basic::extract_topics
    );
    
    // Test with enhanced extractor
    let enhanced_save_result = save::handle_save_command_with_extractor(
        &Vec::new(),
        &conversation,
        &config,
        &enhanced::extract_topics
    );
    
    // Test with advanced extractor
    let advanced_save_result = save::handle_save_command_with_extractor(
        &Vec::new(),
        &conversation,
        &config,
        &advanced::extract_topics
    );
    
    println!("Save command:");
    println!("  Basic: {:?}", basic_save_result);
    println!("  Enhanced: {:?}", enhanced_save_result);
    println!("  Advanced: {:?}", advanced_save_result);
    
    // Test with a technical conversation
    let mut technical_conversation = Conversation::new("test-id-2".to_string());
    technical_conversation.add_user_message("I'm having an issue with this Rust code:".to_string())
        .add_user_message("```rust\nfn main() {\n    let x = 5;\n    println!(\"{}\", y); // Error: y is not defined\n}\n```".to_string())
        .add_assistant_message("The variable `y` is not defined. You should use `x` instead:".to_string(), None)
        .add_assistant_message("```rust\nfn main() {\n    let x = 5;\n    println!(\"{}\", x);\n}\n```".to_string(), None);
    
    // Test advanced topic extraction with technical conversation
    let (tech_main_topic, tech_sub_topic, tech_action_type) = advanced::extract_topics(&technical_conversation);
    println!("Advanced topic extraction (technical conversation):");
    println!("  Main topic: {}", tech_main_topic);
    println!("  Sub-topic: {}", tech_sub_topic);
    println!("  Action type: {}", tech_action_type);
    
    // Test with a feature request conversation
    let mut feature_conversation = Conversation::new("test-id-3".to_string());
    feature_conversation.add_user_message("I would like to request a feature for Amazon Q CLI.".to_string())
        .add_assistant_message("Sure, what feature would you like to request?".to_string(), None)
        .add_user_message("I think it would be great if the CLI could automatically name saved conversations based on their content.".to_string())
        .add_assistant_message("That's a good suggestion. I'll make note of that feature request.".to_string(), None);
    
    // Test advanced topic extraction with feature request conversation
    let (feature_main_topic, feature_sub_topic, feature_action_type) = advanced::extract_topics(&feature_conversation);
    println!("Advanced topic extraction (feature request conversation):");
    println!("  Main topic: {}", feature_main_topic);
    println!("  Sub-topic: {}", feature_sub_topic);
    println!("  Action type: {}", feature_action_type);
    
    // Test with a multi-language conversation (simplified implementation)
    let mut multi_lang_conversation = Conversation::new("test-id-4".to_string());
    multi_lang_conversation.add_user_message("Hola, necesito ayuda con Amazon Q CLI.".to_string())
        .add_assistant_message("Claro, ¿en qué puedo ayudarte con Amazon Q CLI?".to_string(), None)
        .add_user_message("¿Cómo puedo guardar conversaciones automáticamente?".to_string())
        .add_assistant_message("Puedes usar el comando `/save` sin especificar un nombre de archivo.".to_string(), None);
    
    // Test advanced topic extraction with multi-language conversation
    let (multi_lang_main_topic, multi_lang_sub_topic, multi_lang_action_type) = advanced::extract_topics(&multi_lang_conversation);
    println!("Advanced topic extraction (multi-language conversation):");
    println!("  Main topic: {}", multi_lang_main_topic);
    println!("  Sub-topic: {}", multi_lang_sub_topic);
    println!("  Action type: {}", multi_lang_action_type);
    
    // Verify that all components work together correctly
    if advanced_main_topic == "AmazonQ" && 
       advanced_sub_topic == "CLI" && 
       (advanced_action_type == "Help" || advanced_action_type == "Learning") &&
       tech_main_topic == "Rust" && 
       tech_action_type == "Troubleshooting" &&
       feature_main_topic == "AmazonQ" && 
       feature_action_type == "FeatureRequest" {
        println!("Integration checkpoint 3 passed!");
        Ok(())
    } else {
        println!("Integration checkpoint 3 failed!");
        Err("Topic extraction did not produce expected results".to_string())
    }
}

/// Run the integration checkpoint and print the results
pub fn main() {
    match run_integration_checkpoint() {
        Ok(_) => println!("Integration checkpoint 3 completed successfully."),
        Err(e) => println!("Integration checkpoint 3 failed: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_integration_checkpoint() {
        let result = run_integration_checkpoint();
        assert!(result.is_ok());
    }
}
