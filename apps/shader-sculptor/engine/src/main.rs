use gremlin_core::*;
use serde_json::json;
use std::{
    io::{self, Read, Write},
    time::Instant,
};
#[cfg(test)]
const N: usize = 128 * 128;
fn width(pixels: usize) -> usize {
    (pixels as f64).sqrt() as usize
}
const CANDIDATES: usize = 4096;
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Shape {
    x: i32,
    y: i32,
    rx: i32,
    ry: i32,
    kind: i32,
    rotate: i32,
    color: u32,
    alpha: u32,
}
fn inside(s: Shape, x: i32, y: i32) -> bool {
    let (mut dx, mut dy) = (x - s.x, y - s.y);
    if s.rotate != 0 {
        let old = dx;
        dx += dy;
        dy = old - dy;
    }
    dx = dx.abs();
    dy = dy.abs();
    let (dx, dy, rx, ry) = (dx as i64, dy as i64, s.rx as i64, s.ry as i64);
    match s.kind {
        0 => dx * dx * ry * ry + dy * dy * rx * rx <= rx * rx * ry * ry,
        1 => dx <= rx && dy <= ry,
        _ => dx * ry + dy * rx <= rx * ry,
    }
}
fn blend(old: u32, s: Shape) -> u32 {
    let mut value = 0;
    for shift in [0, 8, 16] {
        let a = (old >> shift) & 255;
        let b = (s.color >> shift) & 255;
        value |= ((a * (255 - s.alpha) + b * s.alpha + 127) / 255) << shift;
    }
    value
}
fn error(a: u32, b: u32) -> i64 {
    [0, 8, 16]
        .iter()
        .map(|shift| {
            let d = ((a >> shift) & 255) as i64 - ((b >> shift) & 255) as i64;
            d * d
        })
        .sum()
}
fn loss(canvas: &[u32], target: &[u32]) -> i64 {
    canvas.iter().zip(target).map(|(a, b)| error(*a, *b)).sum()
}
fn paint(canvas: &mut [u32], s: Shape) {
    let w = width(canvas.len());
    for (p, v) in canvas.iter_mut().enumerate() {
        if inside(s, (p % w) as i32, (p / w) as i32) {
            *v = blend(*v, s);
        }
    }
}
#[cfg(feature = "cuda")]
mod cuda {
    use super::*;
    use std::ffi::{c_char, c_void, CStr};
    extern "C" {
        fn sculptor_create(
            target: *const u32,
            canvas: *const u32,
            width: u32,
            out: *mut *mut c_void,
            device: *mut c_char,
            error: *mut c_char,
        ) -> i32;
        fn sculptor_score(
            state: *mut c_void,
            shapes: *const Shape,
            count: u32,
            scores: *mut i64,
            kernel_ms: *mut f32,
            error: *mut c_char,
        ) -> i32;
        fn sculptor_paint(state: *mut c_void, shape: Shape, error: *mut c_char) -> i32;
        fn sculptor_read(state: *mut c_void, canvas: *mut u32, error: *mut c_char) -> i32;
        fn sculptor_destroy(state: *mut c_void);
    }
    pub struct Gpu {
        state: *mut c_void,
        pixels: usize,
        pub device: String,
        pub kernel_ms: f64,
    }
    impl Drop for Gpu {
        fn drop(&mut self) {
            unsafe { sculptor_destroy(self.state) }
        }
    }
    fn check(code: i32, error: &[c_char; 1024]) -> Result<(), String> {
        if code == 0 {
            Ok(())
        } else {
            Err(unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned())
        }
    }
    impl Gpu {
        pub fn new(target: &[u32], canvas: &[u32]) -> Result<Self, String> {
            let mut state = std::ptr::null_mut();
            let mut error = [0; 1024];
            let mut device = [0; 256];
            let code = unsafe {
                sculptor_create(
                    target.as_ptr(),
                    canvas.as_ptr(),
                    width(canvas.len()) as u32,
                    &mut state,
                    device.as_mut_ptr(),
                    error.as_mut_ptr(),
                )
            };
            let mut gpu = Self {
                state,
                pixels: canvas.len(),
                device: String::new(),
                kernel_ms: 0.,
            };
            check(code, &error)?;
            gpu.device = unsafe { CStr::from_ptr(device.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            Ok(gpu)
        }
        pub fn score(&mut self, shapes: &[Shape]) -> Result<Vec<i64>, String> {
            let mut error = [0; 1024];
            let mut scores = vec![0; shapes.len()];
            let mut ms = 0.;
            check(
                unsafe {
                    sculptor_score(
                        self.state,
                        shapes.as_ptr(),
                        shapes.len() as u32,
                        scores.as_mut_ptr(),
                        &mut ms,
                        error.as_mut_ptr(),
                    )
                },
                &error,
            )?;
            self.kernel_ms += ms as f64;
            Ok(scores)
        }
        pub fn paint(&mut self, s: Shape) -> Result<(), String> {
            let mut error = [0; 1024];
            check(
                unsafe { sculptor_paint(self.state, s, error.as_mut_ptr()) },
                &error,
            )
        }
        pub fn read(&mut self) -> Result<Vec<u32>, String> {
            let mut canvas = vec![0; self.pixels];
            let mut error = [0; 1024];
            check(
                unsafe { sculptor_read(self.state, canvas.as_mut_ptr(), error.as_mut_ptr()) },
                &error,
            )?;
            Ok(canvas)
        }
    }
}
#[cfg(not(feature = "cuda"))]
mod cuda {
    use super::*;
    pub struct Gpu {
        pub device: String,
        pub kernel_ms: f64,
    }
    impl Gpu {
        pub fn new(_: &[u32], _: &[u32]) -> Result<Self, String> {
            Err("GPU unavailable: build this app with --features cuda".into())
        }
        pub fn score(&mut self, _: &[Shape]) -> Result<Vec<i64>, String> {
            unreachable!()
        }
        pub fn paint(&mut self, _: Shape) -> Result<(), String> {
            unreachable!()
        }
        pub fn read(&mut self) -> Result<Vec<u32>, String> {
            unreachable!()
        }
    }
}
fn emit(v: serde_json::Value) {
    println!("{v}");
    io::stdout().flush().unwrap();
}
fn rgb64(pixels: &[u32]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(pixels.len() * 4);
    for p in pixels {
        for shift in [18, 12, 6, 0] {
            out.push(ABC[((p >> shift) & 63) as usize] as char);
        }
    }
    out
}
fn source(background: u32, layers: &[Shape]) -> String {
    let mut s=format!("fn pixel(x:i64,y:i64)->i32 {{\n let mut r:i32={}i32; let mut g:i32={}i32; let mut c:i32={}i32;\n",background>>16,(background>>8)&255,background&255);
    for (n, p) in layers.iter().enumerate() {
        let dx = format!("sub(x,{}i64)", p.x);
        let dy = format!("sub(y,{}i64)", p.y);
        let (u, v) = if p.rotate != 0 {
            (format!("add({dx},{dy})"), format!("sub({dx},{dy})"))
        } else {
            (dx, dy)
        };
        s += &format!(" let u{n}:i64={u}; let v{n}:i64={v};\n");
        // Ellipses square their offsets, so absolute-value temporaries are unnecessary.
        // This also keeps maximum-detail exports within the parser's source-size limit.
        if p.kind != 0 {
            s += &format!(" let a{n}:i64=select(slt(u{n},0i64),sub(0i64,u{n}),u{n}); let b{n}:i64=select(slt(v{n},0i64),sub(0i64,v{n}),v{n});\n");
        }
        let condition = match p.kind {
            0 => format!(
                "sle(add(mul(mul(u{n},u{n}),{}i64),mul(mul(v{n},v{n}),{}i64)),{}i64)",
                p.ry * p.ry,
                p.rx * p.rx,
                (p.rx as i64) * p.rx as i64 * p.ry as i64 * p.ry as i64
            ),
            1 => format!("select(sle(a{n},{}i64),sle(b{n},{}i64),false)", p.rx, p.ry),
            _ => format!(
                "sle(add(mul(a{n},{}i64),mul(b{n},{}i64)),{}i64)",
                p.ry,
                p.rx,
                p.rx * p.ry
            ),
        };
        s += &format!(" let hit{n}:bool={condition};\n");
        for (name, shift) in [("r", 16), ("g", 8), ("c", 0)] {
            s += &format!(
                " {name}=select(hit{n},sdiv(add(mul({name},{}i32),{}i32),255i32),{name});\n",
                255 - p.alpha,
                ((p.color >> shift) & 255) * p.alpha + 127
            );
        }
    }
    s + " return or(shl(r,16i32),or(shl(g,8i32),c));\n}\n"
}
fn proposal(rng: &mut Rng, canvas: &[u32], target: &[u32], round: usize) -> Shape {
    let n = target.len();
    let w = width(n);
    let mut p = rng.index(n);
    for _ in 0..3 {
        let q = rng.index(n);
        if error(canvas[q], target[q]) > error(canvas[p], target[p]) {
            p = q;
        }
    }
    let max = if rng.index(4) == 0 {
        8
    } else if round > 64 {
        24
    } else {
        64
    };
    let max = max * w / 128;
    let mut color = 0;
    for shift in [0, 8, 16] {
        let c = ((target[p] >> shift) & 255) as i32;
        let jitter = if rng.index(3) == 0 {
            rng.index(65) as i32 - 32
        } else {
            0
        };
        color |= ((c + jitter).clamp(0, 255) as u32) << shift;
    }
    Shape {
        x: (p % w) as i32,
        y: (p / w) as i32,
        rx: 1 + rng.index(max) as i32,
        ry: 1 + rng.index(max) as i32,
        kind: rng.index(3) as i32,
        rotate: rng.index(2) as i32,
        color,
        alpha: [64, 96, 128, 192, 255][rng.index(5)],
    }
}
// Pixels outside a shape's conservative bounding rectangle cannot change error.
fn footprint(s: Shape, w: usize) -> u64 {
    let (rx, ry) = if s.rotate != 0 {
        (s.rx + s.ry, s.rx + s.ry)
    } else {
        (s.rx, s.ry)
    };
    let last = w as i32 - 1;
    ((s.x + rx).min(last) - (s.x - rx).max(0) + 1) as u64
        * ((s.y + ry).min(last) - (s.y - ry).max(0) + 1) as u64
}
fn preview(canvas: &[u32]) -> String {
    let w = width(canvas.len());
    if w <= 512 {
        return rgb64(canvas);
    }
    let pixels: Vec<_> = (0..512 * 512)
        .map(|p| canvas[(p / 512 * w / 512) * w + (p % 512 * w / 512)])
        .collect();
    rgb64(&pixels)
}
fn run(target: Vec<u32>, seed: u64, budget: u64) -> Result<(), String> {
    let w = width(target.len());
    let n = target.len();
    let clock = Instant::now();
    let search_budget =
        (budget * (w * w / (128 * 128)) as u64).min(if budget > 12000 { 360_000 } else { 120_000 });
    let mut background = 0;
    for shift in [0, 8, 16] {
        let sum: u64 = target.iter().map(|p| ((p >> shift) & 255) as u64).sum();
        background |= ((sum / n as u64) as u32) << shift;
    }
    let mut canvas = vec![background; n];
    let initial = loss(&canvas, &target);
    let mut current = initial;
    let mut gpu = cuda::Gpu::new(&target, &canvas)?;
    let mut rng = Rng::new(seed);
    let mut layers = vec![];
    let mut scored = 0;
    let mut pixel_evaluations = 0u64;
    let mut first = None;
    let mut last_emit = Instant::now();
    emit(
        json!({"kind":"start","width":w,"height":w,"background":background,"initial_error":initial,"budget_ms":budget,"seed":seed,"device":gpu.device,"image":preview(&canvas)}),
    );
    let layer_limit = match budget {
        1500 => 128,
        3000 => 512,
        12000 => 2048,
        24000 => 4096,
        _ => 8192,
    };
    for round in 0..layer_limit {
        if clock.elapsed().as_millis() >= search_budget as u128 {
            break;
        }
        let shapes: Vec<_> = (0..CANDIDATES)
            .map(|_| proposal(&mut rng, &canvas, &target, round))
            .collect();
        pixel_evaluations += shapes.iter().map(|s| footprint(*s, w)).sum::<u64>();
        let scores = gpu.score(&shapes)?;
        scored += shapes.len();
        let (index, delta) = scores.iter().enumerate().min_by_key(|(_, v)| **v).unwrap();
        if *delta < 0 {
            let shape = shapes[index];
            paint(&mut canvas, shape);
            let exact = loss(&canvas, &target);
            if exact != current + delta {
                return Err("GPU fitness disagrees with independent renderer".into());
            }
            current = exact;
            gpu.paint(shape)?;
            layers.push(shape);
            if first.is_none() {
                first = Some(clock.elapsed().as_secs_f64());
            }
        }
        if last_emit.elapsed().as_millis() >= if budget > 12000 { 2000 } else { 500 } || round == 0
        {
            if gpu.read()? != canvas {
                return Err("GPU canvas disagrees with independent renderer".into());
            }
            emit(
                json!({"kind":"progress","pixel_evaluations":pixel_evaluations,"width":w,"height":w,"layers":layers.len(),"candidates":scored,"seconds":clock.elapsed().as_secs_f64(),"error":current,"initial_error":initial,"image":preview(&canvas)}),
            );
            last_emit = Instant::now();
        }
        if current == 0 {
            break;
        }
    }
    let seconds = clock.elapsed().as_secs_f64();
    if gpu.read()? != canvas {
        return Err("final GPU render mismatch".into());
    }
    let text = source(background, &layers);
    let f = parse(&text)?;
    let mut evaluator = Evaluator::new(&f)?;
    let mut check = Rng::new(0x56414c4944415445);
    let mut positions = vec![0, w - 1, w * (w - 1), n - 1, (w / 2) * w + w / 2];
    while positions.len() < 261 {
        positions.push(check.index(n));
    }
    for p in &positions {
        let result = evaluator.execute(
            &[
                Value::new(Type::I64, (p % w) as u64),
                Value::new(Type::I64, (p / w) as u64),
            ],
            1_000_000,
        );
        if result.outcome != Outcome::Completed(Value::new(Type::I32, canvas[*p] as u64)) {
            return Err(format!("Gremlin export mismatch at pixel {p}: {result:?}"));
        }
    }
    emit(
        json!({"kind":"done","width":w,"height":w,"search_budget_ms":search_budget,"mode":"gpu","seconds":seconds,"total_seconds":clock.elapsed().as_secs_f64(),"first_improvement_seconds":first,"layers":layers.len(),"candidates":scored,"pixel_evaluations":pixel_evaluations,"error":current,"initial_error":initial,"rmse":(current as f64/(n*3)as f64).sqrt(),"image":rgb64(&canvas),"background":background,"program":text,"shapes":layers.iter().map(|s|vec![s.x as i64,s.y as i64,s.rx as i64,s.ry as i64,s.kind as i64,s.rotate as i64,s.color as i64,s.alpha as i64]).collect::<Vec<_>>(),"device":gpu.device,"kernel_ms":gpu.kernel_ms,"device_bytes":n*8+CANDIDATES*40,"gremlin_pixels_checked":positions.len(),"native_pixels_checked":n,"candidate_score_download_bytes":scored*8,"target_hash":object_hash(&target),"seed":seed,"budget_ms":budget}),
    );
    Ok(())
}
fn main() {
    let result = (|| {
        let mut text = String::new();
        io::stdin()
            .take(50_000_000)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let object = v.as_object().ok_or("expected object")?;
        if object.len() != 3 {
            return Err("expected target, seed, budget_ms".into());
        }
        let seed = v["seed"].as_u64().ok_or("invalid seed")?;
        let budget = v["budget_ms"].as_u64().ok_or("invalid budget")?;
        if !(1..=3).contains(&seed) || ![1500, 3000, 12000, 24000, 48000].contains(&budget) {
            return Err("unsupported settings".into());
        }
        let target = v["target"]
            .as_array()
            .ok_or("invalid target")?
            .iter()
            .map(|x| {
                x.as_u64()
                    .filter(|v| *v <= 0xffffff)
                    .map(|v| v as u32)
                    .ok_or("invalid RGB pixel".into())
            })
            .collect::<Result<Vec<_>, String>>()?;
        if ![128, 256, 512, 1024, 2048]
            .iter()
            .any(|w| target.len() == w * w)
        {
            return Err(
                "target must be square: 128, 256, 512, 1024, or 2048 pixels per side".into(),
            );
        }
        run(target, seed, budget)
    })();
    if let Err(message) = result {
        emit(json!({"kind":"error","message":message}));
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exported_graphics_program_matches_native_renderer() {
        let mut rng = Rng::new(12);
        let target: Vec<_> = (0..N).map(|_| (rng.next_u64() & 0xffffff) as u32).collect();
        let mut canvas = vec![0x203040; N];
        let layers: Vec<_> = (0..12)
            .map(|n| proposal(&mut rng, &canvas, &target, n))
            .collect();
        for s in &layers {
            paint(&mut canvas, *s);
        }
        let f = parse(&source(0x203040, &layers)).unwrap();
        let mut e = Evaluator::new(&f).unwrap();
        for p in (0..N).step_by(37) {
            assert_eq!(
                e.execute(
                    &[
                        Value::new(Type::I64, (p % 128) as u64),
                        Value::new(Type::I64, (p / 128) as u64)
                    ],
                    100000
                )
                .outcome,
                Outcome::Completed(Value::new(Type::I32, canvas[p] as u64))
            );
        }
    }
    #[test]
    fn maximum_detail_export_fits_parser_limit() {
        for kind in 0..3 {
            let layers = vec![
                Shape {
                    x: 2047,
                    y: 2047,
                    rx: 1024,
                    ry: 1024,
                    kind,
                    rotate: 1,
                    color: 0xffffff,
                    alpha: 128
                };
                8192
            ];
            let text = source(0xffffff, &layers);
            assert!(text.len() <= 8_000_000, "{} bytes", text.len());
            parse(&text).unwrap();
        }
    }
    #[test]
    fn large_geometry_export_matches_native() {
        let layers = vec![
            Shape {
                x: 1000,
                y: 1100,
                rx: 1024,
                ry: 987,
                kind: 0,
                rotate: 1,
                color: 0xff3366,
                alpha: 128,
            },
            Shape {
                x: 2047,
                y: 0,
                rx: 600,
                ry: 900,
                kind: 2,
                rotate: 0,
                color: 0x00ccaa,
                alpha: 255,
            },
        ];
        let f = parse(&source(0x203040, &layers)).unwrap();
        let mut e = Evaluator::new(&f).unwrap();
        for y in (0..2048).step_by(79) {
            for x in (0..2048).step_by(83) {
                let mut expected = 0x203040;
                for s in &layers {
                    if inside(*s, x, y) {
                        expected = blend(expected, *s);
                    }
                }
                assert_eq!(
                    e.execute(
                        &[
                            Value::new(Type::I64, x as u64),
                            Value::new(Type::I64, y as u64)
                        ],
                        10000
                    )
                    .outcome,
                    Outcome::Completed(Value::new(Type::I32, expected as u64))
                );
            }
        }
    }
    #[cfg(feature = "cuda")]
    #[test]
    fn gpu_scores_and_paint_match_native() {
        for w in [128, 512, 2048] {
            let n = w * w;
            let mut rng = Rng::new(42);
            let target: Vec<_> = (0..n).map(|_| (rng.next_u64() & 0xffffff) as u32).collect();
            let mut canvas = vec![0x203040; n];
            let mut gpu = cuda::Gpu::new(&target, &canvas).unwrap();
            for round in 0..3 {
                let shapes: Vec<_> = (0..if w == 128 { 129 } else { 9 })
                    .map(|_| proposal(&mut rng, &canvas, &target, round))
                    .collect();
                let scores = gpu.score(&shapes).unwrap();
                let old = loss(&canvas, &target);
                for (s, delta) in shapes.iter().zip(scores) {
                    let mut test = canvas.clone();
                    paint(&mut test, *s);
                    assert_eq!(loss(&test, &target) - old, delta);
                }
                paint(&mut canvas, shapes[3]);
                gpu.paint(shapes[3]).unwrap();
                assert_eq!(gpu.read().unwrap(), canvas);
            }
        }
    }
}
