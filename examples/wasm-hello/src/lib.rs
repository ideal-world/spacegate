use proxy_wasm::hostcalls;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;

const HELLO: &str = "hello world from spacegate wasm plugin";

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Info);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> { Box::new(HelloRoot) });
}}

struct HelloRoot;

impl Context for HelloRoot {}

impl RootContext for HelloRoot {
    fn on_vm_start(&mut self, _: usize) -> bool {
        let _ = hostcalls::log(LogLevel::Info, HELLO);
        true
    }

    fn on_configure(&mut self, _: usize) -> bool {
        let _ = hostcalls::log(LogLevel::Info, "hello world wasm plugin configured");
        true
    }

    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(HelloHttp))
    }

    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }
}

struct HelloHttp;

impl Context for HelloHttp {}

impl HttpContext for HelloHttp {
    fn on_http_request_headers(&mut self, _: usize, _: bool) -> Action {
        let _ = hostcalls::log(LogLevel::Info, "hello world request reached wasm plugin");
        self.add_http_request_header("x-wasm-hello", "hello-world");

        if self.get_http_request_header(":path").as_deref() == Some("/hello-world") {
            self.send_http_response(200, vec![("content-type", "text/plain"), ("x-powered-by", "spacegate-wasm")], Some(b"hello world\n"));
            return Action::Pause;
        }

        Action::Continue
    }
}
