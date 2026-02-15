use crate::run::run_script;
use crate::utils::print_v;
use rquickjs::Ctx;
use std::io::Write;

use tokio::io::{AsyncBufReadExt, BufReader};

/// Simple REPL
pub async fn repl(ctx: Ctx<'_>) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    loop {
        let script = read_multiline_input(&mut reader).await?;
        if !script.is_empty() {
            match run_script(ctx.clone(), script).await {
                Ok(v) => {
                    if !v.is_undefined() {
                        ctx.globals().set("_", v.clone())?;
                        let _ = print_v(ctx.clone(), v);
                    }
                }
                Err(e) => eprintln!("{e}"),
            }
        }
    }
}

/// Readline REPL
const PROMPT: &str = ">>> ";
const MULTILINE_PROMPT: &str = "... ";
const RL_CHANNEL_CAPACITY: usize = 1;

pub async fn repl_rl(ctx: Ctx<'_>) -> anyhow::Result<()> {
    use rustyline::{error::ReadlineError, DefaultEditor};

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel::<String>(RL_CHANNEL_CAPACITY);
    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::channel::<()>(RL_CHANNEL_CAPACITY);

    // Need to spawn blocking task as rustyline is sync
    let input_handle = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut rl = DefaultEditor::new()?;
        let mut lines = Vec::new();
        let mut prompt = PROMPT;
        loop {
            match rl.readline(prompt) {
                Ok(line) => {
                    lines.push(line.to_string());
                    let cmd = lines.join("\n");
                    // Check if we need more input (unmatched braces/parens)
                    if needs_more_input(&cmd) {
                        prompt = MULTILINE_PROMPT;
                    } else {
                        if !cmd.is_empty() {
                            rl.add_history_entry(cmd.as_str())?;
                        }
                        if cmd_tx.blocking_send(cmd).is_err() {
                            // Channel closed
                            break;
                        }
                        // Wait for reply
                        if reply_rx.blocking_recv().is_none() {
                            // Channel closed
                            break;
                        }
                        lines.clear();
                        prompt = PROMPT;
                    };
                }
                Err(ReadlineError::Interrupted) => {
                    eprintln!("<CTRL-C>");
                    break;
                }
                Err(ReadlineError::Eof) => {
                    eprintln!("<CTRL-D>");
                    break;
                }
                Err(e) => {
                    eprintln!("[-] Readline Error: {:?}", e);
                    break;
                }
            }
        }
        Ok(())
    });

    // Get input cmd
    while let Some(cmd) = cmd_rx.recv().await {
        match run_script(ctx.clone(), cmd).await {
            Ok(v) => {
                if !v.is_undefined() {
                    ctx.globals().set("_", v.clone())?;
                    let _ = print_v(ctx.clone(), v);
                }
            }
            Err(e) => eprintln!("[-] JS Error: {e}"),
        }
        reply_tx.send(()).await?;
    }

    let _ = input_handle.await?;
    Ok(())
}

async fn read_multiline_input(reader: &mut BufReader<tokio::io::Stdin>) -> anyhow::Result<String> {
    let mut lines = Vec::new();
    let mut buffer = String::new();

    loop {
        let prompt = if lines.is_empty() { ">>> " } else { "... " };
        print!("{}", prompt);
        std::io::stdout().flush()?;

        buffer.clear();
        reader.read_line(&mut buffer).await?;
        let line = buffer.trim_end();

        lines.push(line.to_string());

        let full_input = lines.join("\n");
        // Check if we need more input (unmatched braces/parens)
        if !needs_more_input(&full_input) {
            return Ok(full_input);
        }
    }
}

fn needs_more_input(input: &str) -> bool {
    let mut balance = 0i32;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' | '(' | '[' => balance += 1,
            '}' | ')' | ']' => {
                balance -= 1;
                if balance < 0 {
                    return false;
                } // Syntax error
            }
            '"' | '\'' => {
                // Skip string literals
                let quote = ch;
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        // Skip escaped chars
                        chars.next();
                    } else if c == quote {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    balance > 0
}
