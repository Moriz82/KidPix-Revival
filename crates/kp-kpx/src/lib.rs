//! Reader and byte-exact writer for Kid Pix Deluxe 3 `.kpx` picture files.
//!
//! A `.kpx` file is a big-endian Mohawk archive (see `docs/RE_SHELL.md` §8):
//!
//! | resource  | meaning                                                   |
//! |-----------|-----------------------------------------------------------|
//! | `tBMP 1`  | page raster (paint + background), JPEG, 600×452           |
//! | `tBMP 2`  | composite (raster with all objects flattened)             |
//! | `tTHM 1`  | 96×72 thumbnail of the composite                          |
//! | `tWAV 1`  | page sound (Mohawk WAVE, IMA ADPCM)                       |
//! | `tSWF n`  | sticker / animation object (complete uncompressed SWF)    |
//! | `tTXT n`  | text box object                                           |
//! | `tMBM n`  | floating bitmap object                                    |
//! | `tMOV n`  | QuickTime movie object                                    |
//! | `tRem 1`  | catalogue listing the above in z-order, with placements   |
//!
//! [`read`] turns a file into a [`Page`]; [`write`] turns a page back into
//! bytes. A page that was read and not modified is written back
//! byte-identically: the original JPEG bytes are reused while the pixels are
//! unchanged, and the archive layout details (resource order, names, padding)
//! are remembered in [`Page::layout`]. New or edited pages are written the way
//! the original program writes them.

mod catalogue;
mod container;
pub mod raster;
pub mod sound;
pub mod text;

pub use mohawk::{Image, Tag};
pub use raster::Raster;
pub use sound::PageSound;
pub use text::TextBox;

use container::{Key, RawResource};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

/// Page raster width in pixels.
pub const PAGE_WIDTH: u32 = 600;
/// Page raster height in pixels.
pub const PAGE_HEIGHT: u32 = 452;
/// Thumbnail width in pixels.
pub const THUMB_WIDTH: u32 = 96;
/// Thumbnail height in pixels.
pub const THUMB_HEIGHT: u32 = 72;

/// Bitmap resources (`tBMP 1` raster, `tBMP 2` composite).
pub const T_BMP: Tag = Tag(*b"tBMP");
/// Thumbnail resource.
pub const T_THM: Tag = Tag(*b"tTHM");
/// Object catalogue resource.
pub const T_REM: Tag = Tag(*b"tRem");
/// Sound resource.
pub const T_WAV: Tag = Tag(*b"tWAV");
/// Sticker / animation object.
pub const T_SWF: Tag = Tag(*b"tSWF");
/// Text box object.
pub const T_TXT: Tag = Tag(*b"tTXT");
/// Floating bitmap object.
pub const T_MBM: Tag = Tag(*b"tMBM");
/// QuickTime movie object.
pub const T_MOV: Tag = Tag(*b"tMOV");

/// The 8 bytes at archive offset 0x1C in every known file.
const DEFAULT_HEADER_TAIL: [u8; 8] = [0, 0, 0xFF, 0xFD, 0, 0, 0, 0];

