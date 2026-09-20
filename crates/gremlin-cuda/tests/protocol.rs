use gremlin_core::{Execution, Outcome, Type, Value};
use gremlin_cuda::{
    protocol::{decode_response, encode_response},
    Evaluation, Telemetry,
};

fn sample() -> Evaluation {
    let mut row: Vec<_> = [
        Type::U8,
        Type::U16,
        Type::U32,
        Type::U64,
        Type::I8,
        Type::I16,
        Type::I32,
        Type::I64,
    ]
    .into_iter()
    .map(|ty| Execution {
        outcome: Outcome::Completed(Value::new(ty, u64::MAX)),
        steps: u64::MAX,
    })
    .collect();
    row.extend([
        Execution {
            outcome: Outcome::Timeout("step budget exhausted".into()),
            steps: 0,
        },
        Execution {
            outcome: Outcome::Trap("division by zero".into()),
            steps: 17,
        },
        Execution {
            outcome: Outcome::Trap("signed division overflow".into()),
            steps: 19,
        },
    ]);
    Evaluation {
        executions: vec![row.clone(), row],
        telemetry: Telemetry {
            setup_ms: 0.,
            transfer_ms: 1.25,
            kernel_ms: 0.125,
            total_ms: 3.5,
            host_total_ms: 4.75,
            driver: 13010,
            runtime: 13000,
            compute_capability: "8.6".into(),
            device: "test GPU λ".into(),
            compiler: "test compiler".into(),
            allocated_bytes: u64::MAX,
        },
    }
}
#[test]
fn binary_round_trip_preserves_all_types_outcomes_steps_and_telemetry() {
    let original = sample();
    let bytes = encode_response(&Ok(original.clone())).unwrap();
    assert_eq!(&bytes[..8], b"GRCUDA01");
    let decoded = decode_response(&bytes).unwrap();
    assert_eq!(original.executions, decoded.executions);
    assert_eq!(
        serde_json::to_value(original.telemetry).unwrap(),
        serde_json::to_value(decoded.telemetry).unwrap()
    );
    let error = encode_response(&Err("CUDA unavailable".into())).unwrap();
    assert_eq!(decode_response(&error).unwrap_err(), "CUDA unavailable");
}
#[test]
fn malformed_responses_fail_without_truncation_or_normalization() {
    let data = encode_response(&Ok(sample())).unwrap();
    for end in 0..data.len() {
        assert!(decode_response(&data[..end]).is_err(), "length {end}");
    }
    let mut trailing = data.clone();
    trailing.push(0);
    assert!(decode_response(&trailing).is_err());
    let records = data.len() - 22 * 18;
    for (offset, byte) in [
        (0, b'X'),
        (7, b'2'),
        (8, 9),
        (records, 9),
        (records + 1, 9),
        (records + 3, 1),
    ] {
        let mut corrupted = data.clone();
        corrupted[offset] = byte;
        assert!(decode_response(&corrupted).is_err(), "offset {offset}");
    }
    let mut oversized = data.clone();
    oversized[records - 8..records].fill(255);
    assert!(decode_response(&oversized).is_err());
    let mut bad_float = data.clone();
    bad_float[9..17].copy_from_slice(&f64::NAN.to_le_bytes());
    assert!(decode_response(&bad_float).is_err());
    let mut payload = data.clone();
    payload[records + 8 * 18 + 2] = 1;
    assert!(decode_response(&payload).is_err());
    let mut huge_string = data.clone();
    huge_string[65..69].fill(255);
    assert!(decode_response(&huge_string).is_err());
    let mut error = encode_response(&Err("failure".into())).unwrap();
    error.push(0);
    assert!(decode_response(&error).unwrap_err().contains("trailing"));
}
#[test]
fn encoder_rejects_unrepresentable_responses() {
    let mut r = sample();
    r.executions[1].pop();
    assert!(encode_response(&Ok(r)).is_err());
    let mut r = sample();
    r.executions.clear();
    assert!(encode_response(&Ok(r)).is_err());
    let mut r = sample();
    r.executions[0][0].outcome = Outcome::Timeout("other reason".into());
    assert!(encode_response(&Ok(r)).is_err());
    let mut r = sample();
    r.executions[0][0].outcome = Outcome::Completed(Value {
        ty: Type::U8,
        bits: 256,
    });
    assert!(encode_response(&Ok(r)).is_err());
    let mut r = sample();
    r.telemetry.setup_ms = -1.;
    assert!(encode_response(&Ok(r)).is_err());
    assert!(encode_response(&Err("x".repeat(65537))).is_err());
}

#[test]
fn execution_record_has_fixed_little_endian_layout() {
    let mut evaluation = sample();
    evaluation.executions = vec![vec![Execution {
        outcome: Outcome::Completed(Value::new(Type::U32, 0x12345678)),
        steps: 0x0102030405060708,
    }]];
    let bytes = encode_response(&Ok(evaluation)).unwrap();
    assert_eq!(
        &bytes[bytes.len() - 18..],
        &[0, 3, 0x78, 0x56, 0x34, 0x12, 0, 0, 0, 0, 8, 7, 6, 5, 4, 3, 2, 1]
    );
}

#[test]
fn compact_summary_frames_are_bounded_and_distinct_from_detailed_frames() {
    use gremlin_cuda::protocol::{
        decode_summary_response, encode_summary_response, SummaryEvaluation,
    };
    let summaries = vec![[0, 1, 32, 99, 0, 0, 0, 1, u64::MAX, 123]; 3];
    let frame = encode_summary_response(&Ok(SummaryEvaluation {
        summaries: summaries.clone(),
        telemetry: sample().telemetry,
    }))
    .unwrap();
    assert_eq!(
        decode_summary_response(&frame).unwrap().summaries,
        summaries
    );
    assert!(decode_response(&frame).is_err());
    assert!(decode_summary_response(&encode_response(&Ok(sample())).unwrap()).is_err());
    for end in 0..frame.len() {
        assert!(decode_summary_response(&frame[..end]).is_err());
    }
    let mut extra = frame.clone();
    extra.push(0);
    assert!(decode_summary_response(&extra).is_err());
    let mut huge = frame.clone();
    let count = huge.len() - 3 * 80 - 4;
    huge[count..count + 4].fill(255);
    assert!(decode_summary_response(&huge).is_err());
    let error = encode_summary_response(&Err("device lost".into())).unwrap();
    assert_eq!(decode_summary_response(&error).unwrap_err(), "device lost");
}
