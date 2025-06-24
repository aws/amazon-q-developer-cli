use fig_api_client::{
    Client,
    Customization,
};
use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    CodewhispererCustomization,
    CodewhispererListCustomizationRequest,
    CodewhispererListCustomizationResponse,
};

use super::RequestResult;

pub async fn list_customization(_request: CodewhispererListCustomizationRequest) -> RequestResult {
    let client = Client::new()
        .await
        .map_err(|err| format!("failed to create client: {:?}", err))?;
    let customizations = client
        .list_customizations()
        .await
        .map_err(|err| format!("list_customizations failed: {:?}", err))?
        .into_iter()
        .map(|Customization { arn, name, description }| CodewhispererCustomization { arn, name, description })
        .collect();

    let response =
        ServerOriginatedSubMessage::CodewhispererListCustomizationResponse(CodewhispererListCustomizationResponse {
            customizations,
        });

    Ok(response.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_customization() {
        let request = CodewhispererListCustomizationRequest {};
        let response = list_customization(request).await;
        println!("{response:?}");
    }
}
