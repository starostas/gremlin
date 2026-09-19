//! Bind a decoded symbol to the same tables and hash lookup used by the ELF loader.
use std::collections::BTreeMap;
fn range(data: &[u8], offset: u64, size: u64) -> Result<&[u8], String> {
    let end = offset.checked_add(size).ok_or("ELF range overflow")?;
    data.get(
        usize::try_from(offset).map_err(|_| "ELF offset overflow")?
            ..usize::try_from(end).map_err(|_| "ELF offset overflow")?,
    )
    .ok_or("ELF range outside file".into())
}
fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes(b.try_into().unwrap())
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().unwrap())
}
fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b.try_into().unwrap())
}
fn loaded(data: &[u8], address: u64, size: u64) -> Result<&[u8], String> {
    let phoff = u64le(&data[32..40]);
    let count = u16le(&data[56..58]) as u64;
    let end = address.checked_add(size).ok_or("mapped range overflow")?;
    let mut result = None;
    let mut page_delta = None;
    for n in 0..count {
        let p = range(
            data,
            phoff.checked_add(n * 56).ok_or("program header overflow")?,
            56,
        )?;
        if u32le(&p[..4]) != 1 {
            continue;
        }
        let offset = u64le(&p[8..16]);
        let base = u64le(&p[16..24]);
        let filesz = u64le(&p[32..40]);
        let memsz = u64le(&p[40..48]);
        if filesz != memsz
            || offset % 4096 != base % 4096
            || u64le(&p[48..56]) != 4096
            || u32le(&p[4..8]) & 4 == 0
        {
            return Err("unsupported ELF load alignment or file size".into());
        }
        range(data, offset, filesz)?;
        let segment_end = base.checked_add(filesz).ok_or("segment overflow")?;
        if filesz != 0
            && (base & !4095) < (end.checked_add(4095).ok_or("page overflow")? & !4095)
            && (segment_end.checked_add(4095).ok_or("page overflow")? & !4095) > (address & !4095)
        {
            let delta = offset as i128 - base as i128;
            if page_delta.is_some_and(|old| old != delta) {
                return Err("metadata page has conflicting load mappings".into());
            }
            page_delta = Some(delta);
        }
        if address >= base && end <= base.checked_add(filesz).ok_or("segment overflow")? {
            if result.is_some() {
                return Err("ambiguous metadata load mapping".into());
            }
            result = Some(range(
                data,
                offset
                    .checked_add(address - base)
                    .ok_or("load offset overflow")?,
                size,
            )?);
        }
    }
    result.ok_or("metadata is outside file-backed load mapping".into())
}
struct Section<'a> {
    header: &'a [u8],
    data: &'a [u8],
}
impl Section<'_> {
    fn address(&self) -> u64 {
        u64le(&self.header[16..24])
    }
}
pub fn validate(data: &[u8], name: &str, entry: u64, size: u64) -> Result<(), String> {
    if data.len() < 64 || u16le(&data[54..56]) != 56 || u16le(&data[58..60]) != 64 {
        return Err("unsupported ELF header sizes".into());
    }
    if data[6] != 1 || u32le(&data[20..24]) != 1 || u16le(&data[52..54]) != 64 {
        return Err("unsupported ELF version/header".into());
    }
    let phoff = u64le(&data[32..40]);
    let phnum = u16le(&data[56..58]) as u64;
    let mut dynamic = None;
    for n in 0..phnum {
        let p = range(
            data,
            phoff.checked_add(n * 56).ok_or("program header overflow")?,
            56,
        )?;
        if u32le(&p[..4]) == 2 {
            if dynamic.is_some() {
                return Err("multiple dynamic segments unsupported".into());
            }
            let bytes = range(data, u64le(&p[8..16]), u64le(&p[32..40]))?;
            if loaded(data, u64le(&p[16..24]), bytes.len() as u64)? != bytes {
                return Err("dynamic segment differs from loaded bytes".into());
            }
            dynamic = Some(bytes);
        }
    }
    let mut tags = BTreeMap::new();
    let mut terminated = false;
    for row in dynamic.ok_or("missing dynamic segment")?.chunks_exact(16) {
        let tag = u64le(&row[..8]);
        if tag == 0 {
            terminated = true;
            break;
        }
        if tags.insert(tag, u64le(&row[8..])).is_some() {
            return Err("duplicate dynamic tags unsupported".into());
        }
    }
    if !terminated {
        return Err("unterminated dynamic segment".into());
    }
    let shoff = u64le(&data[40..48]);
    let shnum = u16le(&data[60..62]) as u64;
    if shnum == 0 {
        return Err("extended section numbering unsupported".into());
    }
    let mut headers = Vec::new();
    for n in 0..shnum {
        headers.push(range(
            data,
            shoff.checked_add(n * 64).ok_or("section header overflow")?,
            64,
        )?);
    }
    let section = |index: usize| -> Result<Section<'_>, String> {
        let header = *headers.get(index).ok_or("invalid section link")?;
        let bytes = range(data, u64le(&header[24..32]), u64le(&header[32..40]))?;
        if loaded(data, u64le(&header[16..24]), bytes.len() as u64)? != bytes {
            return Err("ELF metadata section differs from loaded bytes".into());
        }
        Ok(Section {
            header,
            data: bytes,
        })
    };
    let symbol_sections: Vec<_> = headers
        .iter()
        .enumerate()
        .filter(|(_, h)| u32le(&h[4..8]) == 11)
        .map(|(i, _)| i)
        .collect();
    if symbol_sections.len() != 1 {
        return Err("exactly one dynamic symbol table is required".into());
    }
    let symbols = section(symbol_sections[0])?;
    if symbols.data.len() % 24 != 0
        || u64le(&symbols.header[56..64]) != 24
        || tags.get(&11) != Some(&24)
        || tags.get(&6) != Some(&symbols.address())
    {
        return Err("dynamic symbol table does not match loader metadata".into());
    }
    let strings = section(u32le(&symbols.header[40..44]) as usize)?;
    if u32le(&strings.header[4..8]) != 3
        || tags.get(&5) != Some(&strings.address())
        || tags.get(&10) != Some(&(strings.data.len() as u64))
    {
        return Err("dynamic string table does not match loader metadata".into());
    }
    let count = symbols.data.len() / 24;
    let symbol = |index: usize| -> Result<&[u8], String> {
        symbols
            .data
            .get(
                index.checked_mul(24).ok_or("symbol index overflow")?
                    ..index
                        .checked_add(1)
                        .and_then(|x| x.checked_mul(24))
                        .ok_or("symbol index overflow")?,
            )
            .ok_or("hash lookup exceeds dynamic symbol table".into())
    };
    let matches = |index: usize| -> Result<bool, String> {
        let s = symbol(index)?;
        let offset = u32le(&s[..4]) as usize;
        let text = strings
            .data
            .get(offset..)
            .ok_or("symbol name outside string table")?;
        let end = text
            .iter()
            .position(|b| *b == 0)
            .ok_or("unterminated symbol name")?;
        Ok(&text[..end] == name.as_bytes() && u16le(&s[6..8]) != 0 && matches!(s[4] >> 4, 1 | 2))
    };
    let table = |kind: u32, address: u64| -> Result<Section<'_>, String> {
        let sections: Vec<_> = headers
            .iter()
            .enumerate()
            .filter(|(_, h)| u32le(&h[4..8]) == kind && u64le(&h[16..24]) == address)
            .map(|(i, _)| i)
            .collect();
        if sections.len() != 1 {
            return Err("hash table does not match ELF section metadata".into());
        }
        section(sections[0])
    };
    let selected = if let Some(address) = tags.get(&0x6fff_fef5) {
        let table = table(0x6fff_fff6, *address)?;
        let bytes = table.data;
        if bytes.len() < 16 {
            return Err("truncated GNU hash header".into());
        }
        let buckets = u32le(&bytes[..4]) as usize;
        let first = u32le(&bytes[4..8]) as usize;
        let bloom = u32le(&bytes[8..12]) as usize;
        let shift = u32le(&bytes[12..16]);
        if buckets == 0 || first > count || bloom == 0 || !bloom.is_power_of_two() || shift >= 32 {
            return Err("invalid GNU hash dimensions".into());
        }
        let bucket_offset = 16usize
            .checked_add(bloom.checked_mul(8).ok_or("GNU bloom overflow")?)
            .ok_or("GNU hash overflow")?;
        let chain_offset = bucket_offset
            .checked_add(buckets.checked_mul(4).ok_or("GNU bucket overflow")?)
            .ok_or("GNU hash overflow")?;
        let expected = chain_offset
            .checked_add((count - first).checked_mul(4).ok_or("GNU chain overflow")?)
            .ok_or("GNU hash overflow")?;
        if bytes.len() != expected {
            return Err("GNU hash chain count differs from dynamic symbols".into());
        }
        let hash = name
            .bytes()
            .fold(5381u32, |h, c| h.wrapping_mul(33).wrapping_add(c as u32));
        let word = u64le(&bytes[16 + ((hash as usize / 64) & (bloom - 1)) * 8..][..8]);
        let mask = (1u64 << (hash % 64)) | (1u64 << ((u64::from(hash) >> shift) % 64));
        if word & mask != mask {
            return Err("export absent from GNU hash bloom filter".into());
        }
        let mut index =
            u32le(&bytes[bucket_offset + (hash as usize % buckets) * 4..][..4]) as usize;
        let mut selected = None;
        if index == 0 || index < first {
            return Err("export absent from GNU hash bucket".into());
        }
        while index < count {
            let chain = u32le(&bytes[chain_offset + (index - first) * 4..][..4]);
            if chain | 1 == hash | 1 && matches(index)? {
                selected = Some(index);
                break;
            }
            if chain & 1 != 0 {
                break;
            }
            index += 1;
        }
        selected.ok_or("loader GNU hash lookup does not resolve the exported symbol")?
    } else if let Some(address) = tags.get(&4) {
        let table = table(5, *address)?;
        let bytes = table.data;
        if bytes.len() < 8 {
            return Err("truncated SysV hash header".into());
        }
        let buckets = u32le(&bytes[..4]) as usize;
        let chains = u32le(&bytes[4..8]) as usize;
        if buckets == 0
            || chains != count
            || 8usize.checked_add(
                buckets
                    .checked_add(chains)
                    .and_then(|n| n.checked_mul(4))
                    .ok_or("SysV hash size overflow")?,
            ) != Some(bytes.len())
        {
            return Err("SysV hash dimensions differ from dynamic symbols".into());
        }
        let hash = name.bytes().fold(0u32, |h, c| {
            let h = h.wrapping_shl(4).wrapping_add(c as u32);
            let g = h & 0xf0000000;
            (h ^ (g >> 24)) & !g
        });
        let mut index = u32le(&bytes[8 + (hash as usize % buckets) * 4..][..4]) as usize;
        let mut selected = None;
        for _ in 0..count {
            if index == 0 {
                break;
            }
            if index >= count {
                return Err("SysV hash index outside symbol table".into());
            }
            if matches(index)? {
                selected = Some(index);
                break;
            }
            index = u32le(&bytes[8 + buckets * 4 + index * 4..][..4]) as usize;
        }
        selected.ok_or("loader SysV hash lookup does not resolve exported symbol")?
    } else {
        return Err("missing supported loader symbol hash table".into());
    };
    let s = symbol(selected)?;
    let section_index = u16le(&s[6..8]) as usize;
    if s[4] & 15 != 2
        || !matches!(s[5], 0 | 3)
        || section_index == 0
        || section_index >= headers.len()
        || section_index >= 0xff00
        || u64le(&s[8..16]) != entry
        || u64le(&s[16..24]) != size
    {
        return Err("loader-resolved symbol differs from modeled function".into());
    }
    Ok(())
}
