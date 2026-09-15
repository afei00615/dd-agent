use std::{collections::BTreeMap, env, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{LlmError, Result};

/// 客户端配置。API key 为可选，方便连接不需要认证的本地模型服务。
#[derive(Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub default_model: String,
    pub timeout: Duration,
}

impl LlmConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        default_model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
            default_model: default_model.into(),
            timeout: Duration::from_secs(60),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 从 `LLM_BASE_URL`、`LLM_API_KEY`、`LLM_MODEL` 和可选的
    /// `LLM_TIMEOUT_SECONDS` 读取配置。
    pub fn from_env() -> Result<Self> {
        let base_url = required_env("LLM_BASE_URL")?;
        let default_model = required_env("LLM_MODEL")?;
        let api_key = env::var("LLM_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let timeout = match env::var("LLM_TIMEOUT_SECONDS") {
            Ok(value) => {
                let seconds = value
                    .parse::<u64>()
                    .map_err(|_| LlmError::Config("LLM_TIMEOUT_SECONDS 必须是正整数".into()))?;
                if seconds == 0 {
                    return Err(LlmError::Config("LLM_TIMEOUT_SECONDS 必须大于 0".into()));
                }
                Duration::from_secs(seconds)
            }
            Err(_) => Duration::from_secs(60),
        };

        Ok(Self {
            base_url,
            api_key,
            default_model,
            timeout,
        })
    }
}

fn required_env(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LlmError::Config(format!("缺少环境变量 {name}")))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Developer,
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Message {
    pub fn text(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(content.into()),
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            extra: BTreeMap::new(),
        }
    }

    pub fn developer(content: impl Into<String>) -> Self {
        Self::text(Role::Developer, content)
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::text(Role::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::text(Role::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text(Role::Assistant, content)
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            name: None,
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FunctionCall {
    pub name: String,
    /// API 返回的是 JSON 字符串，由主体 Agent 决定何时反序列化。
    pub arguments: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<Value>,
    /// 供应商扩展参数会被直接合并到请求 JSON 顶层。
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            stream: false,
            temperature: None,
            max_completion_tokens: None,
            tools: Vec::new(),
            tool_choice: None,
            response_format: None,
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChatResponse {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ChatResponse {
    pub fn first_text(&self) -> Option<&str> {
        self.choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Choice {
    #[serde(default)]
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChatChunk {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    pub usage: Option<Usage>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChunkChoice {
    #[serde(default)]
    pub index: u32,
    pub delta: MessageDelta,
    pub finish_reason: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageDelta {
    pub role: Option<Role>,
    pub content: Option<String>,
    /// 流式工具参数可能是分片字符串，因此保留供应商原始 JSON。
    #[serde(default)]
    pub tool_calls: Vec<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
