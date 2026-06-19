use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use tokio::time::Duration;

use rquickjs::{
    function::{Rest, This},
    promise::PromiseState,
    Ctx, Exception, Function, Object, Type, Value,
};

/// Register useful QJS functions
pub fn register_fns(ctx: &Ctx<'_>) -> anyhow::Result<()> {
    ctx.globals().set("__debug", js_debug)?;
    ctx.globals().set("__print_json", js_print_json)?;
    ctx.globals().set("__sleep", js_sleep)?;
    ctx.globals().set("__globals", js_globals)?;
    ctx.globals().set("__to_buffer", js_to_buffer)?;
    ctx.globals().set("__to_utf8", js_to_utf8)?;
    ctx.globals().set("setTimeout", js_set_timeout)?;
    ctx.globals().set("setInterval", js_set_interval)?;
    ctx.globals()
        .set("setTimeoutCancel", js_set_timeout_cancel)?;
    ctx.globals()
        .set("setIntervalCancel", js_set_interval_cancel)?;
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

/// Print JS Values as JSON
#[rquickjs::function]
pub fn print_json<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<()> {
    let mut s = Vec::<String>::new();
    for v in args.iter() {
        s.push(
            ctx.json_stringify(v)?
                .and_then(|s| s.as_string().map(|s| s.to_string().ok()).flatten())
                .unwrap_or_else(|| format!("{:?}", &v)),
        )
    }
    println!("{}", s.join(" "));
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

/// Escape a string for printing inside double quotes (JSON-style).
/// Borrows the input unchanged when nothing needs escaping (the common case).
fn escape_str(s: &str) -> Cow<'_, str> {
    // Fast path: no changes needed
    if !s.bytes().any(|b| b < 0x20 || b == b'"' || b == b'\\') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            // Any other C0 control char -> \u00XX
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

const MAX_DEPTH: usize = 10;
const ERR: &str = "<ERR>";

fn bigint_to_string<'js>(v: &Value<'js>) -> Option<String> {
    // BigInt has no Rust accessor; call JS BigInt.prototype.toString() with `this` = v.
    v.ctx()
        .globals()
        .get::<_, Object>("BigInt")
        .and_then(|o| o.get::<_, Object>("prototype"))
        .and_then(|p| p.get::<_, Function>("toString"))
        .and_then(|f| f.call::<_, String>((This(v),)))
        .ok()
}

pub fn log_v<'js>(v: &Value<'js>, quote: bool, depth: usize) -> String {
    if depth > MAX_DEPTH {
        return "<Max-Depth Exceeded>".into();
    }

    match v.type_of() {
        Type::Undefined => "undefined".into(),
        Type::Null => "null".into(),
        Type::Bool => v
            .as_bool()
            .map_or(ERR, |b| if b { "true" } else { "false" })
            .into(),
        Type::Int => v.as_int().map_or_else(|| ERR.into(), |i| i.to_string()),
        Type::Float => v.as_float().map_or_else(|| ERR.into(), |f| f.to_string()),
        Type::String => match v.as_string().and_then(|s| s.to_string().ok()) {
            None => ERR.into(),
            Some(s) if quote => format!("\"{}\"", escape_str(&s)),
            Some(s) => s,
        },
        Type::Symbol => "Symbol()".into(),
        Type::Constructor => "Constructor()".into(),
        Type::Function => "Function()".into(),
        Type::Array => v.as_array().map_or_else(
            || ERR.into(),
            |arr| {
                let items = arr
                    .iter()
                    .map(|r| r.map_or_else(|_| ERR.into(), |v| log_v(&v, true, depth + 1)))
                    .collect::<Vec<_>>();
                format!("[{}]", items.join(", "))
            },
        ),
        Type::Promise => v
            .as_promise()
            .map_or(ERR, |p| match p.state() {
                PromiseState::Pending => "Promise(<pending>)",
                PromiseState::Resolved => "Promise(<resolved>)",
                PromiseState::Rejected => "Promise(<rejected>)",
            })
            .into(),
        Type::Exception => v
            .as_exception()
            .and_then(|e| e.message())
            .map_or_else(|| ERR.into(), |m| format!("Error<{}>", m)),
        Type::BigInt => bigint_to_string(v).unwrap_or_else(|| ERR.into()),
        Type::Object => v.as_object().map_or_else(
            || ERR.into(),
            |obj| {
                let items = obj
                    .props::<_, Value<'_>>()
                    .map(|p| {
                        p.map_or_else(
                            |_| ERR.into(),
                            |(k, v)| {
                                format!(
                                    "{}: {}",
                                    log_v(&k, true, depth + 1),
                                    log_v(&v, true, depth + 1)
                                )
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                format!("{{{}}}", items.join(", "))
            },
        ),
        _ => v.type_name().into(),
    }
}

/// console.log
#[rquickjs::function]
fn log<'js>(_ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<()> {
    println!(
        "{}",
        args.iter()
            .map(|a| log_v(a, false, 0))
            .collect::<Vec<String>>()
            .join(" ")
    );
    Ok(())
}

#[rquickjs::function]
async fn sleep(n: u64) -> rquickjs::Result<()> {
    tokio::time::sleep(Duration::from_secs(n)).await;
    Ok(())
}

#[rquickjs::function]
pub fn set_timeout<'js>(
    ctx: rquickjs::Ctx<'js>,
    f: rquickjs::Function<'js>,
    delay_ms: u64,
    args: Rest<Value<'js>>,
) -> rquickjs::Result<()> {
    let mut arg = rquickjs::function::Args::new(ctx.clone(), args.len());
    arg.push_args(args.iter())?;

    let _handle = ctx.spawn({
        async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            let _ = f.call_arg::<()>(arg);
        }
    });

    Ok(())
}

#[rquickjs::function]
pub fn set_interval<'js>(
    ctx: rquickjs::Ctx<'js>,
    f: rquickjs::Function<'js>,
    delay_ms: u64,
    args: Rest<Value<'js>>,
) -> rquickjs::Result<()> {
    let _handle = ctx.spawn({
        let ctx = ctx.clone();
        async move {
            loop {
                let mut arg = rquickjs::function::Args::new(ctx.clone(), args.len());
                let _ = arg.push_args(args.iter());
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                let _ = f.call_arg::<()>(arg);
            }
        }
    });

    Ok(())
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

/*
    Cancellable versions of setTimeout / setInterval

    NOTE:

    For functions that return a oneshot based cancel function there is a concurrency issue if
    the cancel function is not caputured to a variable when ctx.eval() completes and the JS runtime
    runs to completion as a future using rt.idle() or rt.execute_pending_job().

        Error: <<oneshot: called after complete>>

    Either ensure that the cancel function is captured or just use the simpler non-cancel functions
    if the cancel function is not required
*/

/// setTimeout -> returns cancel function (using oneshot)
#[rquickjs::function]
pub fn set_timeout_cancel<'js>(
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
pub fn set_interval_cancel<'js>(
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
