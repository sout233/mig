use super::http::Http;
use rquickjs::{Context, Ctx, Function, Runtime};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub(super) struct Source {
    pub sources: Value,
    context: Context,
    runtime: Runtime,
    http: Http,
    receiver: mpsc::Receiver<(u32, Value)>,
    jobs: Rc<RefCell<Vec<std::thread::JoinHandle<()>>>>,
    started: Instant,
    budget: Arc<AtomicU64>,
}

fn exception(ctx: &Ctx<'_>, error: rquickjs::Error) -> String {
    if error.is_exception() {
        let exception = ctx.catch();
        if let Some(exception) = exception.as_exception() {
            return exception.message().unwrap_or_else(|| "脚本异常".into());
        }
        if let Some(string) = exception.as_string() {
            return string.to_string().unwrap_or_default();
        }
    }
    error.to_string()
}

impl Source {
    pub fn load(http: &Http, raw: &str) -> Result<Self, String> {
        http.check()?;
        let runtime = Runtime::new().map_err(|e| e.to_string())?;
        runtime.set_memory_limit(96 * 1024 * 1024);
        runtime.set_max_stack_size(2 * 1024 * 1024);
        let started = Instant::now();
        let budget = Arc::new(AtomicU64::new(2000));
        let budget_check = budget.clone();

        let http = Http {
            cancel: http.cancel.clone(),
            stopped: Arc::default(),
            request_cancel: Arc::default(),
        };

        let interrupt = http.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            interrupt.check().is_err()
                || started.elapsed().as_millis() > budget_check.load(Ordering::Relaxed) as u128
        })));
        let context = Context::full(&runtime).map_err(|e| e.to_string())?;
        let (sender, receiver) = mpsc::channel();
        let cancellations = Rc::new(RefCell::new(HashMap::<u32, Arc<AtomicBool>>::new()));
        let jobs = Rc::new(RefCell::new(Vec::new()));

        let setup = context.with(|ctx| -> Result<(), String> {
            let globals = ctx.globals();
            globals.set("__rawScript", raw).map_err(|e| e.to_string())?;
            globals.set("__native", Function::new(ctx.clone(), |operation: String, input: String| {
                let result = serde_json::from_str(&input).map_err(|e| e.to_string())
                    .and_then(|input| super::crypto::call(&operation, input));

                match result { Ok(result) => json!({"ok":true,"result":result}), Err(error) => json!({"ok":false,"error":error}) }.to_string()
            }).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let request_http = http.clone();
            let request_jobs = jobs.clone();
            let request_cancellations = cancellations.clone();
            globals.set("__request", Function::new(ctx.clone(), move |id: u32, url: String, options: String| {
                if request_jobs.borrow().len() >= 128 {
                    let _ = sender.send((id, json!({"ok":false,"error":"脚本请求次数超过限制"})));
                    return;
                }
                let mut http = request_http.clone();
                http.request_cancel = Arc::default();
                request_cancellations.borrow_mut().insert(id, http.request_cancel.clone());
                let sender = sender.clone();
                request_jobs.borrow_mut().push(std::thread::spawn(move || {
                    let result = serde_json::from_str::<Value>(&options).map_err(|e| e.to_string())
                        .and_then(|mut options| {
                            options["maxBytes"] = json!(options["maxBytes"].as_u64().unwrap_or(4 * 1024 * 1024).min(4 * 1024 * 1024));
                            http.request(&url, &options)
                        });

                    let packet = match result {
                        Ok(response) => json!({"ok":true,"result":{"statusCode":response.status,"statusMessage":reqwest::StatusCode::from_u16(response.status).ok().and_then(|status| status.canonical_reason()).unwrap_or_default(),"headers":response.headers,
                            "body":response.body,"bytes":response.raw.len(),"raw":response.raw}}),
                        Err(error) => json!({"ok":false,"error":error}),
                    };

                    let _ = sender.send((id, packet));
                }));
            }).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            globals.set("__cancel", Function::new(ctx.clone(), move |id: u32| {
                if let Some(flag) = cancellations.borrow().get(&id) { flag.store(true, Ordering::Relaxed); }
            }).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            ctx.eval::<(), _>(include_str!("lx-runtime.js")).map_err(|e| exception(&ctx, e))?;
            ctx.eval::<(), _>(raw).map_err(|e| exception(&ctx, e))
        });
        // Keep the jobs even if setup fails, so cleanup can still cancel and join them
        let mut source = Self {
            sources: Value::Null,
            context,
            runtime,
            http,
            receiver,
            jobs,
            started,
            budget,
        };

        setup?;
        let info = source.wait(8000)?;
        if info["status"] == false
            || !info["sources"].is_object()
            || !source
                .context
                .with(|ctx| ctx.eval::<bool, _>("typeof __handler === 'function'"))
                .unwrap_or(false)
        {
            return Err(info["message"].as_str().unwrap_or("脚本初始化失败").into());
        }
        source.sources = info["sources"].clone();
        Ok(source)
    }

    pub fn invoke(&mut self, info: &Value) -> Result<Value, String> {
        self.budget.store(
            self.started.elapsed().as_millis() as u64 + 18000,
            Ordering::Relaxed,
        );
        self.context.with(|ctx| {
            ctx.eval::<(), _>(format!("__invoke({info});"))
                .map_err(|e| exception(&ctx, e))
        })?;
        self.wait(18000)
    }

    fn wait(&mut self, milliseconds: u64) -> Result<Value, String> {
        let deadline = self.started.elapsed().as_millis() as u64 + milliseconds;
        self.budget.store(deadline, Ordering::Relaxed);
        loop {
            self.http.check()?;
            if self.started.elapsed().as_millis() as u64 >= deadline {
                return Err("脚本执行超时".into());
            }
            // Stop after a few jobs so we still get a chance to check for cancellation
            for _ in 0..128 {
                match self.runtime.execute_pending_job() {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(error) => {
                        error.0.with(|ctx| {
                            let _ = ctx.catch();
                        });
                    }
                }
            }
            self.context.with(|ctx| {
                ctx.eval::<(), _>("__tick();")
                    .map_err(|e| exception(&ctx, e))
            })?;

            let state: String = self.context.with(|ctx| {
                ctx.eval("JSON.stringify(__settled)")
                    .map_err(|e| exception(&ctx, e))
            })?;

            if state != "null" {
                let packet: Value = serde_json::from_str(&state).map_err(|e| e.to_string())?;
                return if packet["ok"] == true {
                    Ok(packet["result"].clone())
                } else {
                    Err(packet["error"].as_str().unwrap_or("脚本执行失败").into())
                };
            }
            if let Ok((id, response)) = self.receiver.recv_timeout(Duration::from_millis(10)) {
                self.context.with(|ctx| {
                    ctx.eval::<(), _>(format!("__deliver({id},{response});"))
                        .map_err(|e| exception(&ctx, e))
                })?;
            }
        }
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.http.stopped.store(true, Ordering::Relaxed);
        for job in self.jobs.borrow_mut().drain(..) {
            let _ = job.join();
        }
    }
}
