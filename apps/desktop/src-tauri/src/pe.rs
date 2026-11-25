//! Read the product version embedded in a Windows executable (`VS_FIXEDFILEINFO`), on any OS.
//!
//! Only the PE headers and the `.rsrc` section are read, with every offset bounds-checked, so a
//! malformed or hostile file yields `None` rather than a crash or a huge read. An executable's
//! version is not always the full game version; callers label it with `executable` confidence.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const FIXED_FILE_INFO_SIGNATURE: u32 = 0xFEEF_04BD;
/// Resource sections hold icons and strings; real ones are far below this.
const MAX_RSRC_BYTES: u64 = 32 * 1024 * 1024;

pub fn product_version(path: &Path) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let rsrc = read_rsrc(&mut f)?;
    let at = (0..rsrc.len().saturating_sub(24))
        .step_by(4)
        .find(|&i| u32_at(&rsrc, i) == Some(FIXED_FILE_INFO_SIGNATURE))?;
    // dwSignature, dwStrucVersion, dwFileVersionMS, dwFileVersionLS, dwProductVersionMS, dwProductVersionLS
    let (ms, ls) = (u32_at(&rsrc, at + 16)?, u32_at(&rsrc, at + 20)?);
    let parts = [ms >> 16, ms & 0xffff, ls >> 16, ls & 0xffff];
    if parts == [0; 4] {
        return None;
    }
    // 1.32.0.0 reads better as 1.32; keep at least major.minor.
    let keep = parts.iter().rposition(|&p| p != 0).map_or(2, |i| (i + 1).max(2));
    Some(parts[..keep].iter().map(u32::to_string).collect::<Vec<_>>().join("."))
}

fn read_rsrc(f: &mut File) -> Option<Vec<u8>> {
    let mut dos = [0u8; 64];
    f.read_exact(&mut dos).ok()?;
    if &dos[..2] != b"MZ" {
        return None;
    }
    let pe = u64::from(u32_at(&dos, 0x3c)?);
    let mut coff = [0u8; 24];
    f.seek(SeekFrom::Start(pe)).ok()?;
    f.read_exact(&mut coff).ok()?;
    if &coff[..4] != b"PE\0\0" {
        return None;
    }
    let sections = u16::from_le_bytes([coff[6], coff[7]]);
    let optional_header = u16::from_le_bytes([coff[20], coff[21]]);
    f.seek(SeekFrom::Start(pe + 24 + u64::from(optional_header))).ok()?;
    for _ in 0..sections.min(96) {
        let mut s = [0u8; 40];
        f.read_exact(&mut s).ok()?;
        if s.starts_with(b".rsrc\0") {
            let size = u64::from(u32_at(&s, 16)?);
            let offset = u64::from(u32_at(&s, 20)?);
            if size == 0 || size > MAX_RSRC_BYTES {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            f.seek(SeekFrom::Start(offset)).ok()?;
            f.read_exact(&mut buf).ok()?;
            return Some(buf);
        }
    }
    None
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// A minimal PE with one `.rsrc` section holding a `VS_FIXEDFILEINFO`. Used by tests in other modules too.
#[cfg(test)]
pub fn fake_exe(product: [u16; 4]) -> Vec<u8> {
    let mut b = vec![0u8; 0x200];
    b[..2].copy_from_slice(b"MZ");
    b[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
    b[0x40..0x44].copy_from_slice(b"PE\0\0");
    b[0x46..0x48].copy_from_slice(&1u16.to_le_bytes()); // one section
    // optional header size 0, so the section table starts at 0x40 + 24 = 0x58
    b[0x58..0x5e].copy_from_slice(b".rsrc\0");
    b[0x58 + 16..0x58 + 20].copy_from_slice(&0x80u32.to_le_bytes()); // SizeOfRawData
    b[0x58 + 20..0x58 + 24].copy_from_slice(&0x100u32.to_le_bytes()); // PointerToRawData
    let r = 0x100 + 0x20; // some resource-tree bytes before the version block
    b[r..r + 4].copy_from_slice(&FIXED_FILE_INFO_SIGNATURE.to_le_bytes());
    let ms = (u32::from(product[0]) << 16) | u32::from(product[1]);
    let ls = (u32::from(product[2]) << 16) | u32::from(product[3]);
    b[r + 16..r + 20].copy_from_slice(&ms.to_le_bytes());
    b[r + 20..r + 24].copy_from_slice(&ls.to_le_bytes());
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_product_version_and_rejects_junk() {
        let dir = std::env::temp_dir().join(format!("repakhub-pe-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            p
        };
        let v132 = write("a.exe", &fake_exe([1, 32, 0, 0]));
        let v1203 = write("b.exe", &fake_exe([1, 2, 0, 3]));
        let zero = write("c.exe", &fake_exe([0, 0, 0, 0]));
        let junk = write("d.exe", b"MZ not really an executable");
        let mut huge = fake_exe([1, 0, 0, 0]);
        huge[0x58 + 16..0x58 + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        let huge = write("e.exe", &huge);

        let results = [&v132, &v1203, &zero, &junk, &huge].map(|p| product_version(p));
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(results, [Some("1.32".into()), Some("1.2.0.3".into()), None, None, None]);
    }
}
