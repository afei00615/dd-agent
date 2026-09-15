use reqwest::StatusCode;

pub type Result<T> = std::result::Result<T, LlmError>;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("配置错误：{0}")]
    Config(String),

    #[error("无法创建 HTTP 客户端：{0}")]
    Client(#[source] reqwest::Error),

    #[error("大模型请求失败：{0}")]
    Transport(#[source] reqwest::Error),

    #[error("大模型接口返回 HTTP {status}：{message}")]
    Api {
        status: StatusCode,
        message: String,
        body: String,
    },

    #[error("无法解析大模型响应：{source}")]
    Decode {
        #[source]
        source: serde_json::Error,
        body: String,
    },

    #[error("无法解析流式响应：{0}")]
    Stream(String),
}
