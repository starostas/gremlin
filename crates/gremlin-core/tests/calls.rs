use gremlin_core::*;
#[test]
fn calls_recursion_and_roundtrips() {
    let source="fn main(x:u8)->u8{return factorial(x);} fn factorial(n:u8)->u8{if eq(n,0u8){return 1u8;}else{return mul(n,factorial(sub(n,1u8)));}}";
    let m = parse_module(source).unwrap();
    let printed = print_module(&m).unwrap();
    assert_eq!(m, parse_module(&printed).unwrap(), "{printed}");
    let args = [Value::new(Type::U8, 5)];
    assert_eq!(
        execute_module(&m, &args, 1000, 10).outcome,
        Outcome::Completed(Value::new(Type::U8, 120))
    );
    assert!(matches!(
        execute_module(&m, &args, 1000, 3).outcome,
        Outcome::Timeout(_)
    ));
    assert!(matches!(
        execute(&m.functions["main"], &args, 100).outcome,
        Outcome::Invalid(_)
    ));
}
#[test]
fn exact_call_depth_and_global_steps() {
    let m =
        parse_module("fn main(x:u8)->u8{return helper(x);}fn helper(x:u8)->u8{return x;}").unwrap();
    let args = [Value::new(Type::U8, 7)];
    let completed = execute_module(&m, &args, 3, 2);
    assert_eq!(
        completed,
        Execution {
            outcome: Outcome::Completed(args[0]),
            steps: 3
        }
    );
    assert_eq!(
        execute_module(&m, &args, 100, 1),
        Execution {
            outcome: Outcome::Timeout("call depth budget exhausted".into()),
            steps: 1
        }
    );
    assert_eq!(execute_module(&m, &args, 100, 0).steps, 0);
    assert_eq!(execute_module(&m, &args, 2, 2).steps, 2);
    let m = parse_module("fn main(x:u8)->u8{return main(x);}").unwrap();
    assert_eq!(
        execute_module(&m, &args, 1000, 20),
        Execution {
            outcome: Outcome::Timeout("call depth budget exhausted".into()),
            steps: 20
        }
    );
}
#[test]
fn invalid_calls_rejected_before_execution() {
    for source in [
        "fn main(x:u8)->u8{return missing(x);}",
        "fn main(x:u8)->u8{return f(x);}fn f(x:u16)->u16{return x;}",
        "fn main(x:u8)->u8{return x;}fn main(x:u8)->u8{return x;}",
    ] {
        assert!(parse_module(source).is_err(), "{source}");
    }
    let mut m = parse_module("fn main(x:u8)->u8{return f(x);}fn f(x:u8)->u8{return x;}").unwrap();
    m.functions.remove("f");
    assert!(matches!(
        execute_module(&m, &[Value::new(Type::U8, 0)], 100, 64).outcome,
        Outcome::Invalid(_)
    ));
}
