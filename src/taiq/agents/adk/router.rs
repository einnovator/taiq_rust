use adk_rust::{AdkError, Llm, LlmRequest, LlmResponseStream, Result};
use async_trait::async_trait;
use futures::{StreamExt, stream};
use std::sync::Arc;

/// Tries configured models in priority order. Failover happens before the first
/// response event, so a partially streamed answer is never silently repeated.
pub struct RoutingLlm {
    name: String,
    candidates: Vec<Arc<dyn Llm>>,
}

impl RoutingLlm {
    pub fn new(candidates: Vec<Arc<dyn Llm>>) -> Self {
        let name = candidates
            .first()
            .map(|model| model.name().to_owned())
            .unwrap_or_default();
        Self { name, candidates }
    }
}

#[async_trait]
impl Llm for RoutingLlm {
    fn name(&self) -> &str {
        &self.name
    }

    async fn generate_content(
        &self,
        req: LlmRequest,
        streaming: bool,
    ) -> Result<LlmResponseStream> {
        let mut failures = Vec::new();
        for model in &self.candidates {
            let mut request = req.clone();
            request.model = model.name().to_owned();
            match model.generate_content(request, streaming).await {
                Ok(mut responses) => match responses.next().await {
                    Some(Ok(first)) => {
                        return Ok(Box::pin(
                            stream::once(async move { Ok(first) }).chain(responses),
                        ));
                    }
                    Some(Err(error)) => failures.push(format!("{}: {error}", model.name())),
                    None => failures.push(format!("{}: empty response", model.name())),
                },
                Err(error) => failures.push(format!("{}: {error}", model.name())),
            }
        }
        Err(AdkError::model(format!(
            "all configured models failed: {}",
            failures.join("; ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_rust::{Content, LlmResponse};

    struct Broken;
    #[async_trait]
    impl Llm for Broken {
        fn name(&self) -> &str {
            "broken"
        }
        async fn generate_content(&self, _: LlmRequest, _: bool) -> Result<LlmResponseStream> {
            Err(AdkError::model("provider unavailable"))
        }
    }

    #[tokio::test]
    async fn falls_back_before_first_event() {
        let backup = adk_rust::model::MockLlm::new("backup").with_response(LlmResponse::new(
            Content::new("assistant").with_text("ready"),
        ));
        let router = RoutingLlm::new(vec![Arc::new(Broken), Arc::new(backup)]);
        let mut response = router
            .generate_content(LlmRequest::new("broken", vec![]), true)
            .await
            .unwrap();
        assert_eq!(
            response
                .next()
                .await
                .unwrap()
                .unwrap()
                .content
                .unwrap()
                .parts[0]
                .text(),
            Some("ready")
        );
    }
}
