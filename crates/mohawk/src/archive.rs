use crate::{bad, Reader, Result};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

/// Four-character resource type tag, e.g. `Tag(*b"tBMP")`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tag(pub [u8; 4]);

impl Tag {
    pub const fn new(s: &[u8; 4]) -> Tag {
        Tag(*s)
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'{}'", String::from_utf8_lossy(&self.0))
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

/// One resource in the archive.
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: u16,
    pub offset: usize,
    pub size: usize,
    pub name: Option<String>,
}

/// A parsed Mohawk archive held in memory.
#[derive(Clone)]
pub struct Archive {
    data: Arc<[u8]>,
    types: BTreeMap<Tag, BTreeMap<u16, Entry>>,
}

impl fmt::Debug for Archive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_map();
        for (t, m) in &self.types {
            d.entry(t, &m.len());
        }
        d.finish()
    }
}

impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Archive> {
        let data = std::fs::read(path)?;
        Archive::from_bytes(data)
    }

    pub fn from_bytes(data: impl Into<Arc<[u8]>>) -> Result<Archive> {
        let data: Arc<[u8]> = data.into();
        let types = parse(&data)?;
        Ok(Archive { data, types })
    }

    /// All resource types present.
    pub fn tags(&self) -> impl Iterator<Item = Tag> + '_ {
        self.types.keys().copied()
    }

    /// Sorted ids of all resources of a type.
    pub fn ids(&self, tag: Tag) -> Vec<u16> {
        self.types
            .get(&tag)
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default()
    }

    pub fn entries(&self, tag: Tag) -> impl Iterator<Item = &Entry> + '_ {
        self.types.get(&tag).into_iter().flat_map(|m| m.values())
    }

    pub fn entry(&self, tag: Tag, id: u16) -> Option<&Entry> {
        self.types.get(&tag)?.get(&id)
    }

    pub fn has(&self, tag: Tag, id: u16) -> bool {
        self.entry(tag, id).is_some()
    }

    /// Raw bytes of a resource.
    pub fn get(&self, tag: Tag, id: u16) -> Option<&[u8]> {
        let e = self.entry(tag, id)?;
        self.data.get(e.offset..e.offset + e.size)
    }

    /// Find a resource by name (names are rare in Kid Pix archives).
    pub fn find_by_name(&self, tag: Tag, name: &str) -> Option<u16> {
        self.types
            .get(&tag)?
            .values()
            .find(|e| e.name.as_deref() == Some(name))
            .map(|e| e.id)
    }

    pub fn raw(&self) -> &[u8] {
        &self.data
    }
}

fn parse(b: &[u8]) -> Result<BTreeMap<Tag, BTreeMap<u16, Entry>>> {
    let mut r = Reader::new(b);
    if r.bytes(4)? != b"MHWK" {
        return bad("not a Mohawk archive (missing MHWK)");
    }
    r.u32()?;
    if r.bytes(4)? != b"RSRC" {
        return bad("not a Mohawk archive (missing RSRC)");
    }
    let _version = r.u16()?;
    let _compaction = r.u16()?;
    let _total = r.u32()?;
    let dir = r.u32()? as usize;
    let file_table_off = r.u16()? as usize;
    let _file_table_size = r.u16()?;

    let mut d = Reader::at(b, dir);
    let name_list = d.u16()? as usize;
    let type_count = d.u16()? as usize;

    // File table.
    let mut ft = Reader::at(b, dir + file_table_off);
    let file_count = ft.u32()? as usize;
    if file_count > 1_000_000 {
        return bad("absurd file count");
    }
    let mut files = Vec::with_capacity(file_count);
    for _ in 0..file_count {
        let offset = ft.u32()? as usize;
        let lo = ft.u16()? as usize;
        let hi = ft.u8()? as usize;
        let flags = ft.u8()? as usize;
        let _unk = ft.u16()?;
        let size = lo | (hi << 16) | ((flags & 7) << 24);
        files.push((offset, size));
    }
    // The size field is occasionally wrong in Mohawk archives; clamp to the next
    // file's offset / end of data so slicing never fails.
    let mut sorted: Vec<usize> = files.iter().map(|f| f.0).collect();
    sorted.sort_unstable();
    sorted.dedup();
    let clamp = |off: usize, size: usize| -> usize {
        let next = match sorted.binary_search(&off) {
            Ok(i) => sorted.get(i + 1).copied().unwrap_or(b.len()),
            Err(_) => b.len(),
        };
        let max = next.min(b.len()).saturating_sub(off);
        size.min(max)
    };

    let mut types = BTreeMap::new();
    for t in 0..type_count {
        let mut tr = Reader::at(b, dir + 4 + t * 8);
        let tag = Tag(tr.bytes(4)?.try_into().unwrap());
        let res_table = dir + tr.u16()? as usize;
        let name_table = dir + tr.u16()? as usize;

        let mut rt = Reader::at(b, res_table);
        let count = rt.u16()? as usize;
        let mut map: BTreeMap<u16, Entry> = BTreeMap::new();
        let mut by_index: Vec<(u16, u16)> = Vec::with_capacity(count);
        for _ in 0..count {
            let id = rt.u16()?;
            let index = rt.u16()?;
            let Some(&(offset, size)) = files.get((index as usize).wrapping_sub(1)) else {
                return bad(format!("{tag} {id}: bad file index {index}"));
            };
            map.insert(
                id,
                Entry {
                    id,
                    offset,
                    size: clamp(offset, size),
                    name: None,
                },
            );
            by_index.push((index, id));
        }

        let mut nt = Reader::at(b, name_table);
        if let Ok(ncount) = nt.u16() {
            for _ in 0..ncount {
                let (Ok(noff), Ok(index)) = (nt.u16(), nt.u16()) else {
                    break;
                };
                let start = dir + name_list + noff as usize;
                if start >= b.len() {
                    continue;
                }
                let end = b[start..]
                    .iter()
                    .position(|&c| c == 0)
                    .map(|p| start + p)
                    .unwrap_or(b.len());
                let name = crate::latin1(&b[start..end]);
                for &(i, id) in &by_index {
                    if i == index {
                        if let Some(e) = map.get_mut(&id) {
                            e.name = Some(name.clone());
                        }
                    }
                }
            }
        }
        types.insert(tag, map);
    }
    Ok(types)
}
