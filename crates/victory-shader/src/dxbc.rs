//! The DXBC container: a header, then a table of FourCC-tagged parts.
//!
//! ```text
//! 0x00  "DXBC"
//! 0x04  digest[16]
//! 0x14  version major u16, minor u16
//! 0x18  total size u32
//! 0x1C  part count u32
//! 0x20  part offsets u32[count]
//! part: fourcc[4], size u32, data[size]
//! ```

use crate::ShaderError;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FourCC(pub [u8; 4]);

impl FourCC {
    pub const DXIL: FourCC = FourCC(*b"DXIL");
    /// Native code another compiler made, in place of `DXIL` (VIctory upstream 0b35b7f).
    pub const ACCC: FourCC = FourCC(*b"ACCC");
    pub const STAT: FourCC = FourCC(*b"STAT");
    pub const RTS0: FourCC = FourCC(*b"RTS0");
    pub const PSV0: FourCC = FourCC(*b"PSV0");
    pub const ISG1: FourCC = FourCC(*b"ISG1");
    pub const OSG1: FourCC = FourCC(*b"OSG1");
    /// Per-primitive outputs of a mesh shader.
    pub const PSG1: FourCC = FourCC(*b"PSG1");
    pub const SFI0: FourCC = FourCC(*b"SFI0");
    pub const HASH: FourCC = FourCC(*b"HASH");
    pub const ILDB: FourCC = FourCC(*b"ILDB");
    pub const ILDN: FourCC = FourCC(*b"ILDN");
}

impl std::fmt::Debug for FourCC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::fmt::Display for FourCC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for &b in &self.0 {
            let c = if b.is_ascii_graphic() { b as char } else { '.' };
            write!(f, "{c}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Part<'a> {
    pub fourcc: FourCC,
    /// Offset of the part header within the container.
    pub offset: usize,
    pub data: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct Container<'a> {
    pub digest: [u8; 16],
    pub version: (u16, u16),
    pub parts: Vec<Part<'a>>,
}

const HEADER_SIZE: usize = 0x20;
const MAX_PARTS: usize = 256;

impl<'a> Container<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, ShaderError> {
        if data.len() < HEADER_SIZE || &data[0..4] != b"DXBC" {
            return Err(ShaderError::NotDxbc);
        }
        let digest = data[4..20].try_into().expect("16 bytes");
        let version = (u16_at(data, 0x14)?, u16_at(data, 0x16)?);
        let total = u32_at(data, 0x18)? as usize;
        if total > data.len() || total < HEADER_SIZE {
            return Err(ShaderError::Malformed("container size disagrees with data"));
        }
        let data = &data[..total];
        let count = u32_at(data, 0x1C)? as usize;
        if count > MAX_PARTS {
            return Err(ShaderError::Malformed("implausible part count"));
        }
        let mut parts = Vec::with_capacity(count);
        for i in 0..count {
            let offset = u32_at(data, HEADER_SIZE + 4 * i)? as usize;
            let fourcc = FourCC(
                data.get(offset..offset + 4)
                    .ok_or(ShaderError::Malformed("part header out of bounds"))?
                    .try_into()
                    .expect("4 bytes"),
            );
            let size = u32_at(data, offset + 4)? as usize;
            let body = data
                .get(offset + 8..offset + 8 + size)
                .ok_or(ShaderError::Malformed("part data out of bounds"))?;
            parts.push(Part {
                fourcc,
                offset,
                data: body,
            });
        }
        Ok(Container {
            digest,
            version,
            parts,
        })
    }

    pub fn part(&self, fourcc: FourCC) -> Option<&Part<'a>> {
        self.parts.iter().find(|p| p.fourcc == fourcc)
    }

    /// The name the console's compiler gave the program, when the container
    /// holds it: a word reading `<prefix>_<stage>_<family>_<variant>_Wrapped`
    /// (the prefix, when there is one, says what made the container: `MSNC`
    /// and `MSR` for mesh pipelines, `TL` and `ptfx` for effects). It sits in
    /// the container's own bytes (`PSV0` for the shaders seen so far), never
    /// in the archive's tables; 95 % of the containers this title hands over
    /// have one, and the name is what a run's log and the frame recorder know
    /// a program by. `None` when no word reads as one.
    pub fn program_name(&self) -> Option<&'a str> {
        let mut best: Option<&'a str> = None;
        for part in &self.parts {
            for word in words(part.data) {
                if !reads_as_name(word) {
                    continue;
                }
                // The same name may appear twice; a longer word wins over a
                // shorter one that happens to read as a name too.
                if best.is_none_or(|b| word.len() > b.len()) {
                    best = Some(std::str::from_utf8(word).expect("ASCII by construction"));
                }
            }
        }
        best
    }
}

/// The runs of ASCII letters, digits and underscores in `data`.
fn words(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let word = |b: &u8| b.is_ascii_alphanumeric() || *b == b'_';
    let mut at = 0;
    std::iter::from_fn(move || {
        while at < data.len() && !word(&data[at]) {
            at += 1;
        }
        let start = at;
        while at < data.len() && word(&data[at]) {
            at += 1;
        }
        (at > start).then(|| &data[start..at])
    })
}

