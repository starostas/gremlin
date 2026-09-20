use gremlin_core::*;
use serde_json::json;
use std::{
    collections::{BTreeSet, VecDeque},
    io::{self, Write},
    time::Instant,
};

const MASK: u64 = (1u64 << 48) - 1;
const LIMIT: usize = 256;
#[derive(Clone, Debug)]
struct Room {
    walls: u64,
    start: usize,
    heading: usize,
    exit: usize,
    key: usize,
    door: usize,
}
fn neighbor(p: usize, d: usize) -> usize {
    match d % 4 {
        0 => p.wrapping_sub(8) & 63,
        1 => (p + 1) & 63,
        2 => (p + 8) & 63,
        _ => p.wrapping_sub(1) & 63,
    }
}
fn adjacent(p: usize) -> Vec<usize> {
    (0..4)
        .map(|d| neighbor(p, d))
        .filter(|p| p / 8 > 0 && p / 8 < 7 && p % 8 > 0 && p % 8 < 7)
        .collect()
}
fn distances(open: u64, start: usize) -> [usize; 64] {
    let mut dist = [999; 64];
    dist[start] = 0;
    let mut q = VecDeque::from([start]);
    while let Some(p) = q.pop_front() {
        for n in adjacent(p) {
            if open >> n & 1 != 0 && dist[n] == 999 {
                dist[n] = dist[p] + 1;
                q.push_back(n);
            }
        }
    }
    dist
}
fn rooms(count: usize, seed: u64) -> Vec<Room> {
    let mut rng = Rng::new(seed);
    let mut result = vec![];
    let mut seen = BTreeSet::new();
    while result.len() < count {
        let root = 9 + rng.index(6) + 8 * rng.index(6);
        let mut open = 1u64 << root;
        loop {
            let candidates: Vec<_> = (9..55)
                .filter(|p| {
                    p % 8 > 0
                        && p % 8 < 7
                        && open >> p & 1 == 0
                        && adjacent(*p).iter().filter(|q| open >> **q & 1 != 0).count() == 1
                })
                .collect();
            if candidates.is_empty() {
                break;
            }
            open |= 1 << candidates[rng.index(candidates.len())];
        }
        let cells: Vec<_> = (0..64).filter(|p| open >> p & 1 != 0).collect();
        if cells.len() < 15 {
            continue;
        }
        let start = cells[rng.index(cells.len())];
        let ds = distances(open, start);
        let exit = *cells.iter().max_by_key(|p| ds[**p]).unwrap();
        let mut path = vec![exit];
        let mut at = exit;
        while at != start {
            at = adjacent(at)
                .into_iter()
                .find(|p| ds[*p] + 1 == ds[at])
                .unwrap();
            path.push(at);
        }
        if path.len() < 5 {
            continue;
        }
        let door = path[path.len() / 2];
        let reach = distances(open & !(1 << door), start);
        let keys: Vec<_> = cells
            .iter()
            .copied()
            .filter(|p| reach[*p] != 999 && *p != start && *p != exit && *p != door)
            .collect();
        if keys.is_empty() {
            continue;
        }
        let key = keys[rng.index(keys.len())];
        let room = Room {
            walls: !open,
            start,
            heading: rng.index(4),
            exit,
            key,
            door,
        };
        if seen.insert(identity(&room)) {
            result.push(room);
        }
    }
    result
}
fn simulate(brain: u64, r: &Room, trace: bool) -> (u64, Vec<serde_json::Value>) {
    let mut p = r.start;
    let mut d = r.heading;
    let mut memory = 0;
    let mut key = false;
    let mut seen = 0u64;
    let mut steps = 0;
    let mut frames = vec![];
    loop {
        seen |= 1 << p;
        if p == r.key {
            key = true;
        }
        let solved = p == r.exit && key;
        if trace {
            frames.push(json!([p, d, memory, key]));
        }
        if solved || steps == LIMIT {
            break;
        }
        let free = |n: usize| r.walls >> n & 1 == 0 && (key || n != r.door);
        let front = free(neighbor(p, d));
        let left = free(neighbor(p, (d + 3) % 4));
        let right = free(neighbor(p, (d + 1) % 4));
        let row = front as usize + 2 * left as usize + 4 * right as usize + 8 * memory;
        let instruction = (brain >> (row * 3)) & 7;
        memory = (instruction >> 2) as usize;
        match instruction & 3 {
            0 => {
                if front {
                    p = neighbor(p, d)
                }
            }
            1 => d = (d + 3) % 4,
            2 => d = (d + 1) % 4,
            _ => d = (d + 2) % 4,
        };
        steps += 1;
    }
    let solved = p == r.exit && key;
    let packed = ((solved as u64) << 32)
        | ((key as u64) << 31)
        | ((seen.count_ones() as u64) << 16)
        | steps as u64;
    (packed, frames)
}

