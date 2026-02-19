use std::sync::{Arc, Mutex};

use tokio::time::Duration;

use rquickjs::{function::Rest, Ctx, Exception, Function, Object, Value};

/// Register useful QJS functions
pub fn register_fns(ctx: &Ctx<'_>) -> anyhow::Result<()> {
    ctx.globals().set("__debug", js_debug)?;
    ctx.globals().set("__print", js_print)?;
    ctx.globals().set("__print_v", js_print_v)?;
    ctx.globals().set("__sleep", js_sleep)?;
    ctx.globals().set("__globals", js_globals)?;
    ctx.globals().set("__to_buffer", js_to_buffer)?;
    ctx.globals().set("__to_utf8", js_to_utf8)?;
    ctx.globals().set("setTimeout", js_set_timeout)?;
    ctx.globals().set("setInterval", js_set_interval)?;
    // Add console.log function
    let console = Object::new(ctx.clone())?;
    console.set("log", js_log)?;
    ctx.globals().set("console", console)?;
    // Add to_utf8 / to_buffer prototype methods
    ctx.eval::<(),_>(r#"
        Object.defineProperty(String.prototype, "to_buffer", { value: function () { return __to_buffer(this) }});
        Object.defineProperty(ArrayBuffer.prototype, "to_utf8", { value: function() { return __to_utf8(this) }});
    "#)?;
    Ok(())
}

/// Debug JS Value
#[rquickjs::function]
fn debug(v: Value<'_>) {
    println!("{:?}", v);
}

/// Print JS String
#[rquickjs::function]
fn print(s: String) {
    println!("{}", s);
}

/// Convert Value to JSON String
pub fn value_to_json<'js>(ctx: Ctx<'js>, v: Value<'js>) -> anyhow::Result<String> {
    if v.is_undefined() {
        Ok("null".into())
    } else {
        ctx.json_stringify(v)?
            .and_then(|s| s.as_string().map(|s| s.to_string().ok()).flatten())
            .ok_or_else(|| anyhow::anyhow!("JSON Error"))
    }
}

/// Convert JSON String to Value
pub fn json_to_value<'js>(ctx: Ctx<'js>, json: &str) -> anyhow::Result<Value<'js>> {
    match ctx.json_parse(json.as_bytes()) {
        Ok(v) => Ok(v),
        Err(e) => {
            if let Ok(ex) = rquickjs::Exception::from_value(ctx.catch()) {
                Err(anyhow::anyhow!(
                    "JSON Error: {}\n{}",
                    ex.message().unwrap_or("-".into()),
                    ex.stack().unwrap_or("-".into())
                ))
            } else {
                Err(anyhow::anyhow!("JSON Error: {e}"))
            }
        }
    }
}

/// Print JS Value as JSON
#[rquickjs::function]
pub fn print_v<'js>(ctx: Ctx<'js>, v: Value<'js>) -> rquickjs::Result<()> {
    let output = ctx
        .json_stringify(&v)?
        .and_then(|s| s.as_string().map(|s| s.to_string().ok()).flatten())
        .unwrap_or_else(|| format!("{:?}", &v));
    println!("{}", output);
    Ok(())
}

/// Print globals
#[rquickjs::function]
fn globals<'js>(ctx: Ctx<'js>) -> rquickjs::Result<()> {
    let mut i = ctx.globals().props::<String, rquickjs::Value>();
    while let Some(Ok((k, v))) = i.next() {
        println!("{} => {:?}", k, v);
    }
    Ok(())
}

/// String to ArrayBuffer
/// JS: Object.defineProperty(String.prototype, "to_buffer", { value: function () { return __to_buffer(this) }})
#[rquickjs::function]
fn to_buffer<'js>(ctx: Ctx<'js>, s: String) -> rquickjs::Result<rquickjs::ArrayBuffer<'js>> {
    rquickjs::ArrayBuffer::new_copy(ctx.clone(), s.as_bytes())
}

