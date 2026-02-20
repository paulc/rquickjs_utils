use chrono::Utc;

use rquickjs::{class::Trace, Ctx, JsLifetime};

#[derive(Debug, Clone, Trace, JsLifetime)]
#[rquickjs::class]
pub struct Date {
    #[qjs(skip_trace)]
    dt: chrono::DateTime<Utc>,
}

#[rquickjs::methods]
impl Date {
    #[qjs(constructor)]
    pub fn new(_ctx: Ctx<'_>) -> Self {
        Self { dt: Utc::now() }
    }
    pub fn to_string(&self) -> String {
        self.dt.to_rfc2822()
    }
}

pub fn register_date(ctx: &Ctx<'_>) -> anyhow::Result<()> {
    rquickjs::Class::<Date>::define(&ctx.globals())?;
    Ok(())
}
