use serde::{Deserialize, Serialize};

use super::parser::RequestMetadata;

/// Performance metrics calculated from request metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Tokens generated per second
    pub tokens_per_second: f64,
    /// Time to first token in milliseconds
    pub time_to_first_token_ms: u64,
    /// Total response duration in milliseconds
    pub total_duration_ms: u64,
    /// Prompt processing time in milliseconds (if available)
    pub prompt_processing_time_ms: Option<u64>,
    /// Generation time in milliseconds (time from first to last token)
    pub generation_time_ms: u64,
    /// Average time between tokens in milliseconds
    pub average_inter_token_latency_ms: f64,
    /// Total tokens in the response
    pub total_tokens: usize,
    /// Tokens in the prompt/input
    pub prompt_tokens: usize,
}

impl PerformanceMetrics {
    /// Calculate performance metrics from request metadata
    pub fn calculate(metadata: &RequestMetadata) -> Option<Self> {
        let token_metrics = metadata.token_metrics.as_ref()?;

        // Calculate total duration from request start to stream end
        let total_duration_ms = metadata
            .stream_end_timestamp_ms
            .saturating_sub(metadata.request_start_timestamp_ms);

        // Get time to first token
        let time_to_first_token_ms = token_metrics.time_to_first_token_ms?;

        // Calculate generation time (from first to last token)
        let generation_time_ms = token_metrics
            .time_to_last_token_ms
            .unwrap_or(time_to_first_token_ms)
            .saturating_sub(time_to_first_token_ms);

        // Calculate tokens per second
        let tokens_per_second = if generation_time_ms > 0 {
            (token_metrics.total_tokens as f64) / (generation_time_ms as f64 / 1000.0)
        } else {
            0.0
        };

        // Calculate average inter-token latency
        let average_inter_token_latency_ms = if token_metrics.total_tokens > 1 {
            generation_time_ms as f64 / (token_metrics.total_tokens.saturating_sub(1) as f64)
        } else {
            0.0
        };

        // Estimate prompt processing time (time from request start to first token)
        let prompt_processing_time_ms = Some(time_to_first_token_ms);

        Some(PerformanceMetrics {
            tokens_per_second,
            time_to_first_token_ms,
            total_duration_ms,
            prompt_processing_time_ms,
            generation_time_ms,
            average_inter_token_latency_ms,
            total_tokens: token_metrics.total_tokens,
            prompt_tokens: token_metrics.prompt_tokens,
        })
    }



    /// Format comprehensive metrics (verbose level 3+)
    pub fn format_comprehensive(&self) -> String {
        let mut output = format!(
            "Performance Metrics:\n  Tokens/sec: {:.1}\n  TTFT: {}ms\n  Total duration: {:.1}s\n  Generation time: {}ms\n  Avg inter-token latency: {:.1}ms\n  Total tokens: {} ({} prompt + {} completion)",
            self.tokens_per_second,
            self.time_to_first_token_ms,
            self.total_duration_ms as f64 / 1000.0,
            self.generation_time_ms,
            self.average_inter_token_latency_ms,
            self.total_tokens,
            self.prompt_tokens,
            self.total_tokens.saturating_sub(self.prompt_tokens)
        );

        if let Some(prompt_processing_ms) = self.prompt_processing_time_ms {
            output.push_str(&format!("\n  Prompt processing: {}ms", prompt_processing_ms));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::parser::TokenMetrics;

    fn create_test_metadata(
        total_tokens: usize,
        prompt_tokens: usize,
        ttft_ms: u64,
        ttlt_ms: u64,
        request_start_ms: u64,
        stream_end_ms: u64,
    ) -> RequestMetadata {
        RequestMetadata {
            request_id: Some("test-request".to_string()),
            message_id: "test-message".to_string(),
            request_start_timestamp_ms: request_start_ms,
            stream_end_timestamp_ms: stream_end_ms,
            time_to_first_chunk: None,
            time_between_chunks: Vec::new(),
            user_prompt_length: 100,
            response_size: 500,
            chat_conversation_type: None,
            tool_use_ids_and_names: Vec::new(),
            model_id: Some("test-model".to_string()),
            message_meta_tags: Vec::new(),
            token_metrics: Some(TokenMetrics {
                total_tokens,
                prompt_tokens,
                time_to_first_token_ms: Some(ttft_ms),
                time_to_last_token_ms: Some(ttlt_ms),
                token_timestamps: Vec::new(),
            }),
        }
    }

    #[test]
    fn test_calculate_basic_metrics() {
        let metadata = create_test_metadata(
            100,  // total tokens
            20,   // prompt tokens
            500,  // TTFT: 500ms
            2500, // TTLT: 2500ms (2s generation time for 100 tokens = 50 tokens/s)
            1000, // request start
            3000, // stream end (2s total)
        );

        let metrics = PerformanceMetrics::calculate(&metadata).unwrap();

        assert_eq!(metrics.total_tokens, 100);
        assert_eq!(metrics.prompt_tokens, 20);
        assert_eq!(metrics.time_to_first_token_ms, 500);
        assert_eq!(metrics.generation_time_ms, 2000); // 2500 - 500
        assert_eq!(metrics.total_duration_ms, 2000); // 3000 - 1000
        assert!((metrics.tokens_per_second - 50.0).abs() < 0.1); // 100 tokens / 2s
        assert!((metrics.average_inter_token_latency_ms - 20.2).abs() < 0.1); // 2000ms / 99 intervals
    }

    #[test]
    fn test_calculate_with_zero_generation_time() {
        let metadata = create_test_metadata(
            1,    // total tokens
            0,    // prompt tokens
            500,  // TTFT: 500ms
            500,  // TTLT: 500ms (same as TTFT, no generation time)
            1000, // request start
            1500, // stream end
        );

        let metrics = PerformanceMetrics::calculate(&metadata).unwrap();

        assert_eq!(metrics.generation_time_ms, 0);
        assert_eq!(metrics.tokens_per_second, 0.0);
        assert_eq!(metrics.average_inter_token_latency_ms, 0.0);
    }

    #[test]
    fn test_calculate_returns_none_without_token_metrics() {
        let mut metadata = create_test_metadata(100, 20, 500, 2500, 1000, 3000);
        metadata.token_metrics = None;

        let metrics = PerformanceMetrics::calculate(&metadata);
        assert!(metrics.is_none());
    }

    #[test]
    fn test_calculate_returns_none_without_ttft() {
        let mut metadata = create_test_metadata(100, 20, 500, 2500, 1000, 3000);
        if let Some(ref mut token_metrics) = metadata.token_metrics {
            token_metrics.time_to_first_token_ms = None;
        }

        let metrics = PerformanceMetrics::calculate(&metadata);
        assert!(metrics.is_none());
    }

    #[test]
    fn test_format_comprehensive() {
        let metadata = create_test_metadata(100, 20, 500, 2500, 1000, 3000);
        let metrics = PerformanceMetrics::calculate(&metadata).unwrap();

        let formatted = metrics.format_comprehensive();
        assert!(formatted.contains("Performance Metrics:"));
        assert!(formatted.contains("Tokens/sec: 50.0"));
        assert!(formatted.contains("TTFT: 500ms"));
        assert!(formatted.contains("Total duration: 2.0s"));
        assert!(formatted.contains("Generation time: 2000ms"));
        assert!(formatted.contains("Total tokens: 100 (20 prompt + 80 completion)"));
        assert!(formatted.contains("Prompt processing: 500ms"));
    }
}
