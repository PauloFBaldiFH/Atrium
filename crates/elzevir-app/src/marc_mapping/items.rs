// SPDX-License-Identifier: GPL-3.0-or-later
//! Items from Koha's `952` (spec §1.4, Koha's default MARC21 framework).

use elzevir_domain::catalog::Money;

use super::{DataField, MappingError, Marc};
use crate::catalog::{ItemData, ItemStatus};

/// What every `952` of a record falls back on.
pub(super) struct ItemDefaults {
    item_type: Option<String>,
    call_number: Option<String>,
}

pub(super) fn defaults(marc: &Marc<'_>) -> ItemDefaults {
    ItemDefaults {
        item_type: marc.first(b"942").and_then(|field| field.raw(b'c')),
        call_number: fallback_call_number(marc),
    }
}

pub(super) fn map(marc: &Marc<'_>) -> Result<Vec<ItemData>, MappingError> {
    let defaults = defaults(marc);
    marc.data(b"952")
        .enumerate()
        .map(|(position, field)| item_of(position, field, &defaults))
        .collect()
}

/// The Item one `952` describes; `position` is its place among the record's `952`s.
pub(super) fn item_of(
    position: usize,
    field: DataField<'_>,
    defaults: &ItemDefaults,
) -> Result<ItemData, MappingError> {
    let home_branch = field
        .raw(b'a')
        .ok_or(MappingError::MissingHomeBranch { position })?;
    let item_type = field
        .raw(b'y')
        .or_else(|| defaults.item_type.clone())
        .ok_or(MappingError::MissingItemType { position })?;
    Ok(ItemData {
        barcode: field.raw(b'p'),
        inventory_number: field.raw(b'i'),
        holding_branch: field.raw(b'b').unwrap_or_else(|| home_branch.clone()),
        home_branch,
        item_type,
        call_number: field.raw(b'o').or_else(|| defaults.call_number.clone()),
        location: field.raw(b'c'),
        collection: field.raw(b'8'),
        price: field.raw(b'g').and_then(|price| cents(&price)),
        replacement_price: field.raw(b'v').and_then(|price| cents(&price)),
        copy_number: field.raw(b't'),
        status: status(field),
        acquired_on: field.raw(b'd'),
    })
}

/// Without `952$o`: `090$a` and `$b`, else `082$a` (vision B.4).
fn fallback_call_number(marc: &Marc<'_>) -> Option<String> {
    marc.first(b"090")
        .and_then(|field| field.raw_joined(b"ab"))
        .or_else(|| marc.first(b"082").and_then(|field| field.raw(b'a')))
}

fn status(field: DataField<'_>) -> ItemStatus {
    let number = |code| {
        field
            .raw(code)
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0)
    };
    ItemStatus {
        not_for_loan: number(b'7'),
        damaged: number(b'4'),
        lost: number(b'1'),
        withdrawn: number(b'0'),
        restricted: number(b'5'),
    }
}

/// `45.90`, `45,90` or `45` → cents; anything else (`doação`) → `None` (spec §4, case 4).
fn cents(price: &str) -> Option<Money> {
    let price = price.replace(',', ".");
    let (whole, fraction) = price.split_once('.').unwrap_or((&price, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if fraction.len() > 2 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let whole: i64 = whole.parse().ok()?;
    let fraction: i64 = format!("{fraction:0<2}").parse().ok()?;
    whole.checked_mul(100)?.checked_add(fraction).map(Money)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_become_cents() {
        assert_eq!(cents("45.90"), Some(Money(4590)));
        assert_eq!(cents("12,5"), Some(Money(1250)));
        assert_eq!(cents("30"), Some(Money(3000)));
        assert_eq!(cents("doação"), None);
        assert_eq!(cents("1.234"), None);
        assert_eq!(cents(""), None);
    }
}