fn source(brain: u64) -> String {
    format!(
        r#"fn episode(walls:u64, objects:u64, pose:u64)->u64 {{
    let key_cell:u64=and(lshr(objects,6u64),63u64);
    let door:u64=and(lshr(objects,12u64),63u64);
    let goal:u64=and(objects,63u64);
    let brain:u64={brain}u64;
    let mut position:u64=and(pose,63u64);
    let mut heading:u64=and(lshr(pose,6u64),3u64);
    let mut memory:u64=0u64;
    let mut has_key:u64=0u64;
    let mut seen:u64=0u64;
    let mut visited:u64=0u64;
    let mut steps:u64=0u64;
    loop {{
        let here:u64=shl(1u64,position);
        if eq(and(seen,here),0u64) {{ visited=add(visited,1u64); }}
        seen=or(seen,here);
        if eq(position,key_cell) {{ has_key=1u64; }}
        if select(eq(position,goal),ne(has_key,0u64),false) {{ break; }}
        if uge(steps,256u64) {{ break; }}
        let ld:u64=and(add(heading,3u64),3u64);
        let rd:u64=and(add(heading,1u64),3u64);
        let f:u64=and(add(position,select(eq(heading,0u64),18446744073709551608u64,select(eq(heading,1u64),1u64,select(eq(heading,2u64),8u64,18446744073709551615u64)))),63u64);
        let l:u64=and(add(position,select(eq(ld,0u64),18446744073709551608u64,select(eq(ld,1u64),1u64,select(eq(ld,2u64),8u64,18446744073709551615u64)))),63u64);
        let r:u64=and(add(position,select(eq(rd,0u64),18446744073709551608u64,select(eq(rd,1u64),1u64,select(eq(rd,2u64),8u64,18446744073709551615u64)))),63u64);
        let front:bool=select(eq(and(walls,shl(1u64,f)),0u64),select(ne(f,door),true,ne(has_key,0u64)),false);
        let left:bool=select(eq(and(walls,shl(1u64,l)),0u64),select(ne(l,door),true,ne(has_key,0u64)),false);
        let right:bool=select(eq(and(walls,shl(1u64,r)),0u64),select(ne(r,door),true,ne(has_key,0u64)),false);
        let row:u64=add(add(select(front,1u64,0u64),select(left,2u64,0u64)),add(select(right,4u64,0u64),mul(memory,8u64)));
        let decision:u64=and(lshr(brain,mul(row,3u64)),7u64);
        memory=lshr(decision,2u64);
        let action:u64=and(decision,3u64);
        if eq(action,0u64) {{
            if front {{ position=f; }}
        }} else {{ heading=and(add(heading,select(eq(action,1u64),3u64,select(eq(action,2u64),1u64,2u64))),3u64); }}
        steps=add(steps,1u64);
    }}
    let solved:bool=select(eq(position,goal),ne(has_key,0u64),false);
    return or(select(solved,4294967296u64,0u64),or(shl(has_key,31u64),or(shl(visited,16u64),steps)));
}}"#
    )
}
fn input(r: &Room) -> Vec<Value> {
    [
        r.walls,
        (r.exit | (r.key << 6) | (r.door << 12)) as u64,
        (r.start | (r.heading << 6)) as u64,
    ]
    .into_iter()
    .map(|v| Value::new(Type::U64, v))
    .collect()
}
fn add_score(s: &mut (u64, u64, u64, u64), v: u64) {
    s.0 += v >> 32;
    s.1 += (v >> 31) & 1;
    s.2 += (v >> 16) & 127;
    if v >> 32 != 0 {
        s.3 += 256 - (v & 65535);
    }
}

