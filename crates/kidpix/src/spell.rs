//! Toolbox > Check Spelling, with the CD's own Franklin/Proximity lexicon
//! (`KpInstall/usa86.lex` + `engphon.env`, read by the `kp-lex` crate) plus the artist's
//! learned words (`user.dct`, as in the original). Only when the lexicon is missing from
//! the imported data does it fall back to the SCOWL English word list (sizes 10–50,
//! `data/words.z`, see `data/SCOWL-LICENSE.txt`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The lexicon of a game root, loaded once.
fn lexicon(root: &Path) -> Option<&'static kp_lex::Lexicon> {
    static L: OnceLock<Mutex<HashMap<PathBuf, &'static kp_lex::Lexicon>>> = OnceLock::new();
    let mut m = L.get_or_init(Default::default).lock().ok()?;
    if let Some(l) = m.get(root) {
        return Some(l);
    }
    let dir = root.join("KpInstall");
    let path = ["usa86.lex", "USA86.LEX"].iter().map(|n| dir.join(n)).find(|p| p.is_file())?;
    match kp_lex::Lexicon::open(&path) {
        Ok(l) => {
            let l: &'static kp_lex::Lexicon = Box::leak(Box::new(l));
            m.insert(root.to_path_buf(), l);
            Some(l)
        }
        Err(e) => {
            eprintln!("spelling lexicon {}: {e}", path.display());
            None
        }
    }
}

static WORDS: &[u8] = include_bytes!("../data/words.z");

/// Lower-case word → frequency level (10 = most common … 50).
fn dict() -> &'static HashMap<String, u8> {
    static D: OnceLock<HashMap<String, u8>> = OnceLock::new();
    D.get_or_init(|| {
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(WORDS).unwrap_or_default();
        let text: String = raw.iter().map(|&b| b as char).collect();
        let mut m = HashMap::with_capacity(110_000);
        let mut level = 50u8;
        for line in text.lines() {
            if let Some(l) = line.strip_prefix('#') {
                level = l.parse().unwrap_or(50);
            } else if !line.is_empty() {
                m.entry(line.to_lowercase()).or_insert(level);
            }
        }
        m
    })
}

pub struct Checker {
    lex: Option<&'static kp_lex::Lexicon>,
    /// Words the artist taught it ("Learn"), lower case.
    pub learned: Vec<String>,
    /// Words skipped during this check.
    skipped: Vec<String>,
}

impl Checker {
    /// A checker for the game at `root` (its `KpInstall` lexicon).
    pub fn new(root: &Path, learned: Vec<String>) -> Checker {
        Checker { lex: lexicon(root), learned, skipped: Vec::new() }
    }

    /// Whether the CD's lexicon is in use (else the SCOWL fallback).
    pub fn uses_lexicon(&self) -> bool {
        self.lex.is_some()
    }

    pub fn known(&self, word: &str) -> bool {
        let w = word.trim_matches('\'').to_lowercase();
        if w.is_empty() || w.chars().any(|c| c.is_ascii_digit()) || w.chars().count() == 1 {
            return true;
        }
        if self.learned.contains(&w) || self.skipped.contains(&w) {
            return true;
        }
        if let Some(l) = self.lex {
            return l.contains(word.trim_matches('\''));
        }
        dict().contains_key(&w) || self.learned.contains(&w) || self.skipped.contains(&w) || {
            // Possessives and plurals of known words.
            let base = w.strip_suffix("'s").or_else(|| w.strip_suffix('s'));
            base.is_some_and(|b| dict().contains_key(b))
        }
    }

    pub fn skip(&mut self, word: &str) {
        self.skipped.push(word.to_lowercase());
    }

    pub fn learn(&mut self, word: &str) {
        let w = word.to_lowercase();
        if !self.learned.contains(&w) {
            self.learned.push(w);
        }
    }

    /// Up to 8 suggestions. With the lexicon: its misspelling corrections, then words that
    /// sound alike or are one edit away. Fallback: known words one edit away, then two edits away, the most
    /// common first; the case of the first letter follows the word.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        if let Some(l) = self.lex {
            return l.suggest(word, 8);
        }
        let w = word.to_lowercase();
        let d = dict();
        let mut found: Vec<(u8, usize, String)> = Vec::new();
        let e1 = edits1(&w);
        for c in &e1 {
            if let Some(&lv) = d.get(c) {
                found.push((lv, 1, c.clone()));
            }
        }
        if found.len() < 4 && w.len() <= 12 {
            for c in &e1 {
                for c2 in edits1(c) {
                    if let Some(&lv) = d.get(&c2) {
                        found.push((lv, 2, c2));
                    }
                }
            }
        }
        found.sort_by(|a, b| (a.1, a.0, &a.2).cmp(&(b.1, b.0, &b.2)));
        let mut out: Vec<String> = Vec::new();
        for (_, _, s) in found {
            if s != w && !out.contains(&s) {
                out.push(s);
            }
            if out.len() == 8 {
                break;
            }
        }
        let cap = word.chars().next().is_some_and(|c| c.is_uppercase());
        out.into_iter()
            .map(|s| if cap { s[..1].to_uppercase() + &s[1..] } else { s })
            .collect()
    }
}

fn edits1(w: &str) -> Vec<String> {
    let cs: Vec<char> = w.chars().collect();
    let letters = "abcdefghijklmnopqrstuvwxyz'";
    let mut v = Vec::new();
    for i in 0..=cs.len() {
        if i < cs.len() {
            let mut d = cs.clone();
            d.remove(i);
            v.push(d.iter().collect());
        }
        if i + 1 < cs.len() {
            let mut t = cs.clone();
            t.swap(i, i + 1);
            v.push(t.iter().collect());
        }
        for l in letters.chars() {
            if i < cs.len() && cs[i] != l {
                let mut r = cs.clone();
                r[i] = l;
                v.push(r.iter().collect());
            }
            let mut ins = cs.clone();
            ins.insert(i, l);
            v.push(ins.iter().collect());
        }
    }
    v
}

/// Byte ranges of the words in `text` (letters and inner apostrophes).
pub fn words(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (k, &(i, c)) in chars.iter().enumerate() {
        let inner_apos = c == '\'' && start.is_some() && chars.get(k + 1).is_some_and(|n| n.1.is_alphabetic());
        if c.is_alphabetic() || inner_apos {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            out.push((s, i));
        }
    }
    if let Some(s) = start {
        out.push((s, text.len()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_and_suggests() {
        let c = Checker::new(Path::new("/nonexistent"), vec![]);
        assert!(!c.uses_lexicon());
        assert!(c.known("picture") && c.known("Kid") && c.known("isn't"));
        assert!(!c.known("pictre"));
        assert!(c.suggest("pictre").contains(&"picture".to_string()));
        assert_eq!(c.suggest("Helo").first().map(|s| s.chars().next().unwrap().is_uppercase()), Some(true));
        assert_eq!(words("Kid Pix's fun, isn't it?").len(), 5);
    }
}

#[cfg(test)]
mod lexicon_tests {
    use super::*;

    #[test]
    fn uses_cd_lexicon() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game");
        if !root.join("KpInstall/usa86.lex").is_file() {
            return;
        }
        let c = Checker::new(&root, vec!["kidpix".into()]);
        assert!(c.uses_lexicon());
        assert!(c.known("picture") && c.known("Kidpix") && !c.known("pictre"));
        assert_eq!(c.suggest("pictre").first().map(String::as_str), Some("picture"));
        assert_eq!(c.suggest("dogg").first().map(String::as_str), Some("dog"));
    }
}
