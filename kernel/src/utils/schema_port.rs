macro_rules! schema_port {
    ($($schema: literal => $port: literal)*) => {
        #[allow(unreachable_patterns)]
        pub fn schema_to_port(schema: &str) -> Option<u16> {
            match schema {
                $(
                    $schema => Some($port),
                    _ => None,
                )*
            }
        }
        #[allow(unreachable_patterns)]
        pub fn port_to_schema(port: u16) -> Option<&'static str> {
            match port {
                $(
                    $port => Some($schema),
                    _ => None,
                )*
            }
        }
    };
}

schema_port! {
    "http" => 80
    "ws" => 80
    "https" => 443
    "wss" => 443
    "grpc" => 50051
}
