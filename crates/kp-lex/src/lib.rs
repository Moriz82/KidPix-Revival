//! Reader for the spelling lexicon Kid Pix Deluxe 3 ships for Check Spelling: a
//! Franklin / Proximity Technology word list (`KpInstall/usa86.lex`) whose entries are
//! stored as letter codes grouped by a phonetic key that the phonetic rule set
//! (`KpInstall/engphon.env`) computes. See docs/FORMATS.md ("Spelling lexicon").
//!
//! * [`Lexicon::contains`] checks a word against the ~102,000 entries (with the
//!   capitalisation rules of proper nouns and acronyms, and without the "known
//!   misspelling" entries).
//! * [`Lexicon::suggest`] proposes corrections: the lexicon's own misspelling → correction
//!   pairs first, then words in the same phonetic bin (and words one edit away), closest
//!   spelling first.

use std::collections::HashMap;
use std::path::Path;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Format(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Format(s) => write!(f, "bad lexicon: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Entry flags (the byte after a 0xFF marker in an entry).
pub mod flag {
    /// A common misspelling; its literal spelling is the correction (e.g. "awt" → "ought").
    pub const MISSPELLING: u8 = 1;
    /// A proper noun: written with a capital letter.
    pub const PROPER: u8 = 2;
    /// An acronym: written in capitals.
    pub const ACRONYM: u8 = 3;
    /// Abbreviations written with periods (etc., Aug., e.g.).
    pub const ABBREV: u8 = 5;
    pub const ABBREV_CAP: u8 = 6;
    /// A single letter.
    pub const LETTER: u8 = 7;
    pub const ABBREV_DOTTED: u8 = 9;
    /// The literal holds the exact spelling (case, apostrophes, accents).
    pub const EXACT: u8 = 0x10;
    /// The literal holds the exact dotted spelling (Ph.D).
    pub const EXACT_DOTTED: u8 = 0x14;
}

/// One dictionary entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    /// The spelling made from the letter codes (lower case).
    pub spelling: String,
    /// The exact spelling, when the entry carries one (or the correction of a misspelling).
    pub literal: Option<String>,
    pub flags: Vec<u8>,
    /// Phonetic bin (the first two code bytes of the entry).
    pub bin: [u8; 2],
}

impl Word {
    pub fn is_misspelling(&self) -> bool {
        self.flags.contains(&flag::MISSPELLING)
    }

    /// How the word is written.
    pub fn display(&self) -> String {
        if self.is_misspelling() {
            return self.spelling.clone();
        }
        if let Some(l) = &self.literal {
            return l.clone();
        }
        if self.flags.contains(&flag::ACRONYM) {
            return self.spelling.to_uppercase();
        }
        if self.flags.contains(&flag::PROPER) || self.flags.contains(&flag::ABBREV_CAP) {
            return capitalize(&self.spelling);
        }
        self.spelling.clone()
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn u16le(d: &[u8], o: usize) -> Result<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or(Error::Format("truncated"))
}

/// The phonetic rule set (`engphon.env`).
struct Phonetics {
    /// Header words.
    h: [u16; 16],
    /// Version byte (1 → rule outputs from 10, else from 16).
    version: u8,
    /// Phonetic bins (sorted class strings).
    bins: Vec<Vec<u8>>,
    /// Per letter code: (letter, class).
    pairs: Vec<[u8; 2]>,
    base: Vec<u8>,
    check: Vec<u8>,
    output: Vec<u8>,
    rules: Vec<u8>,
}

impl Phonetics {
    fn parse(e: &[u8]) -> Result<Phonetics> {
        let mut h = [0u16; 16];
        for (i, v) in h.iter_mut().enumerate() {
            *v = u16le(e, 4 + 2 * i)?;
        }
        let take = |o: usize, n: usize| e.get(o..o + n).map(|s| s.to_vec()).ok_or(Error::Format("engphon.env truncated"));
        let bin_area = take(36, h[7] as usize)?;
        let bins: Vec<Vec<u8>> = bin_area.split(|&b| b == 0).take(h[0] as usize).map(|s| s.to_vec()).collect();
        let mut o = 36 + h[7] as usize;
        let pair_bytes = take(o, h[8] as usize)?;
        o += h[8] as usize + 3 * 256;
        let n = h[5] as usize;
        let base = take(o, n)?;
        let check = take(o + n, n)?;
        let output = take(o + 2 * n, n)?;
        let rules = take(o + 3 * n, h[15] as usize)?;
        let pairs = pair_bytes.chunks_exact(2).map(|p| [p[0], p[1]]).collect();
        Ok(Phonetics { h, version: e.first().copied().unwrap_or(0), bins, pairs, base, check, output, rules })
    }

    fn nph(&self) -> usize {
        self.h[1] as usize
    }

    /// Transcribe a lower-case ASCII word into (letter, class) pairs with the trie rules.
    fn transcribe(&self, word: &[u8]) -> Vec<u8> {
        const START: u8 = b'Y';
        const END: u8 = b'Z';
        let mut m = Vec::with_capacity(word.len() + 2);
        m.push(START);
        m.extend_from_slice(word);
        m.push(END);
        let threshold = if self.version == 1 { 10 } else { 16 };
        let second = self.h[6] as usize;
        let mut out = Vec::new();
        let mut p = 0;
        while p < m.len() && m[p] != END {
            // Longest match in the double-array trie.
            let (mut state, mut best_len, mut best_state) = (0usize, 0usize, 0usize);
            let mut q = p;
            while q < m.len() {
                let idx = m[q].wrapping_sub(88) as usize;
                q += 1;
                state = *self.base.get(state).unwrap_or(&0) as usize;
                if state == 0 {
                    break;
                }
                state += idx;
                if state >= self.check.len() || self.check[state] as usize != idx {
                    break;
                }
                if self.output[state] != 127 {
                    best_len = q - p;
                    best_state = state;
                }
            }
            if best_len == 0 {
                if m[p] != START {
                    out.extend_from_slice(&[m[p], 11]);
                }
                p += 1;
                continue;
            }
            let v = self.output[best_state] as usize;
            if v >= threshold {
                let mut r = v - threshold + if best_len == 2 { 0 } else { second };
                if self.rules.get(r).is_some_and(|&b| b >= 0x41) {
                    // A literal replacement string.
                    let end = self.rules[r..].iter().position(|&b| b == 0).map(|e| r + e).unwrap_or(self.rules.len());
                    out.extend_from_slice(&self.rules[r..end]);
                    p += best_len;
                } else {
                    for _ in 0..best_len {
                        if m[p] == START {
                            p += 1;
                        } else {
                            if m[p] != END {
                                out.push(m[p]);
                                p += 1;
                            }
                            out.push(*self.rules.get(r).unwrap_or(&11));
                            r += 1;
                        }
                    }
                }
            } else {
                let mut len = best_len;
                if m[p] == START {
                    len -= 1;
                    p += 1;
                }
                out.push(m[p]);
                p += 1;
                out.push((v + 16) as u8);
                if len == 2 && p < m.len() && m[p] != END {
                    out.extend_from_slice(&[9, 11]);
                    p += 1;
                }
            }
        }
        if let Some(z) = out.iter().position(|&b| b == 0) {
            out.truncate(z);
        }
        out
    }

    /// The phonetic bin of a transcription (which lexicon block group holds the word).
    fn bin_of(&self, pairs: &[u8]) -> [u8; 2] {
        let mut cls = Vec::new();
        let mut prev = 16u8;
        for pr in pairs.chunks(2) {
            let (a, c) = (pr[0], *pr.get(1).unwrap_or(&0));
            if a == 0 || a == 8 {
                break;
            }
            if c > 16 && c <= 31 {
                if c == 25 {
                    cls.push(c);
                } else if c != prev {
                    prev = c;
                    if !matches!(c, 24 | 30 | 31) {
                        cls.push(c);
                    }
                }
            }
        }
        let k = self.bins.partition_point(|b| b.as_slice() <= cls.as_slice()).saturating_sub(1);
        let per = self.h[3].max(1) as usize;
        let first = self.h[2] as usize;
        [(k / per + first) as u8, (k % per + first) as u8]
    }
}

/// Fold a word for lookup: lower case, accents removed, only letters and ' - kept.
pub fn fold(word: &str) -> String {
    word.chars()
        .filter_map(|c| {
            let c = unaccent(c).to_ascii_lowercase();
            (c.is_ascii_lowercase() || c == '\'' || c == '-').then_some(c)
        })
        .collect()
}

fn unaccent(c: char) -> char {
    match c {
        'à'..='å' | 'À'..='Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è'..='ë' | 'È'..='Ë' => 'e',
        'ì'..='ï' | 'Ì'..='Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò'..='ö' | 'ø' | 'Ò'..='Ö' | 'Ø' => 'o',
        'ù'..='ü' | 'Ù'..='Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' => 'y',
        '’' | '‘' => '\'',
        c => c,
    }
}

/// Characters of the lexicon's literal strings (the Proximity/DOS character set, 0x80+).
fn literal_char(b: u8) -> char {
    const HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑ";
    if b < 0x80 {
        return b as char;
    }
    HIGH.chars().nth((b - 0x80) as usize).unwrap_or('?')
}

pub struct Lexicon {
    phon: Phonetics,
    pub words: Vec<Word>,
    /// Folded spelling → entry indices.
    index: HashMap<String, Vec<u32>>,
    /// Phonetic bin → entry indices.
    by_bin: HashMap<[u8; 2], Vec<u32>>,
}

impl Lexicon {
    /// Open `usa86.lex` with its phonetic rules (`engphon.env` next to it).
    pub fn open(lex: impl AsRef<Path>) -> Result<Lexicon> {
        let lex = lex.as_ref();
        let env = lex.with_file_name("engphon.env");
        let env = if env.is_file() { env } else { lex.with_file_name("ENGPHON.ENV") };
        Lexicon::from_bytes(&std::fs::read(lex)?, &std::fs::read(env)?)
    }

    pub fn from_bytes(lex: &[u8], env: &[u8]) -> Result<Lexicon> {
        let phon = Phonetics::parse(env)?;
        let nph = phon.nph();
        let mut hdr = [0usize; 7];
        for (i, v) in hdr.iter_mut().enumerate() {
            *v = u16le(lex, 0x102 + 2 * i)? as usize;
        }
        let [_ver, _flags, _sub, ntok, ndig, nblk, slen] = hdr;
        let mut o = 0x110;
        let drop = lex.get(o..o + ntok).ok_or(Error::Format("token table"))?.to_vec();
        o += ntok;
        let dig: Vec<[u8; 2]> = (0..ndig).map(|i| [lex[o + 2 * i], lex[o + 2 * i + 1]]).collect();
        o += 2 * ndig;
        let strings: Vec<&[u8]> = lex.get(o..o + slen).ok_or(Error::Format("string table"))?.split(|&b| b == 0).collect();
        if strings.len() < ntok + nblk {
            return Err(Error::Format("string table too short"));
        }
        let tokens = &strings[..ntok];
        let keys = &strings[ntok..ntok + nblk];
        let tok0 = nph + 16;
        let dig0 = tok0 + ntok;

        let mut words = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            let start = (5 + i) * 1024;
            let Some(blk) = lex.get(start..start + 1024) else { break };
            let mut buf: Vec<u8> = key.to_vec();
            words.push(buf.clone());
            let mut p = 7;
            while p < blk.len() {
                let c = blk[p] as usize;
                if c == 0 {
                    break;
                }
                p += 1;
                if c >= tok0 && c < dig0 {
                    let k = c - tok0;
                    let n = drop[k] as usize;
                    buf.truncate(buf.len().saturating_sub(n));
                    buf.extend_from_slice(tokens[k]);
                } else {
                    let mut n = c.saturating_sub(nph);
                    if c == nph + 15 {
                        n += (blk[p] as usize).saturating_sub(nph);
                        p += 1;
                    }
                    buf.truncate(buf.len().saturating_sub(n));
                }
                // The entry's tail: letter codes, digraphs, literal runs and flags.
                while p < blk.len() {
                    let c = blk[p] as usize;
                    if c == 0 || (c >= nph && c < dig0) {
                        break;
                    }
                    if c == 255 {
                        buf.extend_from_slice(&[255, blk[p + 1]]);
                        p += 2;
                    } else if c >= dig0 {
                        buf.extend_from_slice(&dig[c - dig0]);
                        p += 1;
                    } else {
                        buf.push(c as u8);
                        p += 1;
                        if c == 4 {
                            let j = blk[p..].iter().position(|&b| b == 255).map(|e| p + e).unwrap_or(blk.len());
                            buf.extend_from_slice(&blk[p..j]);
                            p = j;
                        }
                    }
                }
                words.push(buf.clone());
            }
        }
        let words: Vec<Word> = words.iter().filter(|e| e.len() >= 2).map(|e| Self::decode_entry(&phon, e)).collect();
        let mut index: HashMap<String, Vec<u32>> = HashMap::new();
        let mut by_bin: HashMap<[u8; 2], Vec<u32>> = HashMap::new();
        for (i, w) in words.iter().enumerate() {
            index.entry(fold(&w.spelling)).or_default().push(i as u32);
            if let Some(l) = &w.literal {
                if !w.is_misspelling() {
                    index.entry(fold(l)).or_default().push(i as u32);
                }
            }
            by_bin.entry(w.bin).or_default().push(i as u32);
        }
        Ok(Lexicon { phon, words, index, by_bin })
    }

    /// Spell out one entry's codes.
    fn decode_entry(phon: &Phonetics, e: &[u8]) -> Word {
        let bin = [e[0], e[1]];
        let mut spelling = String::new();
        let mut literal = None;
        let mut flags = Vec::new();
        let mut i = 2;
        while i < e.len() {
            let c = e[i];
            if c == 255 {
                if let Some(&f) = e.get(i + 1) {
                    flags.push(f);
                }
                i += 2;
                continue;
            }
            if c == 4 {
                let j = e[i + 1..].iter().position(|&b| b == 255).map(|x| i + 1 + x).unwrap_or(e.len());
                literal = Some(e[i + 1..j].iter().map(|&b| literal_char(b)).collect());
                i = j;
                continue;
            }
            match phon.pairs.get(c as usize).map(|p| p[0]) {
                // Doubled letter.
                Some(9) => {
                    if let Some(last) = spelling.chars().last() {
                        spelling.push(last);
                    }
                }
                // Silent marker.
                Some(11) => {}
                Some(ch) if ch >= 32 => spelling.push(ch as char),
                _ => {}
            }
            i += 1;
        }
        Word { spelling, literal, flags, bin }
    }

    /// Phonetic bin of a word (for suggestions).
    pub fn bin(&self, word: &str) -> [u8; 2] {
        let w = fold(word);
        let pairs = self.phon.transcribe(w.as_bytes());
        self.phon.bin_of(&pairs)
    }

    fn entries(&self, folded: &str) -> impl Iterator<Item = &Word> {
        self.index.get(folded).into_iter().flatten().map(|&i| &self.words[i as usize])
    }

    /// Is `word` spelt correctly? Proper nouns need a capital, acronyms capitals;
    /// "known misspelling" entries never count.
    pub fn contains(&self, word: &str) -> bool {
        let w = word.trim_matches(|c: char| c == '\'' || c == '-');
        if self.contains_entry(w) {
            return true;
        }
        // Possessives ('s, s') of known words, and hyphenated compounds of known words.
        let base = w.strip_suffix("'s").or_else(|| w.strip_suffix("'S")).or_else(|| w.strip_suffix('\''));
        if base.is_some_and(|b| self.contains_entry(b)) {
            return true;
        }
        w.contains('-') && w.split('-').all(|p| !p.is_empty() && self.contains(p))
    }

    fn contains_entry(&self, word: &str) -> bool {
        let w = word.trim_matches(|c: char| c == '\'' || c == '-');
        if w.is_empty() {
            return true;
        }
        let folded = fold(w);
        if folded.chars().count() <= 1 {
            return true;
        }
        let capital = w.chars().next().is_some_and(|c| c.is_uppercase());
        let all_caps = w.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase());
        self.entries(&folded).any(|e| {
            if e.is_misspelling() {
                return false;
            }
            if e.flags.contains(&flag::ACRONYM) {
                return all_caps;
            }
            if e.flags.contains(&flag::PROPER) || e.flags.contains(&flag::ABBREV_CAP) {
                return capital;
            }
            true
        })
    }

    /// Corrections for a misspelt word (up to `max`), best first, in the word's case.
    pub fn suggest(&self, word: &str, max: usize) -> Vec<String> {
        let folded = fold(word);
        let mut out: Vec<String> = Vec::new();
        let push = |out: &mut Vec<String>, s: String| {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(&s)) && !s.eq_ignore_ascii_case(word) {
                out.push(s);
            }
        };
        // 1. The lexicon's own misspelling → correction pairs.
        for e in self.entries(&folded) {
            if e.is_misspelling() {
                if let Some(c) = &e.literal {
                    push(&mut out, c.clone());
                }
            }
        }
        // 2. Words that sound alike (same phonetic bin) and words one edit away, ranked by
        //    spelling distance; ties prefer ordinary words, the same bin, then the length.
        let bin = self.bin(word);
        let n = folded.chars().count() as isize;
        let mut cands: Vec<(usize, u8, u8, usize, String)> = Vec::new();
        let mut add = |e: &Word, d: usize| {
            let proper = u8::from(e.display().chars().next().is_some_and(|c| c.is_uppercase()));
            let len = (e.spelling.chars().count() as isize - n).unsigned_abs();
            cands.push((d, proper, u8::from(e.bin != bin), len, e.display()));
        };
        for &i in self.by_bin.get(&bin).into_iter().flatten() {
            let e = &self.words[i as usize];
            if !e.is_misspelling() {
                let d = levenshtein(&folded, &fold(&e.spelling));
                if d <= 3 {
                    add(e, d);
                }
            }
        }
        for c in edits1(&folded) {
            for e in self.entries(&c) {
                if !e.is_misspelling() {
                    add(e, 1);
                }
            }
        }
        cands.sort();
        for c in cands {
            push(&mut out, c.4);
        }
        out.truncate(max);
        // Match the case of the first letter.
        let cap = word.chars().next().is_some_and(|c| c.is_uppercase());
        out.into_iter().map(|s| if cap { capitalize(&s) } else { s }).collect()
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    // Optimal-string-alignment distance: a swap of two neighbours counts as one edit.
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev2: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cur[j] = cur[j].min(prev2[j - 2] + 1);
            }
        }
        prev2 = std::mem::replace(&mut prev, cur);
    }
    prev[b.len()]
}

fn edits1(w: &str) -> Vec<String> {
    let cs: Vec<char> = w.chars().collect();
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
        for l in 'a'..='z' {
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