/// ArrayBuffer to UTF8
/// JS: Object.defineProperty(ArrayBuffer.prototype, "to_utf8", { value: function() { return __to_utf8(this) }})
#[rquickjs::function]
fn to_utf8<'js>(ctx: Ctx<'js>, a: rquickjs::ArrayBuffer<'js>) -> rquickjs::Result<String> {
    let bytes = a
        .as_bytes()
        .ok_or_else(|| rquickjs::Exception::throw_message(&ctx, "Invalid ArrayBuffer"))?
        .to_vec();
    Ok(String::from_utf8(bytes)?)
}

/// console.log
#[rquickjs::function]
fn log<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<()> {
    println!(
        "{}",
        args.iter()
            .map(|a| -> rquickjs::Result<String> {
                Ok(ctx
                    .json_stringify(a)?
                    .and_then(|s| s.as_string().map(|s| s.to_string().ok()).flatten())
                    .unwrap_or_else(|| format!("{:?}", &a)))
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(", ")
    );
    Ok(())
}

#[rquickjs::function]
async fn sleep(n: u64) -> rquickjs::Result<()> {
    tokio::time::sleep(Duration::from_secs(n)).await;
    Ok(())
}

/*
    NOTE:

    For functions that return a oneshot based cancel function there is a concurrency issue if if
    the cancel function is not caputured to a variable when ctx.eval() completes and the JS runtime
    runs to completion as a future using rt.idle() or rt.execute_pending_job().

        Error: <<oneshot: called after complete>>

    Either ensure that the cancel function is captured or just use the simpler non-cancel functions
    if the cancel function is not required
*/

/// setTimeout -> returns cancel function (using oneshot)
#[rquickjs::function]
fn set_timeout<'js>(
    ctx: rquickjs::Ctx<'js>,
    f: rquickjs::Function<'js>,
    delay_ms: u64,
    args: Rest<Value<'js>>,
) -> rquickjs::Result<Function<'js>> {
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    // Need to move cancel_tx into Arc<Mutex<Option>>> to ensure that cancel_f if Fn
    let cancel_tx = Arc::new(Mutex::new(Some(cancel_tx)));
    let cancel_f = Function::new(ctx.clone(), move |ctx| -> rquickjs::Result<()> {
        cancel_tx
            .lock()
            .map_err(|_| Exception::throw_message(&ctx, "Mutex Locked"))?
            .take()
            .ok_or_else(|| Exception::throw_message(&ctx, "Already Cancelled"))?
            .send(())
            .map_err(|_| Exception::throw_message(&ctx, "Oneshot Channel Closed"))
    })?;

    let mut arg = rquickjs::function::Args::new(ctx.clone(), args.len());
    arg.push_args(args.iter())?;

    let _handle = ctx.spawn({
        async move {
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)) => {
                    let _ = f.call_arg::<()>(arg);
                }
                Ok(()) = cancel_rx => {
                }
            }
        }
    });

    Ok(cancel_f)
}

#[rquickjs::function]
fn set_interval<'js>(
    ctx: rquickjs::Ctx<'js>,
    f: rquickjs::Function<'js>,
    delay_ms: u64,
    args: Rest<Value<'js>>,
) -> rquickjs::Result<Function<'js>> {
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    // Need to move cancel_tx into Arc<Mutex<Option<_>>>> to ensure that cancel_f closure is Fn
    let cancel_tx = Arc::new(Mutex::new(Some(cancel_tx)));
    let cancel_f = Function::new(ctx.clone(), move |ctx| -> rquickjs::Result<()> {
        cancel_tx
            .lock()
            .map_err(|_| Exception::throw_message(&ctx, "Mutex Locked"))?
            .take()
            .ok_or_else(|| Exception::throw_message(&ctx, "Already Cancelled"))?
            .send(())
            .map_err(|_| Exception::throw_message(&ctx, "Oneshot Channel Closed"))
    })?;

    let _handle = ctx.spawn({
        let ctx = ctx.clone();
        async move {
            let mut cancel_rx = cancel_rx;
            loop {
                let mut arg = rquickjs::function::Args::new(ctx.clone(), args.len());
                let _ = arg.push_args(args.iter());
                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)) => {
                        let _ = f.call_arg::<()>(arg);
                    }
                    Ok(()) = &mut cancel_rx => {
                        break;
                    }
                }
            }
        }
    });

    Ok(cancel_f)
}