fn identity(r: &Room) -> (u64, usize, usize, usize, usize, usize) {
    (r.walls, r.start, r.heading, r.exit, r.key, r.door)
}
fn room_json(r: &Room) -> serde_json::Value {
    json!({"walls":format!("{:016x}",r.walls),"start":r.start,"heading":r.heading,"exit":r.exit,"key":r.key,"door":r.door})
}
fn decisions(brain: u64) -> Vec<u64> {
    (0..16).map(|n| (brain >> (n * 3)) & 7).collect()
}
fn brain_source(brain: u64) -> String {
    fn tree(brain: u64, depth: usize, row: usize) -> String {
        if depth == 4 {
            return format!("return {}u64;", (brain >> (row * 3)) & 7);
        }
        let (name, bit) = [("memory", 8), ("front", 1), ("left", 2), ("right", 4)][depth];
        format!(
            "if ne({name},0u64) {{ {} }} else {{ {} }}",
            tree(brain, depth + 1, row | bit),
            tree(brain, depth + 1, row)
        )
    }
    format!(
        "fn brain(front:u64,left:u64,right:u64,memory:u64)->u64 {{ {} }}",
        tree(brain, 0, 0)
    )
}
fn emit(value: serde_json::Value) {
    println!("{value}");
    io::stdout().flush().unwrap();
}
fn previews(brain: u64, maps: &[Room]) -> Vec<serde_json::Value> {
    maps.iter()
        .take(12)
        .map(|r| {
            let (result, frames) = simulate(brain, r, true);
            json!({"room":room_json(r),"solved":result>>32!=0,"frames":frames})
        })
        .collect()
}
fn validate_brain(brain: u64) -> Result<(), String> {
    let f = parse(&brain_source(brain))?;
    let mut e = Evaluator::new(&f)?;
    for row in 0..16 {
        let input = [row & 1, (row >> 1) & 1, (row >> 2) & 1, (row >> 3) & 1]
            .map(|v| Value::new(Type::U64, v));
        if e.execute(&input, 256).outcome
            != Outcome::Completed(Value::new(Type::U64, (brain >> (row * 3)) & 7))
        {
            return Err("controller source does not match genome".into());
        }
    }
    Ok(())
}
fn search(seed: u64, count: usize) -> Result<(), String> {
    let maps = rooms(count, 0x524f424f54 ^ seed);
    let seen: BTreeSet<_> = maps.iter().map(identity).collect();
    let holdout: Vec<_> = rooms(1024, 0x484f4c44 ^ seed)
        .into_iter()
        .filter(|r| !seen.contains(&identity(r)))
        .take(512)
        .collect();
    if holdout.len() != 512 {
        return Err("insufficient disjoint holdout rooms".into());
    }
    let inputs: Vec<_> = maps.iter().map(input).collect();
    let mut active_cases = count.min(128);
    let mut rng = Rng::new(seed);
    let mut population: Vec<_> = (0..256).map(|_| rng.next_u64() & MASK).collect();
    emit(
        json!({"kind":"start","mode":"gpu","cases":active_cases,"requested_cases":count,"seed":seed,"population":256,"generation_limit":80,"step_limit":256,"rooms":maps.iter().take(12).map(room_json).collect::<Vec<_>>()}),
    );
    let clock = Instant::now();
    let mut evaluations = 0;
    let mut steps = 0u64;
    let mut peak = 0;
    let mut best = 0;
    let mut final_score = (0, 0, 0, 0);
    let mut generation = 0;
    let mut device = String::new();
    for gen in 0..=80 {
        let functions = population
            .iter()
            .map(|b| parse(&source(*b)))
            .collect::<Result<Vec<_>, _>>()?;
        let result = gremlin_cuda::evaluate(&gremlin_cuda::Request {
            functions,
            inputs: inputs[..active_cases].to_vec(),
            max_steps: 80000,
            memory_budget: 1 << 30,
        })?;
        peak = peak.max(result.telemetry.allocated_bytes);
        device = result.telemetry.device;
        let mut ranked = vec![];
        for (row, brain) in result.executions.iter().zip(&population) {
            let mut score = (0, 0, 0, 0);
            for (execution, room) in row.iter().zip(&maps) {
                let native = simulate(*brain, room, false).0;
                if execution.outcome != Outcome::Completed(Value::new(Type::U64, native)) {
                    return Err(format!("GPU/independent simulator mismatch: {execution:?}"));
                }
                add_score(&mut score, native);
                steps += execution.steps;
            }
            ranked.push((score, *brain));
        }
        evaluations += population.len() * active_cases;
        ranked.sort_unstable_by(|a, b| b.cmp(a));
        best = ranked[0].1;
        final_score = ranked[0].0;
        generation = gen;
        validate_brain(best)?;
        emit(
            json!({"kind":"progress","mode":"gpu","generation":gen,"cases":active_cases,"requested_cases":count,"solved":final_score.0,"keys":final_score.1,"seconds":clock.elapsed().as_secs_f64(),"evaluations":evaluations,"brain":format!("{best:012x}"),"decisions":decisions(best),"brain_source":brain_source(best),"previews":previews(best,&maps),"device":device}),
        );
        if final_score.0 == active_cases as u64 && active_cases < count && gen < 80 {
            emit(json!({"kind":"curriculum","from":active_cases,"to":count,"generation":gen}));
            active_cases = count;
            // Regrade the complete population on the expanded training set.
            // Holdout results never feed back into selection or this decision.
            continue;
        }
        if final_score.0 == active_cases as u64 || gen == 80 {
            break;
        }
        population = ranked.iter().take(16).map(|x| x.1).collect();
        while population.len() < 256 {
            let mut child = ranked[rng.index(48)].1;
            for _ in 0..(1 + rng.index(3)) {
                let row = rng.index(16);
                child = (child & !(7 << (row * 3))) | ((rng.next_u64() & 7) << (row * 3));
            }
            if rng.index(20) == 0 {
                child = rng.next_u64() & MASK;
            }
            population.push(child);
        }
    }
    let seconds = clock.elapsed().as_secs_f64();
    let f = parse(&source(best))?;
    let mut interpreter = Evaluator::new(&f)?;
    let mut holdout_score = (0, 0, 0, 0);
    for r in &holdout {
        let value = simulate(best, r, false).0;
        if interpreter.execute(&input(r), 80000).outcome
            != Outcome::Completed(Value::new(Type::U64, value))
        {
            return Err("holdout interpreter mismatch".into());
        }
        add_score(&mut holdout_score, value);
    }
    emit(
        json!({"kind":"done","mode":"gpu","seed":seed,"cases":active_cases,"requested_cases":count,"generation":generation,"solved":final_score.0,"converged":final_score.0==count as u64,"holdout_solved":holdout_score.0,"holdout_cases":512,"seconds":seconds,"evaluations":evaluations,"executed_steps":steps,"peak_bytes":peak,"device":device,"brain":format!("{best:012x}"),"decisions":decisions(best),"brain_source":brain_source(best),"episode_source":source(best),"previews":previews(best,&holdout),"corpus_hash":object_hash(&maps[..active_cases].iter().map(room_json).collect::<Vec<_>>()),"holdout_hash":object_hash(&holdout.iter().map(room_json).collect::<Vec<_>>()),"evidence":"TESTED on disjoint generated branching mazes; arbitrary rooms may fail within the action budget"}),
    );
    Ok(())
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = (|| {
        if args.len() != 4 || args[0] != "gpu" || args[1] != "robot" {
            return Err("usage: tiny-robot gpu robot 128|512 seed(1..3)".into());
        }
        let count = args[2].parse().map_err(|_| "invalid room count")?;
        let seed = args[3].parse().map_err(|_| "invalid seed")?;
        if ![128, 512].contains(&count) || !(1..=3).contains(&seed) {
            return Err("unsupported demo settings".into());
        }
        search(seed, count)
    })();
    if let Err(error) = result {
        emit(json!({"kind":"error","message":error}));
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interpreter_agrees_with_independent_simulator() {
        let maps = rooms(12, 123);
        let mut rng = Rng::new(456);
        for _ in 0..12 {
            let brain = rng.next_u64() & MASK;
            validate_brain(brain).unwrap();
            let f = parse(&source(brain)).unwrap();
            let mut evaluator = Evaluator::new(&f).unwrap();
            for r in &maps {
                assert_eq!(
                    evaluator.execute(&input(r), 80000).outcome,
                    Outcome::Completed(Value::new(Type::U64, simulate(brain, r, false).0))
                );
            }
        }
    }
    #[test]
    fn generated_rooms_are_unique_and_key_precedes_locked_door() {
        let maps = rooms(512, 78);
        let ids: BTreeSet<_> = maps.iter().map(identity).collect();
        assert_eq!(ids.len(), 512);
        for r in maps {
            let open = !r.walls;
            let cells = open.count_ones();
            let edges = (0..64)
                .filter(|p| open >> p & 1 != 0)
                .map(|p| adjacent(p).iter().filter(|n| open >> **n & 1 != 0).count())
                .sum::<usize>()
                / 2;
            assert_eq!(edges, cells as usize - 1);
            assert_ne!(distances(open & !(1 << r.door), r.start)[r.key], 999);
            assert_eq!(distances(open & !(1 << r.door), r.start)[r.exit], 999);
            assert_ne!(distances(open, r.key)[r.exit], 999);
        }
    }
}
