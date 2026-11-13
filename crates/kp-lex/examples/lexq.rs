//! `cargo run -p kp-lex --example lexq -- <usa86.lex> word…`: check words and print suggestions.
fn main() {
    let mut a = std::env::args().skip(1);
    let l = kp_lex::Lexicon::open(a.next().expect("usa86.lex path")).expect("open");
    println!("{} entries", l.len());
    for w in a {
        println!("{w:12} known={} bin={:?} suggest={:?}", l.contains(&w), l.bin(&w), l.suggest(&w, 8));
    }
}
