use gremlin_core::{parse, parse_module};

#[test]
fn large_source_is_supported_but_remains_bounded() {
    let program = "fn pixel(x:i32)->i32 { return x; }";
    let source = format!("{}{}", " ".repeat(8_000_000 - program.len()), program);
    assert!(parse(&source).is_ok());
    assert!(parse_module(&source).is_ok());
    let oversized = source + " ";
    assert_eq!(parse(&oversized).unwrap_err(), "source exceeds 8 MB limit");
    assert_eq!(
        parse_module(&oversized).unwrap_err(),
        "source exceeds 8 MB limit"
    );
}
