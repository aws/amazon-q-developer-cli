//! Generic error reason/description helpers used by telemetry emitters.

pub trait ReasonCode: std::error::Error {
    fn reason_code(&self) -> String;
}

/// Returns a generic error reason + reason description pair.
pub fn get_error_reason<E>(error: &E) -> (String, String)
where
    E: ReasonCode + 'static,
{
    let mut err_chain = eyre::Chain::new(error);
    let reason_desc = if err_chain.len() > 1 {
        format!(
            "'{}' caused by: {}",
            error,
            err_chain.next_back().map_or("UNKNOWN".to_string(), |e| e.to_string())
        )
    } else {
        error.to_string()
    };

    (error.reason_code(), reason_desc)
}
