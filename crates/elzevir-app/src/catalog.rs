// SPDX-License-Identifier: GPL-3.0-or-later
//! Catalog commands and the read port (docs/specs/catalogo/modelo-bibliografico.md).
//!
//! Commands name vocabulary entries by their codes (`MAIN`, `BK`, `por`); the writer
//! resolves codes to numeric identities. Branches and item types must exist already;
//! other vocabulary codes (languages, relators, locations, collections, schemes,
//! thesauri) are created on first use with the code as their label.

use elzevir_domain::catalog::{
    AgentKind, BranchId, HubId, IdentifierKind, InstanceId, ItemId, ItemTypeId, MaterialType,
    Money, SubjectKind, WorkId, WorkRelationKind,
};
use serde::{Deserialize, Serialize};

/// Catalog commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogCommand {
    /// Creates a branch.
    CreateBranch {
        /// Unique code, such as `MAIN`.
        code: String,
        /// Display name.
        name: String,
    },
    /// Creates an item type.
    CreateItemType {
        /// Unique code, such as `BK`.
        code: String,
        /// Display description.
        description: String,
    },
    /// Creates a Hub that Works can join.
    CreateHub {
        /// Title shared by the Works of the Hub, such as a uniform title.
        title: String,
    },
    /// Adds one bibliographic record: a new Work and its first Instance.
    AddRecord(Box<NewRecord>),
    /// Adds an Item to an Instance.
    AddItem(Box<NewItem>),
}

/// A new Work with its first Instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewRecord {
    /// Preferred title of the Work.
    pub work_title: String,
    /// Category of material of the Work.
    pub material_type: MaterialType,
    /// The Hub the Work belongs to, if any.
    pub hub: Option<HubId>,
    /// Relations from this Work to existing Works, in order.
    pub relations: Vec<NewWorkRelation>,
    /// Classification number, such as a DDC number.
    pub classification: Option<String>,
    /// Classification scheme code, such as `ddc`.
    pub classification_scheme: Option<String>,
    /// Summary.
    pub summary: Option<String>,
    /// Language codes, in order.
    pub languages: Vec<String>,
    /// Contributors, in order.
    pub contributions: Vec<NewContribution>,
    /// Subjects, in order.
    pub subjects: Vec<NewSubject>,
    /// The Instance.
    pub instance: NewInstance,
    /// Items of the Instance, in order.
    pub items: Vec<ItemData>,
    /// The record as it arrived (MARC21 ISO 2709, UTF-8), kept byte for byte.
    pub source: Option<Vec<u8>>,
}

/// A relation from the new Work to an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewWorkRelation {
    /// How the new Work relates to `work`.
    pub kind: WorkRelationKind,
    /// The existing Work.
    pub work: WorkId,
}

/// One contributor of a Work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewContribution {
    /// Kind of agent.
    pub kind: AgentKind,
    /// The agent's heading, as written.
    pub heading: String,
    /// Relator code or term, such as `aut`.
    pub role: Option<String>,
    /// Whether this is the main entry (MARC21 1XX).
    pub primary: bool,
}

/// One subject of a Work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSubject {
    /// Kind of subject.
    pub kind: SubjectKind,
    /// The heading, as written.
    pub heading: String,
    /// Thesaurus code, such as `lcsh`.
    pub thesaurus: Option<String>,
}

/// An Instance's own data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NewInstance {
    /// Title as on the publication.
    pub title: String,
    /// Category of material of the Instance; `None` takes the Work's (a digital
    /// edition of a printed book says `Digital` here).
    pub material_type: Option<MaterialType>,
    /// Edition statement.
    pub edition: Option<String>,
    /// Place of publication.
    pub place: Option<String>,
    /// Publisher.
    pub publisher: Option<String>,
    /// Date of publication, as written.
    pub date: Option<String>,
    /// Physical description.
    pub extent: Option<String>,
    /// Series statement.
    pub series: Option<String>,
    /// Identifiers, in order.
    pub identifiers: Vec<NewIdentifier>,
}

/// One identifier of an Instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewIdentifier {
    /// Kind of identifier.
    pub kind: IdentifierKind,
    /// The value, as written.
    pub value: String,
    /// Qualifier, such as `(broch.)`.
    pub qualifier: Option<String>,
}

/// A new Item of an existing Instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewItem {
    /// The Instance it belongs to.
    pub instance: InstanceId,
    /// The Item's data.
    pub item: ItemData,
}

/// The circulation status numbers of an Item; 0 means normal, other values are defined by
/// the library, as Koha's authorised values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ItemStatus {
    /// Not for loan.
    pub not_for_loan: i64,
    /// Damaged.
    pub damaged: i64,
    /// Lost.
    pub lost: i64,
    /// Withdrawn.
    pub withdrawn: i64,
    /// Restricted.
    pub restricted: i64,
}

