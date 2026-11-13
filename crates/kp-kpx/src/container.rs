//! Minimal Mohawk container codec tailored to `.kpx` files.
//!
//! The shared `mohawk` crate only reads archives and hides the details a
//! byte-exact rewrite needs (file-table order, the order of the per-type
//! tables, which resources carry names). This module parses those details
//! and writes archives in the exact shape the original program produces
//! (RE_SHELL.md §8.1 / §8.9).

use crate::{Error, Result, Tag};
use std::collections::BTreeSet;

/// Key identifying one resource inside an archive.
pub(crate) type Key = (Tag, u16);

/// Size of the fixed archive header, including the 8 "tail" bytes at 0x1C.
pub(crate) const HEADER_LEN: usize = 0x24;

/// Largest resource the file table can describe (27-bit size field).
const MAX_RESOURCE: usize = (1 << 27) - 1;

/// One resource as found in (or destined for) an archive.
#[derive(Clone, Debug)]
pub(crate) struct RawResource<'a> {
    pub tag: Tag,
    pub id: u16,
    pub name: Option<String>,
    pub data: &'a [u8],
    /// 1-based file-table index (read side only; ignored when writing).
    pub index: u16,
    /// Absolute offset of the body (read side only).
    pub offset: usize,
}

/// A parsed archive plus the ordering facts needed to reproduce it.
#[derive(Debug)]
pub(crate) struct RawArchive<'a> {
    pub header_tail: [u8; 8],
    /// Every resource, in directory order (type table order, then id order).
    pub resources: Vec<RawResource<'a>>,
    /// Order in which the per-type resource tables appear in the directory.
    pub type_order: Vec<Tag>,
}

impl<'a> RawArchive<'a> {
    pub fn get(&self, tag: Tag, id: u16) -> Option<&RawResource<'a>> {
        self.resources.iter().find(|r| r.tag == tag && r.id == id)
    }

    /// Resource keys sorted by body offset (the on-disk order).
    pub fn disk_order(&self) -> Vec<Key> {
        let mut v: Vec<&RawResource> = self.resources.iter().collect();
        v.sort_by_key(|r| (r.offset, r.index));
        v.into_iter().map(|r| (r.tag, r.id)).collect()
    }

    /// Resource keys sorted by file-table index.
    pub fn index_order(&self) -> Vec<Key> {
        let mut v: Vec<&RawResource> = self.resources.iter().collect();
        v.sort_by_key(|r| r.index);
        v.into_iter().map(|r| (r.tag, r.id)).collect()
    }
}

