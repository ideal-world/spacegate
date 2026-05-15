use proxy_wasm::hostcalls;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Info);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> { Box::new(HelloWorldRoot) });
}}

struct HelloWorldRoot;

impl Context for HelloWorldRoot {}

impl RootContext for HelloWorldRoot {
    fn on_vm_start(&mut self, _: usize) -> bool {
        let _ = hostcalls::log(LogLevel::Info, "hello world wasm plugin started");
        true
    }

    fn on_configure(&mut self, _: usize) -> bool {
        let _ = hostcalls::log(LogLevel::Info, "hello world wasm plugin configured");
        true
    }

    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(HelloWorldHttp))
    }

    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }
}

struct HelloWorldHttp;

impl Context for HelloWorldHttp {}

impl HttpContext for HelloWorldHttp {
    fn on_http_request_headers(&mut self, _: usize, _: bool) -> Action {
        let _ = hostcalls::log(LogLevel::Info, "hello world request reached wasm plugin");
        self.add_http_request_header("x-spacegate-wasm-plugin", "hello-world");

        if self.get_http_request_header(":path").as_deref() == Some("/hello-world") {
            self.send_http_response(200, vec![("content-type", "text/plain"), ("x-powered-by", "spacegate-wasm")], Some(b"hello world\n"));
            return Action::Pause;
        }

        Action::Continue
    }
}
