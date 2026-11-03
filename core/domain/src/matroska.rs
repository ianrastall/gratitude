//! Reads a Matroska file's title (Segment/Info/Title) without the engine, so
//! a list can name a book that has never been opened. It walks the top-level
//! elements before the first cluster and nothing else.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const EBML_HEADER: u32 = 0x1A45_DFA3;
const SEGMENT: u32 = 0x1853_8067;
const INFO: u32 = 0x1549_A966;
const CLUSTER: u32 = 0x1F43_B675;
const TITLE: u32 = 0x7BA9;
const CHAPTERS: u32 = 0x1043_A770;
const MAX_TITLE_BYTES: u64 = 4096;
const MAX_SCAN_BYTES: u64 = 16 * 1024 * 1024;

fn byte(stream: &mut impl Read) -> Option<u8> {
    let mut b = [0u8; 1];
    stream.read_exact(&mut b).ok().map(|()| b[0])
}

/// An element's id and data size; a `None` size means unknown.
fn read_element(stream: &mut impl Read) -> Option<(u32, Option<u64>)> {
    let first = byte(stream).filter(|&b| b != 0)?;
    let id_length = first.leading_zeros() + 1;
    if id_length > 4 {
        return None;
    }
    let mut id = u32::from(first);
    for _ in 1..id_length {
        id = (id << 8) | u32::from(byte(stream)?);
    }

    let first = byte(stream).filter(|&b| b != 0)?;
    let size_length = first.leading_zeros() + 1;
    let mask = (0xFFu16 >> size_length) as u8;
    let mut value = u64::from(first & mask);
    let mut all_ones = first & mask == mask;
    for _ in 1..size_length {
        let next = byte(stream)?;
        value = (value << 8) | u64::from(next);
        all_ones &= next == 0xFF;
    }
    Some((id, (!all_ones).then_some(value)))
}

fn skip(stream: &mut impl Seek, size: u64) -> Option<()> {
    stream.seek(SeekFrom::Current(i64::try_from(size).ok()?)).ok().map(drop)
}

/// The title, or `None` if the file has none, is not Matroska or cannot be read.
pub fn read_title(path: &Path) -> Option<String> {
    let mut file = BufReader::with_capacity(4096, File::open(path).ok()?);
    let (id, size) = read_element(&mut file)?;
    if id != EBML_HEADER {
        return None;
    }
    skip(&mut file, size?)?;
    if read_element(&mut file)?.0 != SEGMENT {
        return None;
    }
    while file.stream_position().ok()? < MAX_SCAN_BYTES {
        let (id, size) = read_element(&mut file)?;
        let size = size?;
        if id == CLUSTER {
            return None;
        }
        if id != INFO {
            skip(&mut file, size)?;
            continue;
        }
        let end = file.stream_position().ok()? + size;
        while file.stream_position().ok()? < end {
            let (id, size) = read_element(&mut file)?;
            let size = size?;
            if id != TITLE {
                skip(&mut file, size)?;
                continue;
            }
            if size > MAX_TITLE_BYTES {
                return None;
            }
            let mut text = vec![0u8; size as usize];
            file.read_exact(&mut text).ok()?;
            let title = String::from_utf8_lossy(&text).trim_end_matches('\0').trim().to_owned();
            return (!title.is_empty()).then_some(title);
        }
        return None;
    }
    None
}

