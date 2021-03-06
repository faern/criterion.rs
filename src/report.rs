use stats::bivariate::Data;
use stats::bivariate::regression::Slope;
use stats::univariate::outliers::tukey::LabeledSample;

use format;
use estimate::Estimates;

pub fn abs(estimates: &Estimates) {
    for (&statistic, estimate) in estimates.iter() {
        let ci = estimate.confidence_interval;
        let lb = format::time(ci.lower_bound);
        let ub = format::time(ci.upper_bound);

        println!("  > {:>6} [{} {}]", statistic, lb, ub);
    }
}

pub fn rel(estimates: &Estimates) {
    for (&statistic, estimate) in estimates.iter() {
        let ci = estimate.confidence_interval;
        let lb = format::change(ci.lower_bound, true);
        let ub = format::change(ci.upper_bound, true);

        println!("  > {:>6} [{} {}]", statistic, lb, ub);
    }
}

pub fn outliers(sample: LabeledSample<f64>) {
    let (los, lom, _, him, his) = sample.count();
    let noutliers = los + lom + him + his;
    let sample_size = sample.as_slice().len();

    if noutliers == 0 {
        return;
    }

    let percent = |n: usize| 100. * n as f64 / sample_size as f64;

    println!("> Found {} outliers among {} measurements ({:.2}%)",
             noutliers,
             sample_size,
             percent(noutliers));

    let print = |n, label| {
        if n != 0 {
            println!("  > {} ({:.2}%) {}", n, percent(n), label);
        }
    };

    print(los, "low severe");
    print(lom, "low mild");
    print(him, "high mild");
    print(his, "high severe");
}

pub fn regression(data: Data<f64, f64>, (lb, ub): (Slope<f64>, Slope<f64>)) {
    println!(
        "  > {:>6} [{} {}]",
        "slope",
        format::time(lb.0),
        format::time(ub.0),
        );

    println!("  > {:>6}  {:0.7} {:0.7}",
             "R^2",
             lb.r_squared(data),
             ub.r_squared(data));
}
