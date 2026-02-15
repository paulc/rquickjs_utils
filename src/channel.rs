use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

use rquickjs::{
    function::{Async, Func},
    Ctx, Exception, Function,
};

/// Register TX channel
pub fn register_oneshot<'js, T>(
    ctx: Ctx<'js>,
    tx: oneshot::Sender<T>,
    f: &str,
) -> anyhow::Result<()>
where
    T: rquickjs::IntoJs<'js> + rquickjs::FromJs<'js> + Clone + Send + 'static,
{
    let tx = Arc::new(Mutex::new(Some(tx)));
    ctx.globals().set(
        f,
        Func::new(move |ctx, msg: T| match tx.lock() {
            Ok(mut guard) => match guard.take() {
                Some(tx) => match tx.send(msg) {
                    Ok(_) => Ok::<(), rquickjs::Error>(()),
                    Err(_) => Err::<(), rquickjs::Error>(Exception::throw_message(
                        &ctx,
                        "TX Channel Closed",
                    )),
                },
                None => {
                    Err::<(), rquickjs::Error>(Exception::throw_message(&ctx, "Already Resolved"))
                }
            },
            Err(_) => Err::<(), rquickjs::Error>(Exception::throw_message(&ctx, "Mutex Error")),
        }),
    )?;
    Ok(())
}

/// Register TX channel
pub fn register_tx_channel<'js, T>(
    ctx: Ctx<'js>,
    tx: UnboundedSender<T>,
    f: &str,
) -> anyhow::Result<()>
where
    T: rquickjs::IntoJs<'js> + rquickjs::FromJs<'js> + Clone + Send + 'static,
{
    let tx = Arc::new(Mutex::new(tx));
    ctx.globals().set(
        f,
        Func::new(Async(move |ctx, msg: T| {
            let tx = tx.clone(); // Need to clone tx to ensure closure is Fn vs FnOnce
            async move {
                match tx
                    .lock()
                    .map_err(|_| Exception::throw_message(&ctx, "Mutex Error"))?
                    .send(msg)
                {
                    Ok(_) => Ok::<(), rquickjs::Error>(()),
                    Err(_) => Err::<(), rquickjs::Error>(Exception::throw_message(
                        &ctx,
                        "TX Channel Closed",
                    )),
                }
            }
        })),
    )?;
    Ok(())
}

/// Register RX channel
pub fn register_rx_channel<'js, T>(
    ctx: Ctx<'js>,
    rx: UnboundedReceiver<T>,
    f: &str,
) -> anyhow::Result<()>
where
    T: rquickjs::IntoJs<'js> + rquickjs::FromJs<'js> + Clone + Send + 'static,
{
    let rx = Arc::new(Mutex::new(rx));
    ctx.globals().set(
        f,
        Func::new(Async(move |ctx| {
            // Pass closure to JS engine
            let rx = rx.clone();
            async move {
                // Returns future when called
                if let Some(msg) = {
                    rx.lock()
                        .map_err(|_e| Exception::throw_message(&ctx, "Mutex Error"))?
                        .recv()
                        .await
                } {
                    Ok::<T, rquickjs::Error>(msg)
                } else {
                    Err::<T, rquickjs::Error>(Exception::throw_message(&ctx, "RX Channel Closed"))
                }
            }
        })),
    )?;
    Ok(())
}

/// Register RX channel callback
pub fn register_rx_channel_cb<'js, T>(
    ctx: Ctx<'js>,
    rx: UnboundedReceiver<T>,
    f: &str,
) -> anyhow::Result<()>
where
    T: rquickjs::IntoJs<'js> + rquickjs::FromJs<'js> + Clone + Send + 'static,
{
    let rx = Arc::new(Mutex::new(Some(rx)));
    let ctx = ctx.clone();
    ctx.globals().set(
        f,
        Func::new(Async(move |ctx, f: Function<'js>| {
            let rx = rx.clone(); // Need to clone rx to ensure closure is Fn vs FnOnce
            async move {
                let mut rx_guard = rx
                    .try_lock() // CB holds mutex when registered
                    .map_err(|_| Exception::throw_message(&ctx, "Mutex Locked (CB Exists)"))?;
                let mut rx = rx_guard
                    .take()
                    .ok_or_else(|| Exception::throw_message(&ctx, "CB Already Registered"))?;
                while let Some(msg) = rx.recv().await {
                    f.call::<_, ()>((msg,))?;
                }
                Ok::<(), rquickjs::Error>(())
            }
        })),
    )?;
    Ok(())
}

/// Register RX channel callback with cancel function
pub fn register_rx_channel_cb_cancel<'js, T>(
    ctx: Ctx<'js>,
    rx: UnboundedReceiver<T>,
    f: &str,
) -> anyhow::Result<()>
where
    T: rquickjs::IntoJs<'js> + rquickjs::FromJs<'js> + Clone + Send + 'static,
{
    // Wrap RX channel in Arc<Mutex<Option<>>> to pass into JS fn (Fn vs FnOnce)
    let rx = Arc::new(Mutex::new(Some(rx)));

    ctx.globals().set(
        f,
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, f: Function<'js>| -> rquickjs::Result<Function<'js>> {
                // Create oneshot channel
                let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
                let cancel_tx = Arc::new(Mutex::new(Some(cancel_tx)));

                // Create cancel function
                let cancel_f = Function::new(ctx.clone(), move |ctx| -> rquickjs::Result<()> {
                    cancel_tx
                        .lock()
                        .map_err(|_| Exception::throw_message(&ctx, "Mutex Locked"))?
                        .take()
                        .ok_or_else(|| Exception::throw_message(&ctx, "Already Cancelled"))?
                        .send(())
                        .map_err(|_| Exception::throw_message(&ctx, "Oneshot Error"))
                })?;

                // Spawn background task
                ctx.spawn({
                    let rx = rx.clone();
                    let mut cancel_rx = cancel_rx;
                    async move {
                        match rx.try_lock() {
                            Ok(mut rx_guard) => {
                                match rx_guard.take() {
                                    Some(mut rx) => loop {
                                        tokio::select! {
                                            Ok(()) = &mut cancel_rx => {
                                                // Replace the RX channel in the mutex
                                                rx_guard.replace(rx);
                                                break;
                                            }
                                            msg = rx.recv() => {
                                                match msg {
                                                    Some(msg) => f.call::<_, ()>((msg,)).unwrap(),
                                                    None => break
                                                }
                                            }
                                        }
                                    },
                                    None => eprintln!("Error: CB Already Registered"),
                                }
                            }
                            Err(_) => eprintln!("Error: CB Mutex Locked"),
                        }
                    }
                });
                Ok(cancel_f)
            },
        ),
    )?;
    Ok(())
}