/// Errors from reading or writing `.kpx` files.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The data is not a well-formed `.kpx` file.
    Format(String),
    /// A resource failed to decode in the shared Mohawk decoders.
    Mohawk(mohawk::Error),
    /// JPEG encoding failed.
    Image(String),
    /// The page cannot be represented in the format (sizes, counts).
    TooLarge(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Format(s) => write!(f, "bad .kpx data: {s}"),
            Error::Mohawk(e) => write!(f, "{e}"),
            Error::Image(s) => write!(f, "image error: {s}"),
            Error::TooLarge(s) => write!(f, "cannot store page: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Rectangle in page pixels. Stored on disk in Mac order (top, left, bottom, right);
/// `bottom`/`right` are exclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub top: i16,
    pub left: i16,
    pub bottom: i16,
    pub right: i16,
}

impl Rect {
    pub const fn new(top: i16, left: i16, bottom: i16, right: i16) -> Rect {
        Rect { top, left, bottom, right }
    }

    /// Build from position and size.
    pub const fn from_xywh(x: i16, y: i16, w: i16, h: i16) -> Rect {
        Rect::new(y, x, y + h, x + w)
    }

    /// Rect covering an image of the given size at the origin.
    pub fn of_size(width: u32, height: u32) -> Rect {
        Rect::new(0, 0, height as i16, width as i16)
    }

    pub const fn width(&self) -> i16 {
        self.right - self.left
    }

    pub const fn height(&self) -> i16 {
        self.bottom - self.top
    }

    pub const fn is_empty(&self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }

    pub(crate) fn from_be(b: &[u8]) -> Rect {
        let v = |i: usize| i16::from_be_bytes([b[i], b[i + 1]]);
        Rect::new(v(0), v(2), v(4), v(6))
    }

    pub(crate) fn to_be(self) -> [u8; 8] {
        let mut o = [0u8; 8];
        for (i, v) in [self.top, self.left, self.bottom, self.right].into_iter().enumerate() {
            o[i * 2..i * 2 + 2].copy_from_slice(&v.to_be_bytes());
        }
        o
    }
}

/// Decoded form of a sticker's catalogue `extra` word (RE_SHELL.md §8.5):
/// bits 31–21 width, 20–10 height, 9–8 a 2-bit state, 7–0 a byte that is
/// probably the current frame/phase. The packing is lossless both ways.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StickerPlacement {
    /// Displayed width (11 bits); 0 together with `height == 0` means natural size.
    pub width: u16,
    /// Displayed height (11 bits).
    pub height: u16,
    /// 2-bit state (meaning not yet confirmed).
    pub state: u8,
    /// Low byte, believed to be the truncated current frame / phase.
    pub phase: u8,
}

impl StickerPlacement {
    pub const fn from_extra(extra: u32) -> StickerPlacement {
        StickerPlacement {
            width: ((extra >> 21) & 0x7FF) as u16,
            height: ((extra >> 10) & 0x7FF) as u16,
            state: ((extra >> 8) & 3) as u8,
            phase: extra as u8,
        }
    }

    pub const fn to_extra(self) -> u32 {
        (self.width as u32 & 0x7FF) << 21
            | (self.height as u32 & 0x7FF) << 10
            | (self.state as u32 & 3) << 8
            | self.phase as u32
    }

    /// Displayed size, or `None` when the sticker uses its natural size.
    pub const fn size(self) -> Option<(u16, u16)> {
        if self.width != 0 || self.height != 0 {
            Some((self.width, self.height))
        } else {
            None
        }
    }
}

/// Payload of a page object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    /// `tSWF`: a complete uncompressed SWF movie (colour fills already baked in).
    Sticker(Vec<u8>),
    /// `tTXT`: a text box.
    Text(TextBox),
    /// `tMBM`: floating bitmap, kept raw.
    Bitmap(Vec<u8>),
    /// `tMOV`: QuickTime movie data, kept raw.
    Movie(Vec<u8>),
    /// Any other catalogued resource (including `tTXT` blobs that fail to parse).
    Other { tag: Tag, data: Vec<u8> },
}

impl ObjectKind {
    /// Resource type this payload is stored as.
    pub fn tag(&self) -> Tag {
        match self {
            ObjectKind::Sticker(_) => T_SWF,
            ObjectKind::Text(_) => T_TXT,
            ObjectKind::Bitmap(_) => T_MBM,
            ObjectKind::Movie(_) => T_MOV,
            ObjectKind::Other { tag, .. } => *tag,
        }
    }

    /// Resource bytes.
    pub fn to_bytes(&self) -> Cow<'_, [u8]> {
        match self {
            ObjectKind::Sticker(d) | ObjectKind::Bitmap(d) | ObjectKind::Movie(d) => Cow::Borrowed(d),
            ObjectKind::Other { data, .. } => Cow::Borrowed(data),
            ObjectKind::Text(t) => Cow::Owned(t.to_bytes()),
        }
    }

    fn from_resource(tag: Tag, data: &[u8]) -> ObjectKind {
        match tag {
            T_SWF => ObjectKind::Sticker(data.to_vec()),
            T_MBM => ObjectKind::Bitmap(data.to_vec()),
            T_MOV => ObjectKind::Movie(data.to_vec()),
            T_TXT => match TextBox::parse(data) {
                // Only accept the parse if it reproduces the blob exactly.
                Ok(t) if t.to_bytes() == data => ObjectKind::Text(t),
                _ => ObjectKind::Other { tag, data: data.to_vec() },
            },
            _ => ObjectKind::Other { tag, data: data.to_vec() },
        }
    }
}

