use spacegate_model::Config;

#[test]
fn test_parse_config() {
    let file = include_str!("test_parse_config/config.toml");
    let parse_result = toml::from_str::<Config>(file);
    match parse_result {
        Ok(result) => {
            println!("{:#?}", result);
        }
        Err(e) => {
            eprintln!("{}", e);
            if let Some(span) = e.span() {
                let bytes = file.as_bytes();
                let span_str = std::str::from_utf8(&bytes[span]).unwrap();
                eprintln!("{}", span_str);
            }
            panic!();
        }
    }
}
