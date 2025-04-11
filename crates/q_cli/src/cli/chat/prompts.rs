/// This file contains prompt templates used in the chat module.

/// Summary request templates for conversation summarization
pub mod summary {
    /// Creates a summary request with custom instructions
    pub fn with_custom_prompt(custom_prompt: &str) -> String {
        format!(
            "\n\n\
            FORMAT REQUIREMENTS: Create a structured, concise summary in bullet-point format. DO NOT respond conversationally. DO NOT address the user directly.\n\n\
            IMPORTANT CUSTOM INSTRUCTION: {}\n\n\
            Your task is to create a structured summary document containing:\n\
            1) A bullet-point list of key topics/questions covered\n\
            2) Bullet points for all significant tools executed and their results\n\
            3) Bullet points for any code or technical information shared\n\
            4) A section of key insights gained\n\n\
            FORMAT THE SUMMARY IN THIRD PERSON, NOT AS A DIRECT RESPONSE. Example format:\n\n\
            ## CONVERSATION SUMMARY\n\
            * Topic 1: Key information\n\
            * Topic 2: Key information\n\n\
            ## TOOLS EXECUTED\n\
            * Tool X: Result Y\n\n\
            Remember this is a DOCUMENT not a chat response. The custom instruction above modifies what to prioritize.\n\
            FILTER OUT CHAT CONVENTIONS (greetings, offers to help, etc).",
            custom_prompt
        )
    }

    /// Default summary request without custom instructions
    pub fn default() -> String {
        "[SYSTEM NOTE: This is an automated summarization request, not from the user]\n\n\
        FORMAT REQUIREMENTS: Create a structured, concise summary in bullet-point format. DO NOT respond conversationally. DO NOT address the user directly.\n\n\
        Your task is to create a structured summary document containing:\n\
        1) A bullet-point list of key topics/questions covered\n\
        2) Bullet points for all significant tools executed and their results\n\
        3) Bullet points for any code or technical information shared\n\
        4) A section of key insights gained\n\n\
        FORMAT THE SUMMARY IN THIRD PERSON, NOT AS A DIRECT RESPONSE. Example format:\n\n\
        ## CONVERSATION SUMMARY\n\
        * Topic 1: Key information\n\
        * Topic 2: Key information\n\n\
        ## TOOLS EXECUTED\n\
        * Tool X: Result Y\n\n\
        Remember this is a DOCUMENT not a chat response.\n\
        FILTER OUT CHAT CONVENTIONS (greetings, offers to help, etc).".to_string()
    }
}