/// The stages a name may begin with.
const STAGES: [&[u8]; 8] = [b"AS", b"MS", b"VS", b"HS", b"DS", b"GS", b"PS", b"CS"];

/// Whether `word` reads as a program name: an optional one- or two-segment
/// prefix, then a stage, then a family, and ending in `_Wrapped`. Requiring the
/// stage to be a whole segment keeps a word such as `tMixOutputCS_...` (a
/// bitcode identifier) out.
fn reads_as_name(word: &[u8]) -> bool {
    if word.len() > 160 || !word.ends_with(b"_Wrapped") {
        return false;
    }
    let segments: Vec<&[u8]> = word.split(|&b| b == b'_').collect();
    if segments.iter().any(|s| s.is_empty()) {
        return false;
    }
    // ... `<stage>`, then at least the family, then `Wrapped`.
    matches!(segments.iter().position(|s| STAGES.contains(s)), Some(at) if at <= 2 && at + 2 < segments.len())
}

pub(crate) fn u16_at(data: &[u8], at: usize) -> Result<u16, ShaderError> {
    data.get(at..at + 2)
        .map(|b| u16::from_le_bytes(b.try_into().expect("2 bytes")))
        .ok_or(ShaderError::Malformed("truncated"))
}

pub(crate) fn u32_at(data: &[u8], at: usize) -> Result<u32, ShaderError> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")))
        .ok_or(ShaderError::Malformed("truncated"))
}

pub(crate) fn u64_at(data: &[u8], at: usize) -> Result<u64, ShaderError> {
    data.get(at..at + 8)
        .map(|b| u64::from_le_bytes(b.try_into().expect("8 bytes")))
        .ok_or(ShaderError::Malformed("truncated"))
}

/// Builds a container from parts (test helper).
#[cfg(test)]
pub fn build(parts: &[(FourCC, &[u8])]) -> Vec<u8> {
    let table = HEADER_SIZE + 4 * parts.len();
    let total = table + parts.iter().map(|(_, d)| 8 + d.len()).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"DXBC");
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(parts.len() as u32).to_le_bytes());
    let mut offset = table;
    for (_, d) in parts {
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += 8 + d.len();
    }
    for (fourcc, d) in parts {
        out.extend_from_slice(&fourcc.0);
        out.extend_from_slice(&(d.len() as u32).to_le_bytes());
        out.extend_from_slice(d);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_program_name() {
        // Names made up here (the title's own stay out of the repository).
        let named = build(&[
            (FourCC::SFI0, &[1, 2, 0, 0]),
            (FourCC::PSV0, b"\0\0\0\0PS_ToyFamily_0011aabb_Wrapped\0"),
            (FourCC::HASH, &[9; 20]),
        ]);
        assert_eq!(
            Container::parse(&named).unwrap().program_name(),
            Some("PS_ToyFamily_0011aabb_Wrapped")
        );
        // A mesh pipeline's prefix, and a two-segment one.
        let mesh = build(&[(FourCC::PSV0, b"..MSNC_VS_ToyMesh_0011aabb_Wrapped..")]);
        assert_eq!(
            Container::parse(&mesh).unwrap().program_name(),
            Some("MSNC_VS_ToyMesh_0011aabb_Wrapped")
        );
        let effect = build(&[(FourCC::PSV0, b"\0TL_PS_Ours_0011aabb_Wrapped\0")]);
        assert_eq!(
            Container::parse(&effect).unwrap().program_name(),
            Some("TL_PS_Ours_0011aabb_Wrapped")
        );
        // A bitcode identifier that holds a stage but is not a segment of its
        // own is not a name, and neither is a word without `_Wrapped`.
        assert_eq!(
            Container::parse(&build(&[(
                FourCC::DXIL,
                b"tMixOutputCS_Toy_0011aabb_Wrapped"
            )]))
            .unwrap()
            .program_name(),
            None
        );
        assert_eq!(
            Container::parse(&build(&[(FourCC::DXIL, b"CS_ToyFamily_0011aabb")]))
                .unwrap()
                .program_name(),
            None
        );
    }

    #[test]
    fn round_trips_parts() {
        let bytes = build(&[(FourCC::SFI0, &[1, 2, 3, 4]), (FourCC::HASH, &[9; 20])]);
        let c = Container::parse(&bytes).unwrap();
        assert_eq!(c.parts.len(), 2);
        assert_eq!(c.part(FourCC::SFI0).unwrap().data, &[1, 2, 3, 4]);
        assert_eq!(c.part(FourCC::HASH).unwrap().data.len(), 20);
        assert!(c.part(FourCC::DXIL).is_none());
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Container::parse(b"nope").unwrap_err(), ShaderError::NotDxbc);
        let mut bytes = build(&[(FourCC::SFI0, &[0; 8])]);
        let last = bytes.len() - 1;
        bytes[0x20] = 0xFF; // corrupt the first part offset
        assert!(Container::parse(&bytes).is_err());
        bytes.truncate(last);
        assert!(Container::parse(&bytes).is_err());
    }
}
