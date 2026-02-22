use reqwest::{Client, Method, Response};
use rquickjs::{
    class::Trace, function::Rest, ArrayBuffer, Ctx, Exception, JsLifetime, Object, Value,
};

use std::str::FromStr;

#[derive(Debug, Trace, JsLifetime)]
#[rquickjs::class(rename = "Response")]
pub struct FetchResponse {
    #[qjs(skip_trace)]
    response: Response,
}

#[rquickjs::methods]
impl FetchResponse {
    /// Need to have constructor to register class
    #[qjs(constructor)]
    pub fn new() -> () {}

    pub fn debug(&self) -> String {
        format!("Response: {:?}", self.response)
    }
}

/// Simplified fetch function
/// Only supports - fetch(url,[options]) arguments
///               - { method, headers, body } options
#[rquickjs::function]
pub async fn fetch<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<FetchResponse> {
    // Get url
    let url = args
        .get(0)
        .and_then(|v| v.as_string())
        .and_then(|s| s.to_string().ok())
        .ok_or_else(|| Exception::throw_message(&ctx, "Invalid URL"))?;

    // Get properties
    let default = Object::new(ctx.clone())?;
    let properties = args.get(1).and_then(|v| v.as_object()).unwrap_or(&default);

    let method = Method::from_str(
        &properties
            .get::<_, String>("method")
            .unwrap_or("GET".to_string()),
    )
    .map_err(|_| Exception::throw_message(&ctx, "Invalid Method"))?;

    let client = Client::new();
    let mut builder = client.request(method, url);

    // Headers
    if let Ok(headers) = properties.get::<_, Object<'js>>("headers") {
        let mut i = headers.props::<String, String>();
        while let Some(Ok((k, v))) = i.next() {
            builder = builder.header(k, v);
        }
    }

    // Body (string)
    if let Ok(body) = properties.get::<_, String>("body") {
        builder = builder.body(body);
    }
    // Body (arraybuffer)
    if let Ok(body) = properties.get::<_, ArrayBuffer<'_>>("body") {
        let body = body
            .as_bytes()
            .ok_or_else(|| Exception::throw_message(&ctx, "Invalid Body"))?
            .to_vec();
        builder = builder.body(body);
    }

    let response = builder
        .send()
        .await
        .map_err(|e| Exception::throw_message(&ctx, &format!("Fetch Error: {e}")))?;

    Ok(FetchResponse { response })
}

pub fn register_fetch(ctx: &Ctx) -> rquickjs::Result<()> {
    rquickjs::Class::<FetchResponse>::define(&ctx.globals())?;
    ctx.globals().set("fetch", js_fetch)?;
    Ok(())
}