/// A catalogued page object (everything except raster, composite, thumbnail, sound).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    /// Resource id (unique per type).
    pub id: u16,
    /// Displayed bounds on the page (catalogue rect).
    pub rect: Rect,
    /// Raw catalogue `extra` word; for stickers see [`StickerPlacement`].
    pub extra: u32,
    pub kind: ObjectKind,
}

impl Object {
    pub fn tag(&self) -> Tag {
        self.kind.tag()
    }

    /// A sticker object.
    pub fn sticker(id: u16, rect: Rect, swf: Vec<u8>, placement: StickerPlacement) -> Object {
        Object {
            id,
            rect,
            extra: placement.to_extra(),
            kind: ObjectKind::Sticker(swf),
        }
    }

    /// A text object; the catalogue rect is taken from the box.
    pub fn text(id: u16, text: TextBox) -> Object {
        Object {
            id,
            rect: text.rect,
            extra: 0,
            kind: ObjectKind::Text(text),
        }
    }

    /// The `extra` word decoded as a sticker placement.
    pub fn sticker_placement(&self) -> StickerPlacement {
        StickerPlacement::from_extra(self.extra)
    }

    /// SWF bytes if this is a sticker.
    pub fn as_sticker(&self) -> Option<&[u8]> {
        match &self.kind {
            ObjectKind::Sticker(d) => Some(d),
            _ => None,
        }
    }

