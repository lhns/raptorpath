// SCRATCH investigation example — bulk vs fixed-r comparison grid.
// Not for commit. Run: cargo run -p raptorpath-wasm --example bulk_vs_fixed --release
use raptorpath_wasm::Simulation;

fn run(eps: f64, q: f64, rtt: u32, hint: &str, fixed_r: Option<f64>) -> (u32, f64, f64, u32, u32, f64, f64) {
    let mut sim = Simulation::new(eps, q, rtt, 64, hint.into(), fixed_r, None, None);
    while !sim.is_finished() && sim.get_tick() < 20_000 {
        sim.step();
    }
    let total_oh = (sim.get_total_fec() + sim.get_total_arq()) as f64
        / sim.get_total_src().max(1) as f64 * 100.0;
    (
        sim.get_tick(),
        sim.get_overhead(),        // steady-state overhead %
        sim.get_excess_overhead(), // steady overhead - floor
        sim.get_total_arq(),
        sim.get_total_fec(),
        sim.get_lat_percentile(0.99),
        total_oh,
    )
}

fn main() {
    println!("eps   q    rtt | mode        | ticks  steadyOH% excessOH%  ARQ   FEC   p99ms  totalOH%");
    for &eps in &[0.01f64, 0.05, 0.10] {
        for &q in &[0.3f64, 0.5] {
            for &rtt in &[20u32, 50, 51, 100] {
                let modes: Vec<(String, Option<f64>)> = vec![
                    ("bulk".to_string(), None),
                    ("fixed 0.01".to_string(), Some(0.01)),
                    ("fixed 0.02".to_string(), Some(0.02)),
                    ("fixed 0.05".to_string(), Some(0.05)),
                    ("fixed 0.00".to_string(), Some(0.0)),
                ];
                for (label, r) in modes {
                    let hint = if r.is_some() { "fixed" } else { "bulk" };
                    let (ticks, oh, ex, arq, fec, p99, toh) = run(eps, q, rtt, hint, r);
                    println!(
                        "{:.2} {:.1} {:4} | {:11} | {:6} {:8.2} {:8.2} {:6} {:5} {:7.1} {:8.2}",
                        eps, q, rtt, label, ticks, oh, ex, arq, fec, p99, toh
                    );
                }
                println!();
            }
        }
    }
}
