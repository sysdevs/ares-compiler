//! Collects each script's bytes under its id (`-1` for an unmapped script) and
//! lays out the packs: the jag `script.dat`/`script.idx`, the js5 pack and
//! one `bin/<id>` file per script. A repeated id takes the last buffer.

use std::collections::BTreeMap;
use std::io::Write;

use indexmap::IndexMap;

use super::packet::ByteWriter;
use crate::FatalError;

type R<T = ()> = Result<T, FatalError>;

/// The written outputs of one compilation.
#[derive(Debug, Default, PartialEq)]
pub struct Packs {
    pub script_dat: Vec<u8>,
    pub script_idx: Vec<u8>,
    pub js5: Vec<u8>,
    /// `bin/<id>` files, in write order (a repeated id overwrites the file).
    pub bin: IndexMap<String, Vec<u8>>,
}

/// Builds every pack from `(id, bytes)` in write order.
pub fn pack(scripts: &[(i32, Vec<u8>)]) -> R<Packs> {
    let mut by_id: BTreeMap<i32, &[u8]> = BTreeMap::new();
    let mut bin = IndexMap::new();
    for (id, data) in scripts {
        by_id.insert(*id, data);
        bin.insert(id.to_string(), data.clone());
    }
    let sorted: Vec<(i32, &[u8])> = by_id.into_iter().collect();
    let (script_dat, script_idx) = jag(&sorted)?;
    Ok(Packs { script_dat, script_idx, js5: js5(&sorted)?, bin })
}

/// A length as a 4-byte field.
fn len(n: usize) -> R<i32> {
    i32::try_from(n).map_err(|_| FatalError(format!("Length {n} does not fit in i32.")))
}

/// `count, version 27, data…` / `count, size…`, counting ids `0..=last`,
/// gaps of size 0 (negative ids are never written).
fn jag(sorted: &[(i32, &[u8])]) -> R<(Vec<u8>, Vec<u8>)> {
    let last = sorted.last().map_or(0, |&(id, _)| id);
    let count = last.checked_add(1).ok_or_else(|| FatalError(format!("Script id {last} is too large.")))?;
    let (mut dat, mut idx) = (Vec::new(), Vec::new());
    dat.extend(count.to_be_bytes());
    idx.extend(count.to_be_bytes());
    dat.extend(27i32.to_be_bytes());
    let lookup: BTreeMap<i32, &[u8]> = sorted.iter().copied().collect();
    for i in 0..=last {
        match lookup.get(&i) {
            None => idx.extend(0i32.to_be_bytes()),
            Some(data) => {
                idx.extend(len(data.len())?.to_be_bytes());
                dat.extend_from_slice(data);
            }
        }
    }
    Ok((dat, idx))
}

/// Gzip-packed index, stored groups, group lengths.
fn js5(sorted: &[(i32, &[u8])]) -> R<Vec<u8>> {
    let groups: Vec<(i32, Vec<u8>)> = sorted.iter().map(|&(id, data)| Ok((id, pack_group(data, 0)?))).collect::<R<_>>()?;
    let mut index = ByteWriter::default();
    index.p1(7);
    index.p4(1);
    index.p1(0);
    index.p_smart2or4(len(groups.len())?);
    let mut previous = 0;
    for (id, _) in &groups {
        index.p_smart2or4(id.wrapping_sub(previous));
        previous = *id;
    }
    for (_, packed) in &groups {
        index.p4(crc32fast::hash(packed) as i32);
    }
    for _ in &groups {
        index.p4(1);
    }
    for _ in &groups {
        index.p_smart2or4(1);
    }
    for _ in &groups {
        index.p_smart2or4(0);
    }
    let mut out = pack_group(&index.into_bytes(), 2)?;
    for (_, packed) in &groups {
        out.extend(packed);
    }
    for (_, packed) in &groups {
        out.extend(len(packed.len())?.to_be_bytes());
    }
    Ok(out)
}

/// Type 0 stored (`p1 0, p4 len, data`) or type 2 gzip
/// (`p1 2, p4 compressed len, p4 len, gzip` with the gzip OS byte zeroed).
/// The deflate stream is stock zlib's (`flate2`).
fn pack_group(src: &[u8], compression: u8) -> R<Vec<u8>> {
    let mut w = ByteWriter::default();
    w.p1(compression);
    if compression == 0 {
        w.p4(len(src.len())?);
        w.pdata(src);
    } else {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(src).expect("in-memory gzip");
        let mut compressed = e.finish().expect("in-memory gzip");
        compressed[9] = 0;
        w.p4(len(compressed.len())?);
        w.p4(len(src.len())?);
        w.pdata(&compressed);
    }
    Ok(w.into_bytes())
}
