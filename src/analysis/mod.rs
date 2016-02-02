use std::fmt;
use std::iter::IntoIterator;
use std::path::Path;
use std::process::Command;

use stats::Distribution;
use stats::bivariate::Data;
use stats::bivariate::regression::Slope;
use stats::univariate::{Sample,Percentiles};
use stats::univariate::outliers::tukey::{LabeledSample, self};
use time;

use estimate::{Distributions, Estimates, Statistic};
use program::Program;
use routine::{Function, Routine};
use {Bencher, ConfidenceInterval, Criterion, Estimate};
use {format, fs, plot, report};
use ::Fun;

macro_rules! elapsed {
    ($msg:expr, $block:expr) => ({
        let start = time::precise_time_ns();
        let out = $block;
        let stop = time::precise_time_ns();

        info!("{} took {}", $msg, format::time((stop - start) as f64));

        out
    })
}

mod compare;

pub fn summarize(id: &str, criterion: &Criterion) {
    if criterion.plotting.is_enabled() {
        print!("Summarizing results of {}... ", id);
        plot::summarize(id);
        println!("DONE\n");
    } else {
        println!("Plotting disabled, skipping summarization");
    }
}

pub fn function<F>(id: &str, f: F, criterion: &Criterion) where F: FnMut(&mut Bencher) {
    common(id, &mut Function(f), criterion);

    println!("");
}

pub fn functions<I>(id: &str,
    funs: Vec<Fun<I>>,
    input: &I,
    criterion: &Criterion) -> Vec<(String, Percentiles<f64>)> where
    I: fmt::Display
{
    let mut percentiles = vec![];
    for fun in funs.into_iter() {
        let id = format!("{}/{}", id, fun.n);
        let mut f = fun.f;

        let this_percentiles = common(&id, &mut Function(|b| f(b, input)), criterion);
        percentiles.push((id, this_percentiles));
    }

    summarize(id, criterion);
    percentiles
}

pub fn function_with_inputs<I, F>(
    id: &str,
    mut f: F,
    inputs: I,
    criterion: &Criterion,
) where
    F: FnMut(&mut Bencher, &I::Item),
    I: IntoIterator,
    I::Item: fmt::Display,
{
    for input in inputs {
        let id = format!("{}/{}", id, input);

        common(&id, &mut Function(|b| f(b, &input)), criterion);
    }

    summarize(id, criterion);
}

pub fn program(id: &str, prog: &mut Command, criterion: &Criterion) {
    common(id, &mut Program::spawn(prog), criterion);

    println!("");
}

pub fn program_with_inputs<I, F>(
    id: &str,
    mut prog: F,
    inputs: I,
    criterion: &Criterion,
) where
    F: FnMut() -> Command,
    I: IntoIterator,
    I::Item: fmt::Display,
{
    for input in inputs {
        let id = format!("{}/{}", id, input);

        program(&id, prog().arg(&format!("{}", input)), criterion);
    }

    summarize(id, criterion);
}

// Common analysis procedure
fn common<R>(id: &str, routine: &mut R, criterion: &Criterion) -> Percentiles<f64> where
    R: Routine,
{
    println!("Benchmarking {}", id);

    let (iters, times) = routine.sample(criterion);

    rename_new_dir_to_base(id);

    let avg_times = iters.iter().zip(times.iter()).map(|(&iters, &elapsed)| {
        elapsed / iters
    }).collect::<Vec<f64>>();
    let avg_times = Sample::new(&avg_times);

    fs::mkdirp(&format!(".criterion/{}/new", id));

    let data = Data::new(&iters, &times);
    let labeled_sample = outliers(id, avg_times);
    if criterion.plotting.is_enabled() {
        elapsed!(
            "Plotting the estimated sample PDF",
            plot::pdf(data, labeled_sample, id));
    }
    let (distribution, slope) = regression(id, data, criterion);
    let (mut distributions, mut estimates) = estimates(avg_times, criterion);

    estimates.insert(Statistic::Slope, slope);
    distributions.insert(Statistic::Slope, distribution);

    if criterion.plotting.is_enabled() {
        elapsed!(
            "Plotting the distribution of the absolute statistics",
            plot::abs_distributions(
                &distributions,
                &estimates,
                id));
    }

    fs::save(
        &(data.x().as_slice(), data.y().as_slice()),
        &format!(".criterion/{}/new/sample.json", id));
    fs::save(&estimates, &format!(".criterion/{}/new/estimates.json", id));

    if base_dir_exists(id) {
        compare::common(id, data, avg_times, &estimates, criterion);
    }

    avg_times.percentiles()
}

