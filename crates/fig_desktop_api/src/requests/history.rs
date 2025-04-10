use fig_proto::fig::history_query_request::param::Type;
use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    HistoryQueryRequest,
    HistoryQueryResponse,
};
use fig_settings::history::History;
use fig_settings::history::rusqlite::params_from_iter;
use fig_settings::history::rusqlite::types::Value;

use super::RequestResult;

pub async fn query(request: HistoryQueryRequest) -> RequestResult {
    let history = History::new();
    let mut params: Vec<Value> = Vec::with_capacity(request.params.len());
    for (i, param) in request.params.iter().enumerate() {
        let param = match &param.r#type {
            Some(Type::Null(())) => Value::Null,
            Some(Type::Integer(i)) => Value::Integer(*i),
            Some(Type::Float(f)) => Value::Real(*f),
            Some(Type::String(s)) => Value::Text(s.clone()),
            Some(Type::Blob(b)) => Value::Blob(b.clone()),
            None => return Err(format!("History query parameter {i} is missing type").into()),
        };
        params.push(param);
    }

    let results = history
        .query(&request.query, params_from_iter(params))
        .map_err(|err| format!("Failed querying history: {err}"))?;

    let json_array =
        serde_json::to_string(&results).map_err(|err| format!("Failed serializing history query results: {err}"))?;

    let response = ServerOriginatedSubMessage::HistoryQueryResponse(HistoryQueryResponse { json_array });
    Ok(response.into())
}