/// Bounds-checked big-endian reader over the whole file.
struct Cursor<'a> {
    data: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn slice(&self, at: usize, len: usize) -> Result<&'a [u8]> {
        at.checked_add(len)
            .and_then(|end| self.data.get(at..end))
            .ok_or_else(|| Error::Format(format!("truncated archive: need {len} bytes at {at:#x}")))
    }
    fn u8(&self, at: usize) -> Result<u8> {
        Ok(self.slice(at, 1)?[0])
    }
    fn u16(&self, at: usize) -> Result<u16> {
        let b = self.slice(at, 2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&self, at: usize) -> Result<u32> {
        let b = self.slice(at, 4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Parse a Mohawk archive held in memory.
pub(crate) fn parse(data: &[u8]) -> Result<RawArchive<'_>> {
    let c = Cursor { data };
    if c.slice(0, 4)? != b"MHWK" || c.slice(8, 4)? != b"RSRC" {
        return Err(Error::Format("not a Mohawk archive (missing MHWK/RSRC)".into()));
    }
    let dir = c.u32(0x14)? as usize;
    let file_table = dir + c.u16(0x18)? as usize;
    let mut header_tail = [0u8; 8];
    if data.len() >= HEADER_LEN {
        header_tail.copy_from_slice(&data[0x1C..HEADER_LEN]);
    }

    // File table: (offset, size) per 1-based index.
    let file_count = c.u32(file_table)? as usize;
    if file_count > 0xFFFF {
        return Err(Error::Format(format!("absurd file count {file_count}")));
    }
    let mut files = Vec::with_capacity(file_count);
    for i in 0..file_count {
        let at = file_table + 4 + i * 10;
        let offset = c.u32(at)? as usize;
        let size = c.u16(at + 4)? as usize
            | (c.u8(at + 6)? as usize) << 16
            | ((c.u8(at + 7)? & 7) as usize) << 24;
        c.slice(offset, size)?;
        files.push((offset, size));
    }

    let name_list = dir + c.u16(dir)? as usize;
    let type_count = c.u16(dir + 2)? as usize;
    let mut resources = Vec::new();
    let mut tables = Vec::with_capacity(type_count);
    for t in 0..type_count {
        let at = dir + 4 + t * 8;
        let tag = Tag(c.slice(at, 4)?.try_into().unwrap());
        let res_table = dir + c.u16(at + 4)? as usize;
        let name_table = dir + c.u16(at + 6)? as usize;
        tables.push((res_table, tag));

        // Names are attached by file index.
        let mut names: Vec<(u16, String)> = Vec::new();
        let name_count = c.u16(name_table)? as usize;
        for n in 0..name_count {
            let off = c.u16(name_table + 2 + n * 4)? as usize;
            let index = c.u16(name_table + 4 + n * 4)?;
            let start = name_list + off;
            let tail = data
                .get(start..)
                .ok_or_else(|| Error::Format(format!("{tag}: name offset {off:#x} out of range")))?;
            let len = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
            names.push((index, tail[..len].iter().map(|&b| b as char).collect()));
        }

        let count = c.u16(res_table)? as usize;
        for n in 0..count {
            let id = c.u16(res_table + 2 + n * 4)?;
            let index = c.u16(res_table + 4 + n * 4)?;
            let &(offset, size) = (index as usize)
                .checked_sub(1)
                .and_then(|i| files.get(i))
                .ok_or_else(|| Error::Format(format!("{tag} {id}: bad file index {index}")))?;
            let name = names.iter().find(|(i, _)| *i == index).map(|(_, s)| s.clone());
            resources.push(RawResource {
                tag,
                id,
                name,
                data: &data[offset..offset + size],
                index,
                offset,
            });
        }
    }
    let mut seen = BTreeSet::new();
    for r in &resources {
        if !seen.insert((r.tag, r.id)) {
            return Err(Error::Format(format!("duplicate resource {} {}", r.tag, r.id)));
        }
    }
    tables.sort_by_key(|&(off, _)| off);
    let type_order = tables.into_iter().map(|(_, tag)| tag).collect();
    Ok(RawArchive {
        header_tail,
        resources,
        type_order,
    })
}

/// Everything the writer needs, with all orders already decided.
pub(crate) struct WritePlan<'a> {
    pub header_tail: [u8; 8],
    /// Resources in file-table index order (index = position + 1).
    pub resources: Vec<RawResource<'a>>,
    /// Positions into `resources`, in the order bodies are laid out on disk.
    pub disk_order: Vec<usize>,
    /// Order of the per-type tables in the directory.
    pub type_order: Vec<Tag>,
}

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn dir_u16(v: usize, what: &str) -> Result<u16> {
    u16::try_from(v).map_err(|_| Error::TooLarge(format!("directory too large ({what} at {v:#x})")))
}

/// Serialise an archive following the recipe of RE_SHELL.md §8.9.
pub(crate) fn write(plan: &WritePlan) -> Result<Vec<u8>> {
    let res = &plan.resources;
    if res.len() > 0xFFFF {
        return Err(Error::TooLarge("more than 65535 resources".into()));
    }
    let total_body: usize = res.iter().map(|r| r.data.len()).sum();
    let mut out = Vec::with_capacity(HEADER_LEN + total_body + 256 + res.len() * 32);
    out.resize(0x1C, 0);
    out.extend_from_slice(&plan.header_tail);

    // Bodies, back to back without alignment.
    let mut offsets = vec![0usize; res.len()];
    for &i in &plan.disk_order {
        let r = &res[i];
        if r.data.len() > MAX_RESOURCE {
            return Err(Error::TooLarge(format!("{} {} is {} bytes", r.tag, r.id, r.data.len())));
        }
        offsets[i] = out.len();
        out.extend_from_slice(r.data);
    }
    let dir = out.len();
    if u32::try_from(dir).is_err() {
        return Err(Error::TooLarge("archive exceeds 4 GiB".into()));
    }

    // Tags present, for the sorted type list.
    let mut tags: Vec<Tag> = res.iter().map(|r| r.tag).collect();
    tags.sort();
    tags.dedup();

    // Lay out per-type tables in `type_order` (tags missing from it go last,
    // sorted) and remember their offsets relative to the directory.
    let mut table_order: Vec<Tag> = plan.type_order.iter().copied().filter(|t| tags.contains(t)).collect();
    table_order.dedup();
    for t in &tags {
        if !table_order.contains(t) {
            table_order.push(*t);
        }
    }
    let mut tables = Vec::new();
    let mut table_offsets: Vec<(Tag, usize, usize)> = Vec::new();
    // Name list: names in file-index order; remember each name's offset.
    let mut name_list = Vec::new();
    let mut name_offsets = vec![None; res.len()];
    for (i, r) in res.iter().enumerate() {
        if let Some(name) = &r.name {
            name_offsets[i] = Some(name_list.len());
            name_list.extend(name.chars().map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' }));
            name_list.push(0);
        }
    }
    let tables_start = 4 + 8 * tags.len();
    for tag in &table_order {
        let mut members: Vec<usize> = (0..res.len()).filter(|&i| res[i].tag == *tag).collect();
        members.sort_by_key(|&i| res[i].id);
        let res_off = tables_start + tables.len();
        push_u16(&mut tables, members.len() as u16);
        for &i in &members {
            push_u16(&mut tables, res[i].id);
            push_u16(&mut tables, (i + 1) as u16);
        }
        let name_off = tables_start + tables.len();
        let named: Vec<usize> = members.iter().copied().filter(|&i| name_offsets[i].is_some()).collect();
        push_u16(&mut tables, named.len() as u16);
        for &i in &named {
            push_u16(&mut tables, dir_u16(name_offsets[i].unwrap(), "name offset")?);
            push_u16(&mut tables, (i + 1) as u16);
        }
        table_offsets.push((*tag, res_off, name_off));
    }
    let name_list_off = tables_start + tables.len();
    let file_table_off = name_list_off + name_list.len();

    push_u16(&mut out, dir_u16(name_list_off, "name list")?);
    push_u16(&mut out, tags.len() as u16);
    for tag in &tags {
        let &(_, r, n) = table_offsets.iter().find(|(t, _, _)| t == tag).unwrap();
        out.extend_from_slice(&tag.0);
        push_u16(&mut out, dir_u16(r, "resource table")?);
        push_u16(&mut out, dir_u16(n, "name table")?);
    }
    out.extend_from_slice(&tables);
    out.extend_from_slice(&name_list);
    push_u32(&mut out, res.len() as u32);
    for (i, r) in res.iter().enumerate() {
        let size = r.data.len();
        push_u32(&mut out, offsets[i] as u32);
        push_u16(&mut out, size as u16);
        out.push((size >> 16) as u8);
        out.push(((size >> 24) & 7) as u8);
        push_u16(&mut out, 0);
    }

    let total = out.len();
    let total32 = u32::try_from(total).map_err(|_| Error::TooLarge("archive exceeds 4 GiB".into()))?;
    let mut header = Vec::with_capacity(0x1C);
    header.extend_from_slice(b"MHWK");
    push_u32(&mut header, total32 - 8);
    header.extend_from_slice(b"RSRC");
    push_u16(&mut header, 0x0100);
    push_u16(&mut header, 0x0001);
    push_u32(&mut header, total32);
    push_u32(&mut header, dir as u32);
    push_u16(&mut header, dir_u16(file_table_off, "file table")?);
    push_u16(&mut header, dir_u16(4 + 10 * res.len(), "file table size")?);
    out[..0x1C].copy_from_slice(&header);
    Ok(out)
}
