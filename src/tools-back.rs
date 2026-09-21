use std::{path::Path, time::Duration};

use serde::Deserialize;
use serde_json::{json,Value};
use tokio::{process::Command, time::{timeout}};

pub fn load_tools() -> Vec<Value> {
    vec![power_shell_tool()]
}

fn power_shell_tool() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "run_shell",
            "description": "在受限的本地工作目录执行 PowerShell ",
            "strict": true,
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "要执行的命令"
                    }
                },
                "required": [
                    "command"
                ],
                "additionalProperties": false
            }
        }
    })
}

#[derive(Debug,Deserialize)]
struct ShellArgs {
    // dir:String,
    // shell: ShellKind,
    command: String,
}


pub async fn run_shell(arguments : &str,
    workspace: &Path) -> Result<String,Box<dyn std::error::Error>> {
    let args: ShellArgs = serde_json::from_str(arguments)?;
    if args.command.len() > 4096 {
        return Err("太长了".into());
    }
    // 授权 TODO
    // authorize_command(&args)?;

    let mut command = Command::new("pwsh.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &args.command,
    ]);  
    command.current_dir(workspace);
    command.kill_on_drop(true);
    println!("run command:{:?}",command);
    let output = timeout(
        Duration::from_secs(30), command.output()).await.map_err(|_| "命令执行超时")??;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(json!({
        "success": output.status.success(),
        "exit_code": output.status.code(),
        "stdout": truncate(&stdout, 16_000),
        "stderr": truncate(&stderr, 16_000)
    }).to_string())
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}