use reqwest::{header::HeaderMap, header::HeaderName, Client, Method, Response};
use rquickjs::{
    class::Trace, function::Rest, Array, ArrayBuffer, Ctx, Exception, Function, Iterable,
    JsLifetime, Object, Value,
};

use std::str::FromStr;

#[derive(Debug, Trace, JsLifetime)]
#[rquickjs::class(rename = "Response")]
pub struct FetchResponse {
    #[qjs(skip_trace)]
    response: Option<Response>,
    #[qjs(get, skip_trace)]
    headers: FetchHeaders,
    #[qjs(get)]
    ok: bool,
    #[qjs(get)]
    status: u16,
    #[qjs(get, rename = "statusText")]
    status_text: Option<String>,
    #[qjs(get)]
    url: String,
}

#[rquickjs::methods]
impl FetchResponse {
    pub async fn text(&mut self, ctx: Ctx<'_>) -> rquickjs::Result<String> {
        match self.response.take() {
            Some(r) => r
                .text()
                .await
                .map_err(|e| Exception::throw_message(&ctx, &format!("Response Error: {e}"))),
            None => Err(Exception::throw_message(&ctx, "Body already consumed")),
        }
    }
    #[qjs(rename = "arrayBuffer")]
    pub async fn array_buffer<'js>(&mut self, ctx: Ctx<'js>) -> rquickjs::Result<ArrayBuffer<'js>> {
        match self.response.take() {
            Some(r) => ArrayBuffer::new_copy(
                ctx.clone(),
                r.bytes()
                    .await
                    .map_err(|e| Exception::throw_message(&ctx, &format!("Response Error: {e}")))?,
            ),
            None => Err(Exception::throw_message(&ctx, "Body already consumed")),
        }
    }
    pub async fn bytes<'js>(&mut self, ctx: Ctx<'js>) -> rquickjs::Result<ArrayBuffer<'js>> {
        self.array_buffer(ctx).await
    }
    pub async fn json<'js>(&mut self, ctx: Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        match self.response.take() {
            Some(r) => {
                let b = r
                    .bytes()
                    .await
                    .map_err(|e| Exception::throw_message(&ctx, &format!("Response Error: {e}")))?;
                ctx.json_parse(b)
            }
            None => Err(Exception::throw_message(&ctx, "Body already consumed")),
        }
    }
    #[qjs(rename = "__debug")]
    pub fn debug(&self) -> String {
        format!("FetchResponse: {:?}", self)
    }
}

#[derive(Debug, Clone, Trace, JsLifetime)]
#[rquickjs::class(rename = "Headers")]
pub struct FetchHeaders {
    #[qjs(skip_trace)]
    headers: HeaderMap,
}

#[rquickjs::methods]
impl FetchHeaders {
    #[qjs(constructor)]
    pub fn new() -> Self {
        Self {
            headers: HeaderMap::new(),
        }
    }
    pub fn append(&mut self, ctx: Ctx<'_>, name: String, value: String) -> rquickjs::Result<()> {
        self.headers.append(
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| Exception::throw_message(&ctx, "Invalid Header Name"))?,
            value
                .try_into()
                .map_err(|_| Exception::throw_message(&ctx, "Invalid Header Value"))?,
        );
        Ok(())
    }
    pub fn delete(&mut self, ctx: Ctx<'_>, name: String) -> rquickjs::Result<()> {
        self.headers.remove(
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| Exception::throw_message(&ctx, "Invalid Header Name"))?,
        );
        Ok(())
    }
    pub fn entries<'js>(&mut self, ctx: Ctx<'js>) -> rquickjs::Result<Iterable<Vec<Array<'js>>>> {
        let mut entries: Vec<Array> = Vec::new();
        for (k, v) in self.headers.iter() {
            let entry = Array::new(ctx.clone())?;
            entry.set(0, k.to_string())?;
            entry.set(1, String::from_utf8_lossy(v.as_bytes()).to_string())?;
            entries.push(entry);
        }
        Ok(Iterable::from(entries))
    }
    #[qjs(rename = "forEach")]
    pub fn for_each(&self, _ctx: Ctx<'_>, f: Function<'_>) -> rquickjs::Result<()> {
        for (k, v) in self.headers.iter() {
            let k = k.to_string();
            let v = String::from_utf8_lossy(v.as_bytes()).to_string();
            let _ = f.call::<_, ()>((k, v));
        }
        Ok(())
    }
    pub fn get(&self, name: String) -> Option<String> {
        self.headers
            .get(name)
            .map(|v| String::from_utf8_lossy(v.as_bytes()).to_string())
    }
    pub fn has(&self, name: String) -> bool {
        self.headers.contains_key(name)
    }
    pub fn keys(&mut self) -> rquickjs::Result<Iterable<Vec<String>>> {
        let keys = self
            .headers
            .keys()
            .map(|k| k.to_string())
            .collect::<Vec<String>>();
        Ok(Iterable::from(keys))
    }
    pub fn set(&mut self, ctx: Ctx<'_>, name: String, value: String) -> rquickjs::Result<()> {
        self.headers.insert(
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| Exception::throw_message(&ctx, "Invalid Header Name"))?,
            value
                .try_into()
                .map_err(|_| Exception::throw_message(&ctx, "Invalid Header Value"))?,
        );
        Ok(())
    }
    pub fn values(&mut self) -> rquickjs::Result<Iterable<Vec<String>>> {
        let values = self
            .headers
            .values()
            .map(|v| String::from_utf8_lossy(v.as_bytes()).to_string())
            .collect::<Vec<String>>();
        Ok(Iterable::from(values))
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

    // Method
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

    Ok(FetchResponse {
        headers: FetchHeaders {
            headers: response.headers().clone(),
        },
        ok: response.status().is_success(),
        status: response.status().as_u16(),
        status_text: response.status().canonical_reason().map(|s| s.to_string()),
        url: String::from(response.url().clone()),
        response: Some(response),
    })
}

pub fn register_fetch(ctx: &Ctx) -> rquickjs::Result<()> {
    rquickjs::Class::<FetchResponse>::define(&ctx.globals())?;
    rquickjs::Class::<FetchHeaders>::define(&ctx.globals())?;
    ctx.globals().set("fetch", js_fetch)?;
    Ok(())
}
