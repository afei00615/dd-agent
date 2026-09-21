use std::io::{self, Write};

use dd_agent::{ChatCompletionApi, 
    LlmConfig, Message, OpenAiCompatibleClient, load_tools,run_power_shell};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 读取.env文件
    dotenvy::dotenv().ok();
    let config = LlmConfig::from_env()?;
    let client = OpenAiCompatibleClient::new(config)?;

    // if prompt.trim().is_empty() {
    //     eprintln!("用法：cargo run -- \"你好，请介绍一下自己\"");
    //     std::process::exit(2);
    // }

    // let response = client
    //     .chat(client.request(vec![Message::user(prompt)]))
    //     .await?;
    // println!("{}", response.first_text().unwrap_or(""));
    println!("我是dd小助手，请输入你的问题：");
    let mut messages = vec![Message::system(
        "你是本地agent助手，做危险的操作时，需要获取用户的授权",
    )];
    
    loop {
        io::stdout().flush().unwrap();
        let mut input = String::new();
        
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let input = input.trim_end();
                if "exit" == input {
                    break;
                }
                println!("收到输入:{}", input);
                messages.push(Message::user(input));

                for _round in 0..8 {
                    let mut request = client.request(messages.clone());
                    request.tools = load_tools();
                    let response = client.chat(request).await?;
                    let choice = response.choices.into_iter().next().ok_or("大模型返回空")?;
                    // 返回的message role都为assistant
                    let assistant = choice.message;
                    let has_tool_call = !assistant.tool_calls.is_empty();
                    messages.push(assistant.clone());
                    if has_tool_call {
                        println!("有工具调用{:?}",assistant.tool_calls);
                        for call in assistant.tool_calls {
                            let result = match call.function.name.as_str() {
                                "run_shell" => {
                                    match run_power_shell(
                                        &call.function.arguments,
                                        std::path::Path::new(r"E:\download")
                                    ).await {
                                        Ok(output) => output,
                                        Err(error) => json!({
                                            "success": false,
                                            "error" :error.to_string()
                                        }).to_string(),
                                    }
                                }
                                unknown => json!({
                                    "success": false,
                                    "error": format!("未知工具：{unknown}")
                                })
                                .to_string(),
                                
                            };
                            messages.push(Message::tool(call.id, result));
                        }
                        continue;
                    }
                    match choice.finish_reason.as_deref() {
                        Some("stop") => {
                            println!("finish reason is stop");
                            println!("{}", assistant.content.unwrap_or_default());
                            break;
                        }
                        None => {
                            println!("响应异常");
                            break;
                        }
                        Some(reason) => {
                            println!("模型未正常结束：{reason}");
                            break;
                        }
                    }
                }
            }
            Err(_) => {
                println!("出错，立刻退出");
                break;
            }
        }
    }
    Ok(())
}
