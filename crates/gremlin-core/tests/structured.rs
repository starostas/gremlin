use gremlin_core::*;
fn run(source: &str, args: &[u64], budget: u64) -> Execution {
    let f = parse(source).unwrap();
    let values: Vec<_> = args
        .iter()
        .zip(&f.parameters)
        .map(|(n, p)| Value::new(p.ty, *n))
        .collect();
    execute(&f, &values, budget)
}
#[test]
fn branches_and_mutable_merges() {
    let source="fn f(x:i8)->i8 { let mut y:i8=x; if slt(x,0i8) { y=sub(0i8,x); } else { y=add(x,1i8); } return y; }";
    assert_eq!(
        run(source, &[(-3i64) as u64], 100).outcome,
        Outcome::Completed(Value::new(Type::I8, 3))
    );
    assert_eq!(
        run(source, &[3], 100).outcome,
        Outcome::Completed(Value::new(Type::I8, 4))
    );
    assert_eq!(
        run(
            "fn f(x:u8)->u8 {if eq(x,0u8){return 1u8;}else{return 2u8;}}",
            &[0],
            100
        )
        .outcome,
        Outcome::Completed(Value::new(Type::U8, 1))
    );
}
#[test]
fn nested_loops_break_continue_and_carried_values() {
    let source="fn f(x:u8)->u8 {let mut n:u8=x;let mut sum:u8=0u8;while ne(n,0u8){n=sub(n,1u8);if eq(n,2u8){continue;}let mut j:u8=0u8;loop{j=add(j,1u8);if eq(j,2u8){break;}sum=add(sum,n);}}return sum;}";
    assert_eq!(
        run(source, &[5], 1000).outcome,
        Outcome::Completed(Value::new(Type::U8, 8))
    );
    let source="fn f(x:u8)->u8 { let mut a:u8=x; let mut b:u8=9u8; loop { let saved:u8=a; a=b; b=saved; break; } return add(a,b); }";
    assert_eq!(
        run(source, &[3], 100).outcome,
        Outcome::Completed(Value::new(Type::U8, 12))
    );
}
#[test]
fn nontermination_and_invalid_control_flow() {
    let e = run("fn f(x:u8)->u8 {loop {continue;}}", &[0], 47);
    assert_eq!(e.steps, 47);
    assert!(matches!(e.outcome, Outcome::Timeout(_)));
    for source in [
        "fn f()->u8 {break;}",
        "fn f()->u8 {continue;}",
        "fn f(x:u8)->u8 {if x{return x;}return 0u8;}",
        "fn f(x:u8)->u8 {let mut y:u8=0u8;y=1u16;return y;}",
        "fn f(x:u8)->u8 {if true{let y:u8=x;}return y;}",
        "fn f()->u8 {loop{break;return 1u8;}return 0u8;}",
        "fn f()->u8 {let mut x:u8=0u8;}",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
}
#[test]
fn canonical_cfg_source_preserves_ir_and_step_counts() {
    for source in [
        "fn f(x:u8)->u8 {let mut n:u8=x;while ne(n,0u8){n=sub(n,1u8);}return n;}",
        "fn f(x:i8)->i8 {if slt(x,0i8){return sub(0i8,x);}else{return x;}}",
        "fn f(x:u8)->u8 {loop {continue;}}",
    ] {
        let f = parse(source).unwrap();
        let printed = print_source(&f).unwrap();
        let g = parse(&printed).unwrap();
        assert_eq!(f, g, "{printed}");
        for budget in [0, 1, 10, 100] {
            let args = [Value::new(f.parameters[0].ty, 3)];
            assert_eq!(execute(&f, &args, budget), execute(&g, &args, budget));
        }
    }
}
