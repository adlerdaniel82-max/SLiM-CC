use crate::error::{SlimError, SlimResult};

const RECORD_HEADER_LEN: usize = 24;
const SUBRECORD_HEADER_LEN: usize = 6;

pub fn parse_plugin_masters(bytes: &[u8]) -> SlimResult<Vec<String>> {
    if bytes.len() < RECORD_HEADER_LEN || &bytes[0..4] != b"TES4" {
        return Err(SlimError::InvalidPath(
            "plugin does not start with TES4 header".into(),
        ));
    }

    let data_size = u32::from_le_bytes(
        bytes[4..8]
            .try_into()
            .map_err(|_| SlimError::InvalidPath("invalid TES4 record size".into()))?,
    ) as usize;
    let data_end = RECORD_HEADER_LEN
        .checked_add(data_size)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| SlimError::InvalidPath("TES4 record size exceeds file length".into()))?;

    let mut masters = Vec::new();
    let mut offset = RECORD_HEADER_LEN;
    while offset + SUBRECORD_HEADER_LEN <= data_end {
        let name = &bytes[offset..offset + 4];
        let size = u16::from_le_bytes(
            bytes[offset + 4..offset + 6]
                .try_into()
                .map_err(|_| SlimError::InvalidPath("invalid subrecord size".into()))?,
        ) as usize;
        offset += SUBRECORD_HEADER_LEN;

        if offset + size > data_end {
            return Err(SlimError::InvalidPath(
                "subrecord size exceeds TES4 data".into(),
            ));
        }

        if name == b"MAST" {
            let raw = &bytes[offset..offset + size];
            let end = raw
                .iter()
                .position(|value| *value == 0)
                .unwrap_or(raw.len());
            let master = String::from_utf8_lossy(&raw[..end]).trim().to_string();
            if !master.is_empty() {
                masters.push(master);
            }
        }

        offset += size;
    }

    Ok(masters)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plugin_masters_reads_mast_subrecords_from_tes4_header() {
        let plugin = synthetic_plugin(&["Skyrim.esm", "Update.esm"]);
        let masters = parse_plugin_masters(&plugin).expect("parse masters");

        assert_eq!(masters, vec!["Skyrim.esm", "Update.esm"]);
    }

    fn synthetic_plugin(masters: &[&str]) -> Vec<u8> {
        let mut subrecords = Vec::new();
        for master in masters {
            subrecords.extend_from_slice(b"MAST");
            subrecords.extend_from_slice(&(master.len() as u16 + 1).to_le_bytes());
            subrecords.extend_from_slice(master.as_bytes());
            subrecords.push(0);
        }

        let mut plugin = Vec::new();
        plugin.extend_from_slice(b"TES4");
        plugin.extend_from_slice(&(subrecords.len() as u32).to_le_bytes());
        plugin.extend_from_slice(&[0; 16]);
        plugin.extend_from_slice(&subrecords);
        plugin
    }
}
