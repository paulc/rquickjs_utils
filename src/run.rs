use std::io::Read;

use anyhow::anyhow;
use rquickjs::{prelude::IntoArgs, CatchResultExt, Ctx, Function, Module, Value};

/// If globalThis.__resolve_promise == true check if `v` is a promise 
/// await completion and return the resolved value 
pub async fn resolve_promise<'js>(ctx: &Ctx<'js>, v: Value<'js>) -> anyhow::Result<Value<'js>> {
    if let Some(true) = ctx.globals().get::<_, bool>("__resolve_promise").ok() && v.is_promise() {
        let promise = v.into_promise().expect("checked is_promise");
        promise
            .into_future::<Value<'js>>()
            .await
            .catch(ctx)
            .map_err(|e| anyhow::anyhow!("{e}"))
    } else {
        Ok(v)
    }
}

pub fn set_resolve_promise<'js>(ctx: &Ctx<'js>, v: bool) -> anyhow::Result<()> {
    ctx.globals().set::<_, bool>("__resolve_promise", v).map_err(|e| anyhow::anyhow!("Error setting __resolve_promise: {e}"))
}

/// Expand script arg to handle literal script, @file or stdin (-)
pub fn get_script(script: &str) -> anyhow::Result<String> {
    Ok(if script == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else if script.starts_with("@") {
        std::fs::read_to_string(&script[1..])?
    } else {
        script.to_string()
    })
}

/// Run as script
pub async fn run_script<'js>(ctx: Ctx<'js>, script: String) -> anyhow::Result<Value<'js>> {
    match ctx.eval::<Value, _>(script) {
        Ok(v) => Ok(v),
        Err(e) => {
            if let Ok(ex) = rquickjs::Exception::from_value(ctx.catch()) {
                Err(anyhow!(
                    "{}\n{}",
                    ex.message().unwrap_or("-".into()),
                    ex.stack().unwrap_or("-".into())
                ))
            } else {
                Err(anyhow!("JS Error: {e}"))
            }
        }
    }
}

/// Run as module
pub async fn run_module(ctx: Ctx<'_>, module: String) -> anyhow::Result<()> {
    // Declare module
    let module = Module::declare(ctx.clone(), "main.mjs", module)
        .catch(&ctx)
        .map_err(|e| anyhow!("JS error [declare]: {}", e))?;

    // Evaluate module
    let (_module, promise) = module
        .eval()
        .catch(&ctx)
        .map_err(|e| anyhow!("JS error [eval]: {}", e))?;

    // Complete promise as future
    promise
        .into_future::<()>()
        .await
        .catch(&ctx)
        .map_err(|e| anyhow!("JS error [await]: {}", e))?;

    Ok(())
}

/// Call JS fn
pub async fn call_fn<'js, A>(ctx: Ctx<'js>, path: &str, args: A) -> anyhow::Result<Value<'js>>
where
    A: IntoArgs<'js>,
{
    // Resolve function
    let obj: Function = ctx
        .eval(path)
        .map_err(|e| anyhow::anyhow!("Invalid Path: {path} [{e}]"))?;
    // We dont resolve promises - these will be handled by the pending tasks loop
    let r = obj.call::<A, Value>(args)?;
    Ok(r)
}
