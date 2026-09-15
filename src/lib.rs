//! `dd-agent` 的大模型 API 基础层。
//!
//! 当前实现面向 OpenAI Chat Completions 兼容接口，业务 Agent 可以只依赖
//! [`ChatCompletionApi`] trait，后续替换模型供应商时无需改动主体逻辑。

mod client;
mod error;
mod session;
mod types;

pub use client::{ChatCompletionApi, ChatStream, OpenAiCompatibleClient};
pub use error::{LlmError, Result};
pub use types::*;