    /// Text box if this is a text object.
    pub fn as_text(&self) -> Option<&TextBox> {
        match &self.kind {
            ObjectKind::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// A resource stored in the archive but not listed in the catalogue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub tag: Tag,
    pub id: u16,
    pub data: Vec<u8>,
}

/// Which page part a catalogue position refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Background,
    Composite,
    Thumbnail,
    Sound,
    /// The next entry of [`Page::objects`].
    Object,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SlotEntry {
    slot: Slot,
    /// Catalogue rect/extra for rasters (sound and objects carry their own).
    rect: Rect,
    extra: u32,
}

/// Archive layout details remembered from a file so it can be rewritten
/// byte-identically. A default layout yields the original program's choices.
///
/// Each remembered order is only used while it still describes the page's
/// current set of resources; adding or removing parts falls back to the
/// default rules for that aspect.
#[derive(Clone, Debug)]
pub struct Layout {
    header_tail: [u8; 8],
    rem_version: i16,
    rem_reserved: u32,
    rem_slack: Vec<u8>,
    rem_legacy: bool,
    catalogue: Vec<SlotEntry>,
    disk_order: Vec<Key>,
    index_order: Vec<Key>,
    type_order: Vec<Tag>,
    names: Vec<(Key, Option<String>)>,
}

impl Default for Layout {
    fn default() -> Layout {
        Layout {
            header_tail: DEFAULT_HEADER_TAIL,
            rem_version: catalogue::REM_VERSION,
            rem_reserved: 0,
            rem_slack: Vec::new(),
            rem_legacy: false,
            catalogue: Vec::new(),
            disk_order: Vec::new(),
            index_order: Vec::new(),
            type_order: Vec::new(),
            names: Vec::new(),
        }
    }
}

impl Layout {
    /// True if the page was read from an old-format (18-byte entry) catalogue;
    /// such pages are written in the current format.
    pub fn is_legacy_catalogue(&self) -> bool {
        self.rem_legacy
    }

    /// Forget everything remembered from the source file.
    pub fn reset(&mut self) {
        *self = Layout::default();
    }
}

/// A Kid Pix picture.
#[derive(Clone, Debug)]
pub struct Page {
    /// `tBMP 1`: the paint layer including the background (600×452).
    pub background: Raster,
    /// `tBMP 2`: the raster with every object flattened onto it.
    pub composite: Option<Raster>,
    /// `tTHM 1`: thumbnail of the composite (96×72).
    pub thumbnail: Option<Raster>,
    /// `tWAV 1`: the page sound.
    pub sound: Option<PageSound>,
    /// Page objects in z-order (first is drawn first / bottom-most).
    pub objects: Vec<Object>,
    /// Resources present in the file but not catalogued (kept for round-trips).
    pub extra_resources: Vec<Resource>,
    /// Layout details for exact rewrites; `Layout::default()` for new pages.
    pub layout: Layout,
}

impl Page {
    /// A new page from a background raster. Composite and thumbnail are
    /// derived from it (call [`Page::set_composite`] once objects are added).
    pub fn new(background: Image) -> Page {
        let thumb = raster::downscale(&background, THUMB_WIDTH, THUMB_HEIGHT);
        Page {
            composite: Some(Raster::new(background.clone())),
            thumbnail: Some(Raster::new(thumb)),
            background: Raster::new(background),
            sound: None,
            objects: Vec::new(),
            extra_resources: Vec::new(),
            layout: Layout::default(),
        }
    }

    /// A new all-white 600×452 page.
    pub fn new_blank() -> Page {
        Page::new(Image::filled(PAGE_WIDTH, PAGE_HEIGHT, 0xFFFF_FFFF))
    }

    /// Replace the composite (raster with objects flattened) and regenerate the thumbnail.
    pub fn set_composite(&mut self, composite: Image) {
        self.composite = Some(Raster::new(composite));
        self.refresh_thumbnail();
    }

    /// Regenerate the thumbnail from the composite (or the raster if there is none).
    pub fn refresh_thumbnail(&mut self) {
        let src = self.composite.as_ref().unwrap_or(&self.background);
        let thumb = raster::downscale(&src.image, THUMB_WIDTH, THUMB_HEIGHT);
        match &mut self.thumbnail {
            Some(t) if t.image == thumb => {}
            _ => self.thumbnail = Some(Raster::new(thumb)),
        }
    }

    /// Next free id for a resource type: highest id in use plus one.
    pub fn next_id(&self, tag: Tag) -> u16 {
        let used = self
            .objects
            .iter()
            .filter(|o| o.tag() == tag)
            .map(|o| o.id)
            .chain(self.extra_resources.iter().filter(|r| r.tag == tag).map(|r| r.id));
        used.max().map_or(1, |m| m.saturating_add(1))
    }

    /// Append an object on top of the others, assigning the next free id.
    pub fn add_object(&mut self, kind: ObjectKind, rect: Rect, extra: u32) -> u16 {
        let id = self.next_id(kind.tag());
        self.objects.push(Object { id, rect, extra, kind });
        id
    }

    /// Sticker objects in z-order.
    pub fn stickers(&self) -> impl Iterator<Item = &Object> {
        self.objects.iter().filter(|o| matches!(o.kind, ObjectKind::Sticker(_)))
    }

    /// Text boxes in z-order, with their object records.
    pub fn texts(&self) -> impl Iterator<Item = (&Object, &TextBox)> {
        self.objects.iter().filter_map(|o| o.as_text().map(|t| (o, t)))
    }

    /// Encode any modified rasters now so later writes can reuse the result.
    pub fn commit_rasters(&mut self) -> Result<()> {
        self.background.commit()?;
        if let Some(r) = &mut self.composite {
            r.commit()?;
        }
        if let Some(r) = &mut self.thumbnail {
            r.commit()?;
        }
        Ok(())
    }

    fn present_fixed(&self) -> Vec<Slot> {
        let mut v = vec![Slot::Background];
        if self.composite.is_some() {
            v.push(Slot::Composite);
        }
        if self.thumbnail.is_some() {
            v.push(Slot::Thumbnail);
        }
        if self.sound.is_some() {
            v.push(Slot::Sound);
        }
        v
    }

    /// Catalogue order to write: the remembered one if it still fits, else the default
    /// (raster, sound, objects, thumbnail, composite).
    fn catalogue_slots(&self) -> Vec<SlotEntry> {
        let remembered = &self.layout.catalogue;
        let mut fixed: Vec<Slot> = remembered.iter().map(|e| e.slot).filter(|s| *s != Slot::Object).collect();
        fixed.sort();
        let mut present = self.present_fixed();
        present.sort();
        let n_obj = remembered.iter().filter(|e| e.slot == Slot::Object).count();
        if !remembered.is_empty() && fixed == present && n_obj == self.objects.len() {
            return remembered.clone();
        }
        let raster = |slot: Slot, r: &Raster| SlotEntry {
            slot,
            rect: Rect::of_size(r.image.width, r.image.height),
            extra: 0,
        };
        let mut v = vec![raster(Slot::Background, &self.background)];
        if self.sound.is_some() {
            v.push(SlotEntry {
                slot: Slot::Sound,
                rect: Rect::default(),
                extra: 0,
            });
        }
        v.extend(self.objects.iter().map(|_| SlotEntry {
            slot: Slot::Object,
            rect: Rect::default(),
            extra: 0,
        }));
        if let Some(t) = &self.thumbnail {
            v.push(raster(Slot::Thumbnail, t));
        }
        if let Some(c) = &self.composite {
            v.push(raster(Slot::Composite, c));
        }
        v
    }
}

/// Default resource name: decimal FourCC followed by decimal id. Sounds and
/// movies are stored without a name.
fn default_name(tag: Tag, id: u16) -> Option<String> {
    match tag {
        T_WAV | T_MOV => None,
        _ => Some(format!("{}{}", u32::from_be_bytes(tag.0), id)),
    }
}

/// Parse a `.kpx` file held in memory.
pub fn read(bytes: &[u8]) -> Result<Page> {
    let arc = container::parse(bytes)?;
    let rem_res = arc
        .get(T_REM, 1)
        .ok_or_else(|| Error::Format("missing tRem 1 catalogue".into()))?;
    let cat = catalogue::parse(rem_res.data)?;
    let fetch = |tag: Tag, id: u16| arc.get(tag, id).map(|r| r.data);

    let background = Raster::decode(fetch(T_BMP, 1).ok_or_else(|| Error::Format("missing tBMP 1 raster".into()))?)?;
    let composite = fetch(T_BMP, 2).map(Raster::decode).transpose()?;
    let thumbnail = fetch(T_THM, 1).map(Raster::decode).transpose()?;
    let mut sound = fetch(T_WAV, 1).map(|d| PageSound::from_bytes(d.to_vec()));

    let mut listed: BTreeSet<Key> = BTreeSet::new();
    let mut slots = Vec::with_capacity(cat.entries.len());
    let mut objects = Vec::new();
    for e in &cat.entries {
        let key = (e.tag, e.id);
        if !listed.insert(key) {
            return Err(Error::Format(format!("catalogue lists {} {} twice", e.tag, e.id)));
        }
        let slot = match key {
            (T_BMP, 1) => Slot::Background,
            (T_BMP, 2) => Slot::Composite,
            (T_THM, 1) => Slot::Thumbnail,
            (T_WAV, 1) => Slot::Sound,
            (T_REM, 1) => return Err(Error::Format("catalogue lists itself".into())),
            _ => Slot::Object,
        };
        match slot {
            Slot::Sound => {
                let s = sound.as_mut().ok_or_else(|| Error::Format("catalogue lists missing tWAV 1".into()))?;
                s.rect = e.rect;
                s.extra = e.extra;
            }
            Slot::Object => {
                let data = fetch(e.tag, e.id)
                    .ok_or_else(|| Error::Format(format!("catalogue lists missing {} {}", e.tag, e.id)))?;
                objects.push(Object {
                    id: e.id,
                    rect: e.rect,
                    extra: e.extra,
                    kind: ObjectKind::from_resource(e.tag, data),
                });
            }
            _ => {}
        }
        slots.push(SlotEntry {
            slot,
            rect: e.rect,
            extra: e.extra,
        });
    }

    let fixed: [Key; 5] = [(T_REM, 1), (T_BMP, 1), (T_BMP, 2), (T_THM, 1), (T_WAV, 1)];
    let extra_resources = arc
        .resources
        .iter()
        .filter(|r| !listed.contains(&(r.tag, r.id)) && !fixed.contains(&(r.tag, r.id)))
        .map(|r| Resource {
            tag: r.tag,
            id: r.id,
            data: r.data.to_vec(),
        })
        .collect();

    let layout = Layout {
        header_tail: arc.header_tail,
        rem_version: if cat.legacy { catalogue::REM_VERSION } else { cat.version },
        rem_reserved: cat.reserved,
        rem_slack: cat.slack,
        rem_legacy: cat.legacy,
        catalogue: slots,
        disk_order: arc.disk_order(),
        index_order: arc.index_order(),
        type_order: arc.type_order.clone(),
        names: arc.resources.iter().map(|r| ((r.tag, r.id), r.name.clone())).collect(),
    };
    Ok(Page {
        background,
        composite,
        thumbnail,
        sound,
        objects,
        extra_resources,
        layout,
    })
}

/// Read a `.kpx` file from disk.
pub fn read_file(path: impl AsRef<Path>) -> Result<Page> {
    read(&std::fs::read(path)?)
}

/// Serialise a page to `.kpx` bytes.
///
/// Fails if two parts share a resource type and id, or a part is too large
/// for the format.
pub fn write(page: &Page) -> Result<Vec<u8>> {
    let layout = &page.layout;
    let bg = page.background.to_resource()?;
    let composite = page.composite.as_ref().map(|r| r.to_resource()).transpose()?;
    let thumbnail = page.thumbnail.as_ref().map(|r| r.to_resource()).transpose()?;
    let object_bytes: Vec<Cow<[u8]>> = page.objects.iter().map(|o| o.kind.to_bytes()).collect();

    // Catalogue, and the resource bodies in catalogue order.
    let mut entries = Vec::new();
    let mut bodies: Vec<(Key, &[u8])> = Vec::new();
    let mut next_object = 0;
    for s in page.catalogue_slots() {
        let (tag, id, rect, extra, data): (Tag, u16, Rect, u32, &[u8]) = match s.slot {
            Slot::Background => (T_BMP, 1, s.rect, s.extra, &bg),
            Slot::Composite => (T_BMP, 2, s.rect, s.extra, composite.as_deref().unwrap()),
            Slot::Thumbnail => (T_THM, 1, s.rect, s.extra, thumbnail.as_deref().unwrap()),
            Slot::Sound => {
                let snd = page.sound.as_ref().unwrap();
                (T_WAV, 1, snd.rect, snd.extra, &snd.data)
            }
            Slot::Object => {
                let o = &page.objects[next_object];
                let data: &[u8] = &object_bytes[next_object];
                next_object += 1;
                (o.tag(), o.id, o.rect, o.extra, data)
            }
        };
        entries.push(catalogue::RemEntry { tag, id, rect, extra });
        bodies.push(((tag, id), data));
    }
    for r in &page.extra_resources {
        bodies.push(((r.tag, r.id), &r.data));
    }
    let rem = catalogue::write(&entries, layout.rem_version, layout.rem_reserved, &layout.rem_slack);
    bodies.push(((T_REM, 1), &rem));

    let keys: BTreeSet<Key> = bodies.iter().map(|b| b.0).collect();
    if keys.len() != bodies.len() {
        return Err(Error::Format("two page parts share a resource type and id".into()));
    }
    let fits = |order: &[Key]| order.len() == keys.len() && order.iter().copied().collect::<BTreeSet<_>>() == keys;

    // File-table order: catalogue first in the default, preceded by tRem.
    let default_index: Vec<Key> = std::iter::once((T_REM, 1))
        .chain(bodies.iter().map(|b| b.0).filter(|k| *k != (T_REM, 1)))
        .collect();
    let index_order = if fits(&layout.index_order) { layout.index_order.clone() } else { default_index };
    // Disk order: catalogue bodies, extras, tRem last (= `bodies` order).
    let default_disk: Vec<Key> = bodies.iter().map(|b| b.0).collect();
    let disk_order = if fits(&layout.disk_order) { layout.disk_order.clone() } else { default_disk };

    let body_of = |k: &Key| bodies.iter().find(|b| b.0 == *k).unwrap().1;
    let name_of = |k: &Key| match layout.names.iter().find(|(nk, _)| nk == k) {
        Some((_, n)) => n.clone(),
        None => default_name(k.0, k.1),
    };
    let resources: Vec<RawResource> = index_order
        .iter()
        .map(|k| RawResource {
            tag: k.0,
            id: k.1,
            name: name_of(k),
            data: body_of(k),
            index: 0,
            offset: 0,
        })
        .collect();
    let disk_positions = disk_order
        .iter()
        .map(|k| index_order.iter().position(|i| i == k).unwrap())
        .collect();

    // Per-type tables: remembered order if it names exactly the present types,
    // else ordered by each type's first file-table index.
    let mut tags: Vec<Tag> = index_order.iter().map(|k| k.0).collect();
    let mut seen = BTreeSet::new();
    tags.retain(|t| seen.insert(*t));
    let remembered_tags: BTreeSet<Tag> = layout.type_order.iter().copied().collect();
    let type_order = if layout.type_order.len() == tags.len() && remembered_tags == seen {
        layout.type_order.clone()
    } else {
        tags
    };

    container::write(&container::WritePlan {
        header_tail: layout.header_tail,
        resources,
        disk_order: disk_positions,
        type_order,
    })
}

/// Write a page to disk.
pub fn write_file(page: &Page, path: impl AsRef<Path>) -> Result<()> {
    std::fs::write(path, write(page)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticker_placement_round_trips() {
        for extra in [0u32, 0xFFFF_FFFF, 0x1234_5678, 0x0C80_4A07] {
            assert_eq!(StickerPlacement::from_extra(extra).to_extra(), extra);
        }
        let p = StickerPlacement {
            width: 120,
            height: 80,
            state: 2,
            phase: 7,
        };
        assert_eq!(StickerPlacement::from_extra(p.to_extra()), p);
    }

    #[test]
    fn rect_bytes_round_trip() {
        let r = Rect::new(-3, 10, 452, 600);
        assert_eq!(Rect::from_be(&r.to_be()), r);
        assert_eq!(Rect::from_xywh(10, 20, 30, 40), Rect::new(20, 10, 60, 40));
    }

    #[test]
    fn text_box_round_trips_every_byte() {
        let mut t = TextBox::new(Rect::new(1, 2, 3, 4), "", "Comic Sans MS", 24);
        t.text = text::decode_ansi(&(0u8..=255).collect::<Vec<_>>());
        t.style = 0xA5A5;
        t.justification = 1;
        t.set_rgb(10, 20, 30);
        let b = t.to_bytes();
        assert_eq!(b.len(), text::TEXT_HEADER_LEN + 256);
        assert_eq!(TextBox::parse(&b).unwrap(), t);
        assert_eq!(t.face_name(), "Comic Sans MS");
        assert_eq!(t.rgb(), Some((10, 20, 30)));
    }

    #[test]
    fn adpcm_encoder_decodes_close_to_input() {
        let pcm: Vec<i16> = (0..4001).map(|i| ((i as f32 * 0.05).sin() * 12000.0) as i16).collect();
        let snd = PageSound::encode(sound::RECORD_RATE, &pcm);
        let dec = snd.decode().unwrap();
        assert_eq!(dec.sample_rate, 22050);
        assert_eq!(dec.channels, 1);
        assert_eq!(dec.samples.len(), pcm.len());
        let err: f64 = pcm
            .iter()
            .zip(&dec.samples)
            .skip(50)
            .map(|(a, b)| (*a as f64 - *b as f64).abs())
            .sum::<f64>()
            / pcm.len() as f64;
        assert!(err < 300.0, "mean abs error {err}");
    }

    #[test]
    fn downscale_averages() {
        let mut img = Image::filled(4, 2, 0xFF00_0000);
        for x in 0..2 {
            for y in 0..2 {
                img.put(x, y, 0xFFFF_FFFF);
            }
        }
        let s = raster::downscale(&img, 2, 1);
        assert_eq!(s.pixels, vec![0xFFFF_FFFF, 0xFF00_0000]);
        let s = raster::downscale(&img, 1, 1);
        assert_eq!(s.pixels, vec![0xFF80_8080]);
    }
}
