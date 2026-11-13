// SPDX-License-Identifier: GPL-3.0-or-later
//! MARCXML serialization, laid out byte for byte as Koha's `MARC::File::XML` writes it.

use crate::{Field, Record, Tag, WriteError};

/// The namespace attributes `MARC::File::XML` puts on `<record>` and `<collection>`.
const NAMESPACES: &str = "\n    xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n    \
    xsi:schemaLocation=\"http://www.loc.gov/MARC21/slim http://www.loc.gov/standards/marcxml/schema/MARC21slim.xsd\"\n    \
    xmlns=\"http://www.loc.gov/MARC21/slim\">";

/// Start of a MARCXML collection file (the staff export), with the blank line the export
/// prints after it.
///
/// Koha: `MARC::File::XML::header`, then `"\n"` (`Koha::Exporter::Record::export`).
pub const MARCXML_COLLECTION_HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
    <collection\n  xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n  \
    xsi:schemaLocation=\"http://www.loc.gov/MARC21/slim http://www.loc.gov/standards/marcxml/schema/MARC21slim.xsd\"\n  \
    xmlns=\"http://www.loc.gov/MARC21/slim\">\n\n";

/// End of a MARCXML collection file.
///
/// Koha: `MARC::File::XML::footer`, then `"\n"` (`Koha::Exporter::Record::export`).
pub const MARCXML_COLLECTION_FOOTER: &str = "</collection>\n";

impl Record {
    /// Appends the record as a standalone MARCXML document (XML declaration and namespaced
    /// `<record>`), as the OPAC export writes it. On error `out` is left as it was.
    ///
    /// Koha: `MARC::Record::as_xml_record` (`MARC::File::XML::encode` without collection).
    ///
    /// # Errors
    ///
    /// [`WriteError::NotUtf8`] when the leader does not declare UTF-8 or a value is not UTF-8.
    pub fn write_marcxml_document(&self, out: &mut String) -> Result<(), WriteError> {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<record");
        xml.push_str(NAMESPACES);
        xml.push_str("\n\n");
        self.write_marcxml_body(&mut xml)?;
        out.push_str(&xml);
        Ok(())
    }

    /// Appends the record as one `<record>` of a collection, followed by the blank line the
    /// export prints after each record, to go between [`MARCXML_COLLECTION_HEADER`] and
    /// [`MARCXML_COLLECTION_FOOTER`]. On error `out` is left as it was.
    ///
    /// Koha: `MARC::File::XML::record`, then `"\n"` (`Koha::Exporter::Record::export`).
    ///
    /// # Errors
    ///
    /// [`WriteError::NotUtf8`] when the leader does not declare UTF-8 or a value is not UTF-8.
    pub fn write_marcxml_collection_record(&self, out: &mut String) -> Result<(), WriteError> {
        let mut xml = String::from("<record>\n");
        self.write_marcxml_body(&mut xml)?;
        xml.push('\n');
        out.push_str(&xml);
        Ok(())
    }

    /// Leader, fields and `</record>\n`, one element per line.
    fn write_marcxml_body(&self, xml: &mut String) -> Result<(), WriteError> {
        let leader = utf8(Tag::LEADER, &self.leader)?;
        if self.leader.get(9) != Some(&b'a') {
            return Err(WriteError::NotUtf8 { tag: Tag::LEADER });
        }
        xml.push_str("  <leader>");
        escape_into(xml, leader);
        xml.push_str("</leader>\n");
        for field in &self.fields {
            write_field(xml, field)?;
        }
        xml.push_str("</record>\n");
        Ok(())
    }
}

fn write_field(xml: &mut String, field: &Field) -> Result<(), WriteError> {
    let tag = field.tag();
    let tag_text = utf8(tag, tag.as_bytes())?;
    match field {
        Field::Control { value, .. } => {
            xml.push_str("  <controlfield tag=\"");
            escape_into(xml, tag_text);
            xml.push_str("\">");
            escape_into(xml, utf8(tag, value)?);
            xml.push_str("</controlfield>\n");
        }
        Field::Data {
            indicators,
            subfields,
            ..
        } => {
            let [first, second] = indicators;
            xml.push_str("  <datafield tag=\"");
            escape_into(xml, tag_text);
            xml.push_str("\" ind1=\"");
            escape_into(xml, utf8(tag, &[*first])?);
            xml.push_str("\" ind2=\"");
            escape_into(xml, utf8(tag, &[*second])?);
            xml.push_str("\">\n");
            for subfield in subfields {
                xml.push_str("    <subfield code=\"");
                escape_into(xml, utf8(tag, &[subfield.code])?);
                xml.push_str("\">");
                escape_into(xml, utf8(tag, &subfield.value)?);
                xml.push_str("</subfield>\n");
            }
            xml.push_str("  </datafield>\n");
        }
    }
    Ok(())
}

fn utf8(tag: Tag, bytes: &[u8]) -> Result<&str, WriteError> {
    std::str::from_utf8(bytes).map_err(|_| WriteError::NotUtf8 { tag })
}

/// Koha: `MARC::File::XML::escape`. Only these five characters; control characters pass
/// through unchanged, as in Koha.
fn escape_into(xml: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => xml.push_str("&amp;"),
            '<' => xml.push_str("&lt;"),
            '>' => xml.push_str("&gt;"),
            '\'' => xml.push_str("&apos;"),
            '"' => xml.push_str("&quot;"),
            other => xml.push(other),
        }
    }
}
