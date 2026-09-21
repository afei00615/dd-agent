use std::{path::Path, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{process::Command, time::{timeout}};



/// 大模型参数
#[derive(Debug,Deserialize)]
pub struct ShellArgs {
    pub command :String
}

#[derive(Debug,Serialize)]
#[serde(rename_all="lowercase")]
pub enum ToolType {
    // 函数调用
    Function
}

#[derive(Debug,Serialize)]
pub struct ToolDefinition {
    #[serde(rename="type")]
    pub tool_type : ToolType,
    pub function: FunctionDefinition,

}

#[derive(Debug,Serialize)]
pub struct FunctionDefinition {
    pub name : String,
    pub description : String,
    pub strict : bool,
    /**
     * {
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
     */
    pub parameters : Value,
    
}
/// 策略判定
pub enum PolicyDecision {
    Allow,
    RequestApproval{reason:String},
    Deny {reason : String}
}


pub fn load_tools() -> Vec<Value> {
    let power_shell_tools = ToolDefinition {
        tool_type : ToolType::Function,
        function : FunctionDefinition { 
            name: String::from("run_shell"), 
            description: String::from("在受限的本地工作目录执行 PowerShell"), 
            strict: true, 
            parameters: json!(
            {
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
            ) }
    };
    vec![json!(power_shell_tools)]

}

pub async  fn run_power_shell(args : &str,workspace: &Path) ->Result<String,Box<dyn std::error::Error>> {
    let args :ShellArgs = serde_json::from_str(args)?;
    if args.command.len() > 4096 {
        return Err("命令太长".into());
    }

    let mut command = Command::new("pwsh.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &args.command,
    ]);
    command.current_dir(workspace);
    command.kill_on_drop(true);

    let output = timeout(Duration::from_secs(40), command.output()).await.map_err(|_|"命令超时")??;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(json!({
        "success": output.status.success(),
        "exit_code": output.status.code(),
        "stdout": truncate(&stdout, 16_000),
        "stderr": truncate(&stderr, 16_000)
    }).to_string())
}

fn check_power_shell(command : &str) -> PolicyDecision {
    let cmd = command.trim().to_lowercase();
    let denied = [
        "invoke-expression",
        "iex ",
        "-encodedcommand",
        "set-executionpolicy",
        "add-mppreference",
        "clear-disk",
        "format-volume",
        "remove-partition",
    ];
    if let Some(pattern) = denied.iter().find(|pattern| cmd.contains(*pattern)) {
        return PolicyDecision::Deny {
            reason: format!("command contains denied pattern: {pattern}"),
        };
    }
    PolicyDecision::Allow
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}
