use polyntu::amm::{Side, calculate};
use std::{hint::black_box, time::Instant};
fn main() {
    for outcomes in [2, 3, 8] {
        let start = Instant::now();
        for _ in 0..10_000 {
            black_box(calculate(black_box(&vec![0; outcomes]), 100, 0, Side::Buy, 10_000).unwrap());
        }
        println!(
            "{outcomes} outcomes: {:.2} us/quote (10000 iterations)",
            start.elapsed().as_secs_f64() * 100.0
        );
    }
}
