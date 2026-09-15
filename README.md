# dd-agent

一个面向 Agent 主体的 Rust 大模型 API 基础层。目前支持 OpenAI Chat Completions 兼容协议：

- 普通响应与 SSE 流式响应
- Bearer Token 认证（也允许无认证的本地模型）
- 工具调用消息的数据结构
- HTTP 状态、服务端错误消息、网络和反序列化错误分层
- 供应商自定义请求/响应字段
- 可替换、可 mock 的 `ChatCompletionApi` trait

## 快速开始

PowerShell：

```powershell
$env:LLM_BASE_URL = "https://api.openai.com/v1"
$env:LLM_API_KEY = "你的 API Key"
$env:LLM_MODEL = "你有权限使用的模型名"
cargo run -- "你好，请用一句话介绍自己"
```

程序只读取环境变量，不会自动读取 `.env` 文件；`.env` 已加入 `.gitignore`，避免误提交密钥。

## 在主体 Agent 中调用

```rust
use dd_agent::{ChatCompletionApi, LlmConfig, Message, OpenAiCompatibleClient};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = OpenAiCompatibleClient::new(LlmConfig::from_env()?)?;
let request = client.request(vec![
    Message::developer("你是一个可靠的编程助手"),
    Message::user("解释 Rust 的所有权"),
]);
let response = client.chat(request).await?;
println!("{}", response.first_text().unwrap_or(""));
# Ok(())
# }
```

流式调用返回 `ChatStream`，可用 `futures_util::StreamExt` 逐块消费：

```rust
use futures_util::StreamExt;
# use dd_agent::{ChatCompletionApi, LlmConfig, Message, OpenAiCompatibleClient};
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
# let client = OpenAiCompatibleClient::new(LlmConfig::from_env()?)?;
let mut stream = client
    .chat_stream(client.request(vec![Message::user("你好")]))
    .await?;
while let Some(chunk) = stream.next().await {
    for choice in chunk?.choices {
        if let Some(text) = choice.delta.content {
            print!("{text}");
        }
    }
}
# Ok(())
# }
```

`ChatRequest::extra` 可传入兼容服务商的顶层扩展参数；`tools`、`tool_choice` 和工具调用响应结构已经预留，后续主体部分可直接实现工具执行循环。
