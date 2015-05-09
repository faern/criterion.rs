extern crate criterion;

use criterion::Criterion;

const SIZE: usize = 1024 * 1024;

#[test]
fn dealloc() {
    let setup = || (0..SIZE).collect::<Vec<_>>();
    let iter_f = |mut v| {
        v[0] = 99;
        v
    };
    let verify = |v| {
        assert_eq!(99, v[0]);
        assert_eq!(SIZE, v.len());
    }
    Criterion::default().bench("dealloc", |b| {
        b.iter_with_setup_and_verify(setup, iter_f, verify)
    });
}
