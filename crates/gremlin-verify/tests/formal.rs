use gremlin_core::*;
use gremlin_verify::{
    lifter,
    proof::{self, Status},
};
use std::{
    fs,
    path::Path,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "gremlin-proof-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn target(temp: &Temp, body: &str) -> BinaryContract {
    let source = temp.0.join("target.s");
    let output = temp.0.join("target.so");
    fs::write(&source,format!(".text\n.global target\n.type target,@function\ntarget:\n{body}\n.size target,.-target\n.section .note.GNU-stack,\"\",@progbits\n")).unwrap();
    assert!(Command::new("cc")
        .args(["-shared", "-nostdlib", "-o"])
        .arg(&output)
        .arg(source)
        .status()
        .unwrap()
        .success());
    BinaryContract {
        path: output.to_string_lossy().into_owned(),
        sha256: hash(&fs::read(output).unwrap()),
        architecture: "x86_64".into(),
        format: "elf".into(),
        symbol: "target".into(),
        abi: "sysv64".into(),
        environment: "empty".into(),
        wall_timeout_ms: 1000,
        cpu_seconds: 1,
        memory_mb: 256,
    }
}
#[test]
fn binary_equivalence_counterexample_and_lifter_replay() {
    let temp = Temp::new();
    let contract = target(&temp, "mov %rdi,%rax\nimul $3,%rax,%rax\nadd $1,%rax\nret");
    let f = parse("fn f(x:u64)->u64{return add(mul(x,3u64),1u64);}").unwrap();
    let r = proof::verify_binary(&f, &contract, Path::new("/usr/bin/z3"), 5000, |input| {
        Ok(Value::new(
            Type::U64,
            input[0].bits.wrapping_mul(3).wrapping_add(1),
        ))
    })
    .unwrap();
    assert_eq!(r.status, Status::Equivalent);
    assert_eq!(r.evidence_level.as_deref(), Some("E4"));
    assert_eq!(r.evidence_scope, "binary");
    let wrong = parse("fn f(x:u64)->u64{return x;}").unwrap();
    let r = proof::verify_binary(&wrong, &contract, Path::new("/usr/bin/z3"), 5000, |input| {
        Ok(Value::new(
            Type::U64,
            input[0].bits.wrapping_mul(3).wrapping_add(1),
        ))
    })
    .unwrap();
    assert_eq!(r.status, Status::Counterexample);
    assert!(r.input.is_some());
    assert!(r.evidence_level.is_none());
    let model = lifter::lift(&contract, &f.signature()).unwrap();
    let mut rng = Rng::new(123);
    for _ in 0..10000 {
        let x = rng.next_u64();
        assert_eq!(
            model.execute(&[Value::new(Type::U64, x)]).unwrap().bits,
            x.wrapping_mul(3).wrapping_add(1)
        );
    }
    assert!(
        proof::verify_binary(&wrong, &contract, Path::new("/usr/bin/z3"), 5000, |_| Ok(
            Value::new(Type::U64, 99)
        ))
        .unwrap_err()
        .contains("modeling error")
    );
}
#[test]
fn unsupported_paths_registers_initializers_and_reference_scope() {
    let temp = Temp::new();
    let f = parse("fn f(x:u64)->u64{return x;}").unwrap();
    for body in [
        "mov (%rdi),%rax\nret",
        "call target\nret",
        "mov %rbx,%rax\nret",
        "mov %rdi,%rbx\nmov %rdi,%rax\nret",
        "mov %r10,%rax\nret",
        "jmp target\nret",
        "mov %rdi,%rax\n.byte 0x66,0xc3",
    ] {
        let contract = target(&temp, body);
        let r = proof::verify_binary(&f, &contract, Path::new("/usr/bin/z3"), 5000, |_| {
            panic!("unsupported must not replay")
        })
        .unwrap();
        assert_eq!(r.status, Status::Unsupported, "{body}");
        assert!(r.evidence_level.is_none());
    }
    let r = proof::verify_reference(&f, &f, Path::new("/usr/bin/z3"), 5000).unwrap();
    assert_eq!(r.status, Status::Equivalent);
    assert_eq!(r.evidence_scope, "reference_model");
    assert!(r.binary_model.is_none());
    let cfg = parse("fn f(x:u64)->u64{if eq(x,0u64){return x;}else{return 0u64;}}").unwrap();
    assert_eq!(
        proof::verify_reference(&cfg, &cfg, Path::new("/usr/bin/z3"), 5000)
            .unwrap()
            .status,
        Status::Unsupported
    );
}
#[test]
fn widths_wrapping_shifts_signed_division_and_eager_traps() {
    for ty in [
        Type::U8,
        Type::U16,
        Type::U32,
        Type::U64,
        Type::I8,
        Type::I16,
        Type::I32,
        Type::I64,
    ] {
        for (a, b) in [
            (format!("rotl(x,0{ty})"), "x".into()),
            (format!("shl(x,{}{ty})", ty.width()), "x".into()),
            (format!("add(x,1{ty})"), format!("sub(x,-1{ty})")),
        ] {
            if !ty.signed() && b.contains("-1") {
                continue;
            }
            let a = parse(&format!("fn f(x:{ty})->{ty}{{return {a};}}")).unwrap();
            let b = parse(&format!("fn f(x:{ty})->{ty}{{return {b};}}")).unwrap();
            assert_eq!(
                proof::verify_reference(&a, &b, Path::new("/usr/bin/z3"), 5000)
                    .unwrap()
                    .status,
                Status::Equivalent
            );
        }
        let op = if ty.signed() { "sdiv" } else { "udiv" };
        let a = parse(&format!(
            "fn f(x:{ty})->{ty}{{return select(eq(x,x),x,{op}(x,0{ty}));}}"
        ))
        .unwrap();
        let b = parse(&format!("fn f(x:{ty})->{ty}{{return x;}}")).unwrap();
        assert_eq!(
            proof::verify_reference(&a, &b, Path::new("/usr/bin/z3"), 5000)
                .unwrap()
                .status,
            Status::Counterexample
        );
    }
}
#[test]
fn solver_timeout_unknown_and_unavailable_are_not_equivalence() {
    use std::os::unix::fs::PermissionsExt;
    let temp = Temp::new();
    let f = parse("fn f(x:u8)->u8{return x;}").unwrap();
    let path = temp.0.join("solver");
    for (response, status) in [
        ("echo unknown", Status::Unknown),
        ("sleep 10", Status::Timeout),
    ] {
        let temporary = temp.0.join("solver.new");
        fs::write(&temporary,format!("#!/bin/sh\nif [ \"$1\" = '-version' ]; then echo test-solver; exit 0; fi\ncat >/dev/null\n{response}\n")).unwrap();
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o700)).unwrap();
        fs::rename(&temporary, &path).unwrap();
        let r = proof::verify_reference(&f, &f, &path, 100).unwrap();
        assert_eq!(r.status, status);
        assert!(r.evidence_level.is_none());
    }
    assert!(
        proof::verify_reference(&f, &f, &temp.0.join("missing"), 100)
            .unwrap_err()
            .contains("unavailable")
    );
}
#[test]
fn trap_reasons_and_order_remain_observable() {
    let zero = parse("fn f(x:i8)->i8{let a:i8=sdiv(x,0i8);return sdiv(-128i8,-1i8);}").unwrap();
    let overflow = parse("fn f(x:i8)->i8{let a:i8=sdiv(-128i8,-1i8);return sdiv(x,0i8);}").unwrap();
    assert_eq!(
        proof::verify_reference(&zero, &overflow, Path::new("/usr/bin/z3"), 5000)
            .unwrap()
            .status,
        Status::Counterexample
    );
}
#[test]
fn thirty_two_bit_writes_and_initializers() {
    let temp = Temp::new();
    let mut contract = target(&temp, "mov %edi,%eax\nadd $1,%eax\nret");
    let f = parse("fn f(x:u64)->u64{return and(add(x,1u64),4294967295u64);}").unwrap();
    assert_eq!(
        proof::verify_binary(&f, &contract, Path::new("/usr/bin/z3"), 5000, |_| panic!(
            "equivalent"
        ))
        .unwrap()
        .status,
        Status::Equivalent
    );
    assert!(Command::new("cc")
        .args(["-shared", "-o"])
        .arg(&contract.path)
        .arg(temp.0.join("target.s"))
        .status()
        .unwrap()
        .success());
    contract.sha256 = hash(&fs::read(&contract.path).unwrap());
    let r = proof::verify_binary(&f, &contract, Path::new("/usr/bin/z3"), 5000, |_| {
        panic!("unsupported")
    })
    .unwrap();
    assert_eq!(r.status, Status::Unsupported);
}
