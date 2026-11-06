//! LUT file reading with format detection. The library's parsers take text; this adds the
//! format choice (extension, then XML sniffing) and UTF-16 `.look` files, which Photoshop loads.

use craft::cms::lutfile::{self, LutError, LutFile};
use photocraft_doc::adjust::LutFormat;

/// The format [`parse`] reads `bytes` as: from `file_name`'s extension, else sniffed
/// (XML is `.look`, anything else `.cube`).
pub fn detect(file_name: &str, bytes: &[u8]) -> LutFormat {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".3dl") {
        LutFormat::ThreeDl
    } else if lower.ends_with(".look") || decode_text(bytes).trim_start().starts_with('<') {
        LutFormat::Look
    } else {
        LutFormat::Cube
    }
}

/// Parses a LUT, picking the format from `file_name`'s extension (falls back to sniffing).
pub fn parse(file_name: &str, bytes: &[u8]) -> Result<LutFile, LutError> {
    parse_as(detect(file_name, bytes), bytes)
}

/// Parses a LUT in a known format.
pub fn parse_as(format: LutFormat, bytes: &[u8]) -> Result<LutFile, LutError> {
    let text = decode_text(bytes);
    match format {
        LutFormat::Cube => lutfile::parse_cube(&text),
        LutFormat::ThreeDl => lutfile::parse_3dl(&text),
        LutFormat::Look => lutfile::parse_look(&text),
    }
}

/// UTF-16 with a byte-order mark (Photoshop loads such `.look` files), else UTF-8.
fn decode_text(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    let utf16 = |rest: &[u8], from: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|&c| from(c)).collect();
        String::from_utf16_lossy(&units).into()
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        _ => String::from_utf8_lossy(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> LutFile {
        LutFile::identity(2).unwrap()
    }

    #[test]
    fn look_reads_utf16_with_either_byte_order() {
        let id = identity();
        let hex: String = id.data.iter().flat_map(|v| v.to_le_bytes()).map(|b| format!("{b:02X}")).collect();
        let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-16\"?><look><LUT><size>\"2\"</size><data>\"{hex}\"</data></LUT></look>");
        let le: Vec<u8> = [0xFF, 0xFE].into_iter().chain(xml.encode_utf16().flat_map(u16::to_le_bytes)).collect();
        let be: Vec<u8> = [0xFE, 0xFF].into_iter().chain(xml.encode_utf16().flat_map(u16::to_be_bytes)).collect();
        assert_eq!(parse("a.look", &le).unwrap().data, id.data);
        assert_eq!(parse("a.look", &be).unwrap().data, id.data);
        // Sniffed as XML through the byte-order mark too, so the recorded format is the one parsed.
        assert_eq!(detect("a.lut", &le), LutFormat::Look);
        assert_eq!(parse_as(LutFormat::Look, &be).unwrap().data, id.data);
    }

    #[test]
    fn format_detection_matches_parse() {
        assert_eq!(detect("a.3DL", b"<look>"), LutFormat::ThreeDl);
        assert_eq!(detect("a.Look", b""), LutFormat::Look);
        assert_eq!(detect("a.cube", b"  <look>"), LutFormat::Look, "XML is sniffed as .look");
        assert_eq!(detect("a.txt", b"LUT_3D_SIZE 2"), LutFormat::Cube);
        let text = lutfile::write_cube(&identity());
        assert_eq!(parse_as(LutFormat::Cube, text.as_bytes()), parse("a.cube", text.as_bytes()));
    }

    fn hex(l: &LutFile) -> String {
        l.data.iter().flat_map(|v| v.to_le_bytes()).map(|b| format!("{b:02X}")).collect()
    }

    #[test]
    fn cube_title_with_hash_round_trips() {
        let l = LutFile::from_fn("Look #3", 2, |c| c).unwrap();
        assert_eq!(lutfile::parse_cube(&lutfile::write_cube(&l)).unwrap().title, "Look #3");
        let t = "TITLE \"A #b\" # note\nLUT_3D_SIZE 2 # c\n# comment\n";
        let rows = "0 0 0 # x\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        assert_eq!(lutfile::parse_cube(&format!("{t}{rows}")).unwrap().title, "A #b");
    }

    #[test]
    fn write_cube_keeps_non_default_domain() {
        let mut l = identity();
        l.domain_min = [-0.5, 0.0, 0.25];
        l.domain_max = [1.5, 2.0, 1.0];
        let back = lutfile::parse_cube(&lutfile::write_cube(&l)).unwrap();
        assert_eq!((back.domain_min, back.domain_max), (l.domain_min, l.domain_max));
    }

    #[test]
    fn three_dl_without_a_header_reads_low_nodes_as_12_bit() {
        let table = |peak: u32| -> String {
            let mut text = String::from("0 1023\n");
            for i in 0..8 {
                let on = |bit: u32| if i & bit != 0 { peak } else { 0 };
                text.push_str(&format!("{} {} {}\n", on(4), on(2), on(1)));
            }
            text
        };
        for peak in [255, 1023, 4095] {
            let white = lutfile::parse_3dl(&table(peak)).unwrap().data[21..24].to_vec();
            assert_eq!(white, [peak as f32 / 4095.0; 3], "peak {peak}");
        }
        // A header keeps the inferred 10-bit depth.
        let white = lutfile::parse_3dl(&format!("3DMESH\nMesh 1 10\n{}", table(1023))).unwrap().data[21..24].to_vec();
        assert_eq!(white, [1.0; 3]);
    }

    #[test]
    fn look_ignores_tags_inside_xml_comments() {
        let inverse = LutFile::from_fn("", 2, |[r, g, b]| [1.0 - r, 1.0 - g, 1.0 - b]).unwrap();
        let xml = format!(
            "<look><!-- <size>\"3\"</size><data>\"{}\"</data> --><LUT><size>\"2\"</size><data>\"{}\"</data></LUT><!-- unterminated <data>00",
            hex(&identity()),
            hex(&inverse)
        );
        assert_eq!(parse("commented.look", xml.as_bytes()).unwrap().data, inverse.data);
    }

    #[test]
    fn look_rgb_and_rgba_allow_whitespace_in_hex_data() {
        let id = identity();
        for channels in [3, 4] {
            let mut hex = String::new();
            for rgb in id.data.as_chunks::<3>().0 {
                for value in rgb.iter().copied().chain((channels == 4).then_some(1.0)) {
                    for byte in value.to_le_bytes() {
                        hex.push_str(&format!("{byte:02x} \t\r\n"));
                    }
                }
            }
            let xml = format!("<look><size>2</size><data>\"{hex}\"</data></look>");
            assert_eq!(parse("valid.look", xml.as_bytes()).unwrap().data, id.data);
        }
    }

    #[test]
    fn look_rejects_partial_floats_and_invalid_hex() {
        let hex = hex(&identity());
        for suffix in ["0", "00", "0000", "000000", "G", "é"] {
            let xml = format!("<look><size>2</size><data>{hex}{suffix}</data></look>");
            assert!(parse("bad.look", xml.as_bytes()).is_err(), "accepted malformed suffix {suffix:?}");
        }
        let invalid = hex.replacen('0', "G", 1);
        let xml = format!("<look><size>2</size><data>{invalid}</data></look>");
        assert!(parse("bad.look", xml.as_bytes()).unwrap_err().0.contains("invalid hexadecimal"));
        let xml = format!("<look><size>2</size><data>{hex}00</data></look>");
        assert!(parse("bad.look", xml.as_bytes()).unwrap_err().0.contains("incomplete float"));
    }
}
