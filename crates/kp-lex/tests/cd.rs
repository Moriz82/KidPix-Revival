//! Runs against the imported CD data in `game/KpInstall` (skipped when it isn't there).

use kp_lex::Lexicon;
use std::path::PathBuf;

fn lexicon() -> Option<Lexicon> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game/KpInstall/usa86.lex");
    if !p.is_file() {
        eprintln!("skipping: {} missing", p.display());
        return None;
    }
    Some(Lexicon::open(p).expect("open lexicon"))
}

#[test]
fn decodes_and_checks() {
    let Some(l) = lexicon() else { return };
    assert!(l.len() > 100_000, "{} entries", l.len());
    for w in ["picture", "dog", "heckle", "guitar", "likes", "My", "isn't", "Kid"] {
        assert!(l.contains(w), "{w} should be known");
    }
    for w in ["pictre", "dogg", "guitr"] {
        assert!(!l.contains(w), "{w} should be unknown");
    }
    let s = l.suggest("pictre", 8);
    assert!(s.iter().any(|x| x == "picture"), "pictre → {s:?}");
    let s = l.suggest("dogg", 8);
    assert!(s.first().map(String::as_str) == Some("dog") || s.iter().any(|x| x == "dog"), "dogg → {s:?}");
    assert_eq!(l.bin("picture"), l.bin("pictre"));
}
