// SPDX-License-Identifier: GPL-3.0-or-later
//! The index schema: one document per Instance (docs/specs/catalogo/busca.md, §1).

use tantivy::Index;
use tantivy::schema::{
    FAST, INDEXED, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions,
};
use tantivy::tokenizer::{
    AsciiFoldingFilter, Language, LowerCaser, RemoveLongFilter, SimpleTokenizer, Stemmer,
    TextAnalyzer,
};

/// Words lower-cased, stemmed as Portuguese and without accents, so `coração` finds
/// `Coracao` and `histórias` finds `história`. Folding runs before stemming so that a
/// word typed without accents gets the same stem as the accented one.
pub(crate) const FOLDED: &str = "elzevir_folded";

/// The fields of the schema.
pub(crate) struct Fields {
    pub(crate) instance: tantivy::schema::Field,
    pub(crate) work: tantivy::schema::Field,
    pub(crate) hub: tantivy::schema::Field,
    pub(crate) title: tantivy::schema::Field,
    pub(crate) work_title: tantivy::schema::Field,
    pub(crate) main_contributor: tantivy::schema::Field,
    pub(crate) contributor: tantivy::schema::Field,
    pub(crate) subject: tantivy::schema::Field,
    pub(crate) publisher: tantivy::schema::Field,
    pub(crate) summary: tantivy::schema::Field,
    pub(crate) note: tantivy::schema::Field,
    pub(crate) date: tantivy::schema::Field,
    pub(crate) material_type: tantivy::schema::Field,
    pub(crate) classification: tantivy::schema::Field,
    pub(crate) identifier: tantivy::schema::Field,
    pub(crate) call_number: tantivy::schema::Field,
    pub(crate) call_number_key: tantivy::schema::Field,
    pub(crate) location: tantivy::schema::Field,
    pub(crate) barcode: tantivy::schema::Field,
    pub(crate) inventory_number: tantivy::schema::Field,
    pub(crate) language: tantivy::schema::Field,
}

pub(crate) fn build() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let words = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(FOLDED)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let code = STRING | STORED;
    let fields = Fields {
        instance: builder.add_u64_field("instance", INDEXED | STORED | FAST),
        work: builder.add_u64_field("work", STORED | FAST),
        hub: builder.add_u64_field("hub", STORED | FAST),
        title: builder.add_text_field("title", words.clone()),
        work_title: builder.add_text_field("work_title", words.clone()),
        main_contributor: builder.add_text_field("main_contributor", STORED),
        contributor: builder.add_text_field("contributor", words.clone()),
        subject: builder.add_text_field("subject", words.clone()),
        publisher: builder.add_text_field("publisher", words.clone()),
        summary: builder.add_text_field("summary", words.clone()),
        note: builder.add_text_field("note", words),
        date: builder.add_text_field("date", STORED),
        material_type: builder.add_u64_field("material_type", INDEXED | STORED | FAST),
        classification: builder.add_text_field("classification", code.clone()),
        identifier: builder.add_text_field("identifier", code.clone()),
        call_number: builder.add_text_field("call_number", STORED),
        call_number_key: builder.add_text_field("call_number_key", STRING),
        location: builder.add_text_field("location", code.clone()),
        barcode: builder.add_text_field("barcode", code.clone()),
        inventory_number: builder.add_text_field("inventory_number", code.clone()),
        language: builder.add_text_field("language", code),
    };
    (builder.build(), fields)
}

/// Tokenizers are not saved in the index, so every open registers them again.
pub(crate) fn register_tokenizers(index: &Index) {
    index.tokenizers().register(
        FOLDED,
        TextAnalyzer::builder(SimpleTokenizer::default())
            .filter(RemoveLongFilter::limit(40))
            .filter(LowerCaser)
            .filter(AsciiFoldingFilter)
            .filter(Stemmer::new(Language::Portuguese))
            .build(),
    );
}