/// Source/credit notes from bounded Matroska tags before the first cluster.
pub fn read_source_notes(path: &Path) -> String {
    fn tags(bytes: &[u8], depth: u32, out: &mut Vec<String>) {
        if depth > 8 {
            return;
        }
        let mut cursor = std::io::Cursor::new(bytes);
        let mut name = String::new();
        let mut value = String::new();
        while (cursor.position() as usize) < bytes.len() {
            let Some((id, Some(size))) = read_element(&mut cursor) else { break };
            let start = cursor.position() as usize;
            let Some(end) = start.checked_add(size as usize).filter(|&end| end <= bytes.len()) else { break };
            match id {
                0x7373 | 0x67C8 => tags(&bytes[start..end], depth + 1, out),
                0x45A3 => name = String::from_utf8_lossy(&bytes[start..end]).to_uppercase(),
                0x4487 if size < 1024 * 1024 => value = String::from_utf8_lossy(&bytes[start..end]).into_owned(),
                _ => {}
            }
            cursor.set_position(end as u64);
        }
        if ["COMMENT", "DESCRIPTION", "SOURCE", "SOURCE_TYPE", "TAPER", "TRANSFERER", "LINEAGE", "CREDITS"]
            .contains(&name.as_str())
            && !value.is_empty()
        {
            out.push(value);
        }
    }
    let read = || -> Option<String> {
        let mut file = BufReader::new(File::open(path).ok()?);
        let (id, size) = read_element(&mut file)?;
        if id != EBML_HEADER {
            return None;
        }
        skip(&mut file, size?)?;
        if read_element(&mut file)?.0 != SEGMENT {
            return None;
        }
        let mut values = Vec::new();
        while file.stream_position().ok()? < MAX_SCAN_BYTES {
            let (id, size) = read_element(&mut file)?;
            if id == CLUSTER {
                break;
            }
            let size = size?;
            if id == 0x1254_C367 && size < 4 * 1024 * 1024 {
                let mut bytes = vec![0; size as usize];
                file.read_exact(&mut bytes).ok()?;
                tags(&bytes, 0, &mut values);
            } else {
                skip(&mut file, size)?;
            }
        }
        Some(values.join("\n"))
    };
    read().unwrap_or_default()
}

