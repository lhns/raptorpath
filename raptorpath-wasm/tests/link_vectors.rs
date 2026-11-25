//! The forward leg (src/link.rs) and the resend budget (src/budget.rs)
//! against vectors driven through the offline simulation's own `Sim.fwd` and
//! `Sim.send_copy` (tests/gen_link_vectors.py; addendum 1 A4, A5 of
//! docs/derivations/visualizer-three-knobs-plan.md). The leg bit for bit; the
//! budget decision for decision, except at the recorded window boundaries
//! where the integer-tick form is the model's.

use raptorpath_wasm::{budget, link};

const VECTORS: &str = include_str!("link_vectors.txt");

fn f(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex f64"))
}

#[test]
fn the_leg_matches_the_toy_bit_for_bit() {
    let mut cfg: Option<(f64, f64, f64)> = None;
    let mut free = 0.0f64;
    let (mut n, mut drops, mut cfgs) = (0usize, 0usize, 0usize);
    for line in VECTORS.lines().filter(|l| !l.starts_with('#')) {
        let mut it = line.split(' ');
        match it.next() {
            Some("leg_cfg") => {
                let v: Vec<f64> = it.map(f).collect();
                cfg = Some((v[0], v[1], v[2]));
                free = 0.0;
                cfgs += 1;
            }
            Some("leg") => {
                let (c, q, d) = cfg.expect("leg_cfg first");
                let (sv, qs) = (1.0 / c, q / c);
                let t = f(it.next().unwrap());
                assert_eq!(it.next(), Some("->"));
                let want: Vec<&str> = it.collect();
                let got = link::leg_send(&mut free, t, sv, qs, d);
                match (got, want.as_slice()) {
                    (None, ["drop"]) => drops += 1,
                    (Some((dep, arr)), [wd, wa]) => {
                        assert_eq!((dep.to_bits(), arr.to_bits()), (f(wd).to_bits(), f(wa).to_bits()), "{line}");
                    }
                    (g, w) => panic!("{line}: model {g:?}, toy {w:?}"),
                }
                n += 1;
            }
            _ => {}
        }
    }
    assert!(cfgs >= 4 && n > 3000 && drops > 100, "{cfgs} legs, {n} sends, {drops} drops");
}

#[test]
fn the_budget_matches_the_toy_off_the_window_boundaries() {
    let mut used = std::collections::HashMap::<usize, u32>::new();
    let (mut n, mut boundary, mut braked) = (0usize, 0usize, 0usize);
    for line in VECTORS.lines().filter(|l| l.starts_with("brake ")) {
        let v: Vec<&str> = line.split(' ').collect();
        let tick: u32 = v[1].parse().unwrap();
        let (toy, model, tag) = (v[3] == "1", v[4] == "1", v[5]);
        let w = budget::window(tick);
        let u = used.entry(w).or_insert(0);
        let ok = budget::admits(*u, budget::ENGINE_RESENDS_PER_WINDOW);
        if ok {
            *u += 1;
        }
        assert_eq!(ok, model, "{line}");
        match tag {
            "toy" => assert_eq!(ok, toy, "{line}"),
            "boundary" => boundary += 1,
            other => panic!("unknown tag {other}"),
        }
        braked += !ok as usize;
        n += 1;
    }
    assert!(n > 1000 && braked > 100, "{n} attempts, {braked} braked");
    assert!(boundary < n / 50, "{boundary} boundary attempts of {n}");
    println!("budget vectors: {n} attempts, {braked} braked, {boundary} at a window boundary");
}