/// An Item's own data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemData {
    /// Barcode, unique when present.
    pub barcode: Option<String>,
    /// Inventory number (tombo), not necessarily unique.
    pub inventory_number: Option<String>,
    /// Home branch code.
    pub home_branch: String,
    /// Holding branch code.
    pub holding_branch: String,
    /// Item type code.
    pub item_type: String,
    /// Call number.
    pub call_number: Option<String>,
    /// Shelving location code.
    pub location: Option<String>,
    /// Collection code.
    pub collection: Option<String>,
    /// Purchase price.
    pub price: Option<Money>,
    /// Replacement price.
    pub replacement_price: Option<Money>,
    /// Copy number.
    pub copy_number: Option<String>,
    /// Circulation status numbers.
    pub status: ItemStatus,
    /// Date of acquisition, ISO 8601 as written.
    pub acquired_on: Option<String>,
}

/// What a catalog command created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogOutput {
    /// A branch was created.
    Branch(BranchId),
    /// An item type was created.
    ItemType(ItemTypeId),
    /// A Hub was created.
    Hub(HubId),
    /// A record was added.
    Record {
        /// The new Work.
        work: WorkId,
        /// Its Instance.
        instance: InstanceId,
        /// Its Items, in order.
        items: Vec<ItemId>,
    },
    /// An Item was added.
    Item(ItemId),
}

/// Why a catalog command was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogRefusal {
    /// A branch or item type with this code exists.
    #[error("code `{0}` is already in use")]
    CodeTaken(String),
    /// No branch has this code.
    #[error("unknown branch `{0}`")]
    UnknownBranch(String),
    /// No item type has this code.
    #[error("unknown item type `{0}`")]
    UnknownItemType(String),
    /// No Instance has this id.
    #[error("unknown instance {}", .0.0)]
    UnknownInstance(InstanceId),
    /// No Hub has this id.
    #[error("unknown hub {}", .0.0)]
    UnknownHub(HubId),
    /// No Work has this id.
    #[error("unknown work {}", .0.0)]
    UnknownWork(WorkId),
    /// Another Item has this barcode.
    #[error("barcode `{0}` is already in use")]
    BarcodeTaken(String),
}

/// An Instance as the catalog shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceView {
    /// The Instance.
    pub id: InstanceId,
    /// Its Work.
    pub work: WorkId,
    /// The Work's preferred title.
    pub work_title: String,
    /// The Work's Hub, if any.
    pub hub: Option<HubId>,
    /// The Work's category of material; `None` only for rows written before it existed.
    pub work_material_type: Option<MaterialType>,
    /// The Instance's category of material; `None` as for the Work.
    pub material_type: Option<MaterialType>,
    /// Relations from the Work to other Works, in order.
    pub relations: Vec<(WorkRelationKind, WorkId)>,
    /// The Instance's title.
    pub title: String,
    /// Publisher.
    pub publisher: Option<String>,
    /// Date of publication.
    pub date: Option<String>,
    /// The Work's classification number, such as a DDC number.
    pub classification: Option<String>,
    /// Contributor headings with their kind, in order.
    pub contributors: Vec<(AgentKind, String)>,
    /// Subject headings with their kind, in order.
    pub subjects: Vec<(SubjectKind, String)>,
    /// Identifiers, in order.
    pub identifiers: Vec<(IdentifierKind, String)>,
    /// Language codes, in order.
    pub languages: Vec<String>,
}

/// An Item as the catalog shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemView {
    /// The Item.
    pub id: ItemId,
    /// Barcode.
    pub barcode: Option<String>,
    /// Inventory number (tombo).
    pub inventory_number: Option<String>,
    /// Home branch code.
    pub home_branch: String,
    /// Item type code.
    pub item_type: String,
    /// Call number.
    pub call_number: Option<String>,
    /// Shelving location code.
    pub location: Option<String>,
}

/// Port: reading the catalog.
pub trait CatalogReader: Send + Sync {
    /// One Instance, or `None`.
    ///
    /// # Errors
    ///
    /// When storage fails or holds a value this build does not understand.
    fn instance(&self, id: InstanceId) -> Result<Option<InstanceView>, CatalogReadError>;

    /// The Items of an Instance, in creation order.
    ///
    /// # Errors
    ///
    /// When storage fails.
    fn items(&self, instance: InstanceId) -> Result<Vec<ItemView>, CatalogReadError>;
}

/// The catalog could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot read the catalog: {0}")]
pub struct CatalogReadError(pub String);