/// Chapter titles and lengths in seconds from the first edition, read from the header
/// before the first cluster. A chapter without an end runs to the next one's start, or
/// has length 0 when it is the last.
pub fn read_chapters(path: &Path) -> Vec<(String, f64)> {
    fn children(bytes: &[u8]) -> Vec<(u32, &[u8])> {
        let mut cursor = std::io::Cursor::new(bytes);
        let mut out = Vec::new();
        while (cursor.position() as usize) < bytes.len() {
            let Some((id, Some(size))) = read_element(&mut cursor) else { break };
            let start = cursor.position() as usize;
            let Some(end) = start.checked_add(size as usize).filter(|&end| end <= bytes.len()) else { break };
            out.push((id, &bytes[start..end]));
            cursor.set_position(end as u64);
        }
        out
    }
    fn uint(bytes: &[u8]) -> u64 {
        bytes.iter().take(8).fold(0, |n, &b| (n << 8) | u64::from(b))
    }
    let read = || -> Option<Vec<u8>> {
        let mut file = BufReader::new(File::open(path).ok()?);
        let (id, size) = read_element(&mut file)?;
        if id != EBML_HEADER {
            return None;
        }
        skip(&mut file, size?)?;
        if read_element(&mut file)?.0 != SEGMENT {
            return None;
        }
        while file.stream_position().ok()? < MAX_SCAN_BYTES {
            let (id, size) = read_element(&mut file)?;
            let size = size?;
            if id == CLUSTER {
                return None;
            }
            if id == CHAPTERS && size < 4 * 1024 * 1024 {
                let mut bytes = vec![0; size as usize];
                file.read_exact(&mut bytes).ok()?;
                return Some(bytes);
            }
            skip(&mut file, size)?;
        }
        None
    };
    let Some(bytes) = read() else { return Vec::new() };
    let Some((_, edition)) = children(&bytes).into_iter().find(|(id, _)| *id == 0x45B9) else { return Vec::new() };
    let atoms = children(edition)
        .into_iter()
        .filter(|(id, _)| *id == 0xB6)
        .map(|(_, atom)| {
            let mut start = 0;
            let mut end = None;
            let mut title = String::new();
            for (id, data) in children(atom) {
                match id {
                    0x91 => start = uint(data),
                    0x92 => end = Some(uint(data)),
                    0x80 => {
                        if let Some((_, s)) = children(data).into_iter().find(|(id, _)| *id == 0x85) {
                            title = String::from_utf8_lossy(s).trim_end_matches('\0').to_owned();
                        }
                    }
                    _ => {}
                }
            }
            (title, start, end)
        })
        .collect::<Vec<_>>();
    atoms
        .iter()
        .enumerate()
        .map(|(i, (title, start, end))| {
            let end = end.or_else(|| atoms.get(i + 1).map(|a| a.1)).unwrap_or(*start);
            (title.clone(), end.saturating_sub(*start) as f64 / 1e9)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_chapter_titles_and_lengths() {
        let display = |t: &str| element(&[0x80], &element(&[0x85], t.as_bytes()));
        let mut first = element(&[0x91], &[0]);
        first.extend(element(&[0x92], &90_500_000_000u64.to_be_bytes()));
        first.extend(display("Bertha"));
        let mut second = element(&[0x91], &90_500_000_000u64.to_be_bytes());
        second.extend(display("Good Lovin'"));
        let mut edition = element(&[0xB6], &first);
        edition.extend(element(&[0xB6], &second));
        let mut file = element(&[0x1A, 0x45, 0xDF, 0xA3], &[]);
        file.extend([0x18, 0x53, 0x80, 0x67, 0xFF]);
        file.extend(element(&[0x10, 0x43, 0xA7, 0x70], &element(&[0x45, 0xB9], &edition)));
        file.extend(element(&[0x1F, 0x43, 0xB6, 0x75], &[]));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("show.mka");
        std::fs::write(&path, &file).unwrap();
        assert_eq!(read_chapters(&path), vec![("Bertha".to_owned(), 90.5), ("Good Lovin'".to_owned(), 0.0)]);
    }

    fn element(id: &[u8], data: &[u8]) -> Vec<u8> {
        assert!(data.len() < 0x7F);
        let mut out = id.to_vec();
        out.push(0x80 | data.len() as u8);
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn reads_the_segment_title() {
        let mut info = element(&[0x2A, 0xD7, 0xB1], &[0x0F, 0x42, 0x40]); // TimestampScale
        info.extend(element(&[0x7B, 0xA9], "Earth\u{2019}s Children".as_bytes()));
        let mut segment = element(&[0x11, 0x4D, 0x9B, 0x74], &[0xEC, 0x80]); // SeekHead holding a Void
        segment.extend(element(&[0x15, 0x49, 0xA9, 0x66], &info));
        let mut file = element(&[0x1A, 0x45, 0xDF, 0xA3], &[0x42, 0x86, 0x81, 0x01]);
        // A segment of unknown size, as muxers write while streaming.
        file.extend([0x18, 0x53, 0x80, 0x67, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        file.extend(segment);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.mka");
        std::fs::write(&path, &file).unwrap();
        assert_eq!(read_title(&path).as_deref(), Some("Earth\u{2019}s Children"));
    }

    #[test]
    fn reads_source_notes_without_using_song_titles() {
        let mut tag = element(&[0x45, 0xA3], b"COMMENT");
        tag.extend(element(&[0x44, 0x87], b"SBD; Charlie Miller"));
        let simple = element(&[0x67, 0xC8], &tag);
        let mut file = element(&[0x1A, 0x45, 0xDF, 0xA3], &[]);
        file.extend([0x18, 0x53, 0x80, 0x67, 0xFF]);
        file.extend(element(&[0x12, 0x54, 0xC3, 0x67], &element(&[0x73, 0x73], &simple)));
        file.extend(element(&[0x1F, 0x43, 0xB6, 0x75], &[]));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.mka");
        std::fs::write(&path, &file).unwrap();
        assert_eq!(read_source_notes(&path), "SBD; Charlie Miller");
    }

    #[test]
    fn rejects_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.m4b");
        std::fs::write(&path, b"\0\0\0\x20ftypM4B ").unwrap();
        assert_eq!(read_title(&path), None);
        assert_eq!(read_title(&dir.path().join("absent.mka")), None);
    }
}
