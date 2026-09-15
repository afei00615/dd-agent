use std::pin::Pin;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::{Stream, StreamExt};
use reqwest::{Client, Response, Url};
use serde::Deserialize;

use crate::{ChatChunk, ChatRequest, ChatResponse, LlmConfig, LlmError, Result};

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatChunk>> + Send>>;

/// 主体 Agent 依赖的抽象接口；测试时可以替换成内存 mock。
#[async_trait]
pub trait ChatCompletionApi: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    async fn chat_stream(&self, request: ChatRequest) -> Result<ChatStream>;
}

#[derive(Clone)]
pub struct OpenAiCompatibleClient {
    http: Client,
    endpoint: Url,
    api_key: Option<String>,
    default_model: String,
}

impl OpenAiCompatibleClient {
    pub fn new(config: LlmConfig) -> Result<Self> {
        if config.default_model.trim().is_empty() {
            return Err(LlmError::Config("模型名称不能为空".into()));
        }

        let base = format!("{}/", config.base_url.trim_end_matches('/'));
        let base_url = Url::parse(&base)
            .map_err(|error| LlmError::Config(format!("LLM_BASE_URL 不是有效 URL：{error}")))?;
        if !matches!(base_url.scheme(), "http" | "https")
            || base_url.host_str().is_none()
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            return Err(LlmError::Config(
                "LLM_BASE_URL 必须是无凭据、查询参数和片段的 HTTP(S) 地址".into(),
            ));
        }
        let endpoint = base_url
            .join("chat/completions")
            .map_err(|error| LlmError::Config(format!("无法生成接口地址：{error}")))?;
        let http = Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(LlmError::Client)?;

        Ok(Self {
            http,
            endpoint,
            api_key: config.api_key,
            default_model: config.default_model,
        })
    }

    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    pub fn request(&self, messages: Vec<crate::Message>) -> ChatRequest {
        ChatRequest::new(self.default_model.clone(), messages)
    }

    async fn send(&self, request: &ChatRequest) -> Result<Response> {
        let mut builder = self.http.post(self.endpoint.clone()).json(request);
        if let Some(api_key) = self.api_key.as_deref() {
            builder = builder.bearer_auth(api_key);
        }
        builder.send().await.map_err(LlmError::Transport)
    }

    async fn ensure_success(response: Response) -> Result<Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let body = response.text().await.map_err(LlmError::Transport)?;
        let message = parse_api_message(&body).unwrap_or_else(|| {
            status
                .canonical_reason()
                .unwrap_or("未知接口错误")
                .to_string()
        });
        Err(LlmError::Api {
            status,
            message,
            body,
        })
    }
}

#[async_trait]
impl ChatCompletionApi for OpenAiCompatibleClient {
    async fn chat(&self, mut request: ChatRequest) -> Result<ChatResponse> {
        request.stream = false;
        let response = Self::ensure_success(self.send(&request).await?).await?;
        let body = response.text().await.map_err(LlmError::Transport)?;
        serde_json::from_str(&body).map_err(|source| LlmError::Decode { source, body })
    }

    async fn chat_stream(&self, mut request: ChatRequest) -> Result<ChatStream> {
        request.stream = true;
        let response = Self::ensure_success(self.send(&request).await?).await?;
        let mut events = response.bytes_stream().eventsource();

        Ok(Box::pin(async_stream::stream! {
            while let Some(event) = events.next().await {
                match event {
                    Ok(event) if event.data.trim() == "[DONE]" => break,
                    Ok(event) if event.data.trim().is_empty() => continue,
                    Ok(event) => match serde_json::from_str::<ChatChunk>(&event.data) {
                        Ok(chunk) => yield Ok(chunk),
                        Err(source) => yield Err(LlmError::Decode {
                            source,
                            body: event.data,
                        }),
                    },
                    Err(error) => {
                        yield Err(LlmError::Stream(error.to_string()));
                        break;
                    }
                }
            }
        }))
    }
}

#[derive(Deserialize)]
struct ApiErrorEnvelope {
    error: ApiErrorBody,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    message: String,
}

fn parse_api_message(body: &str) -> Option<String> {
    serde_json::from_str::<ApiErrorEnvelope>(body)
        .ok()
        .map(|value| value.error.message)
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::*;
    use crate::Message;

    async fn mock_server(
        status: StatusCode,
        content_type: &str,
        body: &str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        let content_type = content_type.to_owned();
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0; 16 * 1024];
            let length = socket.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..length]).to_string();
            let response = format!(
                "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
                content_type,
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            request
        });
        (format!("http://{address}/v1"), handle)
    }

    #[tokio::test]
    async fn sends_non_stream_request_and_reads_text() {
        let (base_url, request_handle) = mock_server(
            StatusCode::OK,
            "application/json",
            r#"{"id":"chat-1","model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"pong"},"finish_reason":"stop"}]}"#,
        )
        .await;
        let client =
            OpenAiCompatibleClient::new(LlmConfig::new(base_url, Some("secret".into()), "test"))
                .unwrap();

        let response = client
            .chat(client.request(vec![Message::user("ping")]))
            .await
            .unwrap();
        let raw_request = request_handle.await.unwrap();

        assert_eq!(response.first_text(), Some("pong"));
        assert!(raw_request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(raw_request
            .to_ascii_lowercase()
            .contains("authorization: bearer secret"));
        assert!(raw_request.contains("\"stream\":false"));
    }

    #[tokio::test]
    async fn exposes_provider_error_message() {
        let (base_url, _) = mock_server(
            StatusCode::UNAUTHORIZED,
            "application/json",
            r#"{"error":{"message":"bad key"}}"#,
        )
        .await;
        let client = OpenAiCompatibleClient::new(LlmConfig::new(base_url, None, "test")).unwrap();

        let error = client
            .chat(client.request(vec![Message::user("ping")]))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            LlmError::Api {
                status: StatusCode::UNAUTHORIZED,
                ..
            }
        ));
        assert!(error.to_string().contains("bad key"));
    }

    #[tokio::test]
    async fn parses_sse_chunks_until_done() {
        let body = concat!(
            "data: {\"id\":\"chat-1\",\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"你\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chat-1\",\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"好\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let (base_url, _) = mock_server(StatusCode::OK, "text/event-stream", body).await;
        let client = OpenAiCompatibleClient::new(LlmConfig::new(base_url, None, "test")).unwrap();
        let mut stream = client
            .chat_stream(client.request(vec![Message::user("你好")]))
            .await
            .unwrap();
        let mut text = String::new();

        while let Some(chunk) = stream.next().await {
            if let Some(content) = chunk.unwrap().choices[0].delta.content.as_deref() {
                text.push_str(content);
            }
        }

        assert_eq!(text, "你好");
    }
}