fn base_dir_exists(id: &str) -> bool {
    Path::new(&format!(".criterion/{}/base", id)).exists()
}

// Performs a simple linear regression on the sample
fn regression(
    id: &str,
    data: Data<f64, f64>,
    criterion: &Criterion,
) -> (Distribution<f64>, Estimate) {
    let cl = criterion.confidence_level;

    println!("> Performing linear regression");

    let distribution = elapsed!(
        "Bootstrapped linear regression",
        data.bootstrap(criterion.nresamples, |d| (Slope::fit(d).0,))).0;

    let point = Slope::fit(data);
    let (lb, ub) =  distribution.confidence_interval(criterion.confidence_level);
    let se = distribution.std_dev(None);

    let (lb_, ub_) = (Slope(lb), Slope(ub));

    report::regression(data, (lb_, ub_));

    if criterion.plotting.is_enabled() {
        elapsed!(
            "Plotting linear regression",
            plot::regression(
                data,
                &point,
                (lb_, ub_),
                id));
    }

    (distribution, Estimate {
        confidence_interval: ConfidenceInterval {
            confidence_level: cl,
            lower_bound: lb,
            upper_bound: ub,
        },
        point_estimate: point.0,
        standard_error: se,
    })
}

// Classifies the outliers in the sample
fn outliers<'a>(id: &str, avg_times: &'a Sample<f64>) -> LabeledSample<'a, f64> {
    let sample = tukey::classify(avg_times);

    report::outliers(sample);
    fs::save(&sample.fences(), &format!(".criterion/{}/new/tukey.json", id));

    sample
}

// Estimates the statistics of the population from the sample
fn estimates(
    avg_times: &Sample<f64>,
    criterion: &Criterion,
) -> (Distributions, Estimates) {
    fn stats(sample: &Sample<f64>) -> (f64, f64, f64, f64) {
        let mean = sample.mean();
        let std_dev = sample.std_dev(Some(mean));
        let median = sample.percentiles().median();
        let mad = sample.median_abs_dev(Some(median));

        (mean, median, mad, std_dev)
    }

    let cl = criterion.confidence_level;
    let nresamples = criterion.nresamples;

    let points = {
        let (a, b, c, d) = stats(avg_times);

        [a, b, c, d]
    };

    println!("> Estimating the statistics of the sample");
    let distributions = {
        let (a, b, c, d) = elapsed!(
        "Bootstrapping the absolute statistics",
        avg_times.bootstrap(nresamples, stats));

        vec![a, b, c, d]
    };
    let statistics = [
        Statistic::Mean,
        Statistic::Median,
        Statistic::MedianAbsDev,
        Statistic::StdDev,
    ];
    let distributions: Distributions = statistics.iter().map(|&x| {
        x
    }).zip(distributions.into_iter()).collect();
    let estimates = Estimate::new(&distributions, &points, cl);

    report::abs(&estimates);

    (distributions, estimates)
}

fn rename_new_dir_to_base(id: &str) {
    let root_dir = Path::new(".criterion").join(id);
    let base_dir = root_dir.join("base");
    let new_dir = root_dir.join("new");

    if base_dir.exists() { fs::rmrf(&base_dir) }
    if new_dir.exists() { fs::mv(&new_dir, &base_dir) };
}
