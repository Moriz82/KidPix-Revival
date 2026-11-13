//! usage: import <zip|iso|dir> <dest>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = std::time::Instant::now();
    let mut last = 0;
    let r = kp_import::import(a[1].as_ref(), a[2].as_ref(), &mut |p| {
        let pct = p.done_bytes * 100 / p.total_bytes.max(1);
        if pct != last {
            last = pct;
            eprint!("\r{pct:3}% {}            ", p.current);
        }
    });
    eprintln!();
    println!("{r:?} in {:?}", t.elapsed());
}
