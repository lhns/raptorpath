// SCRATCH diagnosis — per-tick traces for bulk vs fixed. Not for commit.
// Run: cargo run -p raptorpath-wasm --example diag --release
use raptorpath_wasm::Simulation;

fn trace(eps: f64, q: f64, rtt: u32, hint: &str, fixed_r: Option<f64>) {
    let mut sim = Simulation::new(eps, q, rtt, 64, hint.into(), fixed_r, None, None);
    let mut send_done_tick = 0u32;
    let mut fec_at_send_done = 0u32;
    let mut arq_at_send_done = 0u32;
    let mut fec_3rtt = 0u32; // FEC in the first 3 RTTs (cold start)
    let mut arq_3rtt = 0u32;
    let mut max_rate = 0.0f64;
    let mut rate_samples: Vec<(u32, f64, f64)> = Vec::new(); // (tick, rate, p_upper)
    while !sim.is_finished() && sim.get_tick() < 20_000 {
        sim.step();
        let t = sim.get_tick();
        let r = sim.get_r_star();
        if r > max_rate {
            max_rate = r;
        }
        if t <= 3 * rtt {
            fec_3rtt = sim.get_total_fec();
            arq_3rtt = sim.get_total_arq();
        }
        if send_done_tick == 0 && sim.get_total_src() >= sim.get_num_source() {
            send_done_tick = t;
            fec_at_send_done = sim.get_total_fec();
            arq_at_send_done = sim.get_total_arq();
        }
        for probe in [5u32, 20, 60, 150, 300, 450] {
            if t == probe {
                rate_samples.push((t, r, sim.get_p_upper()));
            }
        }
    }
    println!(
        "{:11} eps={:.2} q={:.1} rtt={:3} | finish={:4} send_done={:4} tail={:3} | FEC: 3rtt={:3} at_send_done={:3} total={:3} tail_fec={:2} | ARQ: 3rtt={:3} steady={:3} tail={:3} total={:3} | max_r={:.3}",
        if fixed_r.is_some() { format!("fixed {:.2}", fixed_r.unwrap()) } else { hint.to_string() },
        eps, q, rtt,
        sim.get_tick(), send_done_tick,
        sim.get_tick() - send_done_tick,
        fec_3rtt, fec_at_send_done, sim.get_total_fec(),
        sim.get_total_fec() - fec_at_send_done,
        arq_3rtt, arq_at_send_done, sim.get_total_arq() - arq_at_send_done, sim.get_total_arq(),
        max_rate,
    );
    let samples: Vec<String> = rate_samples
        .iter()
        .map(|(t, r, p)| format!("t={} r={:.3} p_up={:.3}", t, r, p))
        .collect();
    println!("            rate trace: {}", samples.join(" | "));
}

fn main() {
    for &(eps, q, rtt) in &[
        (0.01, 0.5, 50u32),
        (0.01, 0.5, 100),
        (0.05, 0.5, 50),
        (0.05, 0.5, 100),
        (0.10, 0.5, 50),
        (0.10, 0.5, 100),
        (0.10, 0.3, 100),
    ] {
        trace(eps, q, rtt, "bulk", None);
        trace(eps, q, rtt, "fixed", Some(0.01));
        println!();
    }
}
