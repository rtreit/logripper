use std::collections::HashMap;
use std::sync::LazyLock;

use crate::proto::qsoripper::domain::{QsoRecord, RstReport};

#[derive(Clone, Copy)]
struct DxccEntityInfo {
    country: &'static str,
    continent: &'static str,
    cq_zone: Option<u32>,
    itu_zone: Option<u32>,
}

static DXCC_ENTITIES: LazyLock<HashMap<u32, DxccEntityInfo>> = LazyLock::new(|| {
    include_str!("data/dxcc_entities.tsv")
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let code = parts.next()?.parse().ok()?;
            let country = parts.next()?;
            let continent = parts.next()?;
            let cq_zone = parts.next().and_then(|value| value.parse().ok());
            let itu_zone = parts.next().and_then(|value| value.parse().ok());

            Some((
                code,
                DxccEntityInfo {
                    country,
                    continent,
                    cq_zone,
                    itu_zone,
                },
            ))
        })
        .collect()
});

pub(crate) fn parse_rst_report(raw: &str) -> RstReport {
    let trimmed = raw.trim();
    let bytes = trimmed.as_bytes();

    let parse_digit = |index: usize, min: u8, max: u8| -> Option<u32> {
        let digit = *bytes.get(index)?;
        if !digit.is_ascii_digit() {
            return None;
        }

        let value = digit - b'0';
        ((min..=max).contains(&value)).then_some(u32::from(value))
    };

    RstReport {
        readability: parse_digit(0, 1, 5),
        strength: parse_digit(1, 1, 9),
        tone: parse_digit(2, 1, 9),
        raw: raw.to_owned(),
    }
}

pub(crate) fn enrich_from_dxcc(qso: &mut QsoRecord) {
    let Some(dxcc) = qso.worked_dxcc else {
        return;
    };
    let Some(entity) = DXCC_ENTITIES.get(&dxcc).copied() else {
        return;
    };

    if string_is_blank(qso.worked_country.as_deref()) {
        qso.worked_country = Some(entity.country.to_owned());
    }
    if string_is_blank(qso.worked_continent.as_deref()) && !entity.continent.is_empty() {
        qso.worked_continent = Some(entity.continent.to_owned());
    }
    if qso.worked_cq_zone.is_none() {
        qso.worked_cq_zone = entity.cq_zone;
    }
    if qso.worked_itu_zone.is_none() {
        qso.worked_itu_zone = entity.itu_zone;
    }
}

fn string_is_blank(value: Option<&str>) -> bool {
    value.is_none_or(|value| value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::{enrich_from_dxcc, parse_rst_report};
    use crate::proto::qsoripper::domain::QsoRecord;

    #[test]
    fn rst_parser_extracts_numeric_components() {
        let rst = parse_rst_report("599");
        assert_eq!(rst.readability, Some(5));
        assert_eq!(rst.strength, Some(9));
        assert_eq!(rst.tone, Some(9));
    }

    #[test]
    fn dxcc_enrichment_preserves_existing_country() {
        let mut qso = QsoRecord {
            worked_dxcc: Some(339),
            worked_country: Some("Japan".to_string()),
            ..QsoRecord::default()
        };

        enrich_from_dxcc(&mut qso);

        assert_eq!(qso.worked_country.as_deref(), Some("Japan"));
        assert_eq!(qso.worked_continent.as_deref(), Some("AS"));
        assert_eq!(qso.worked_cq_zone, Some(25));
        assert_eq!(qso.worked_itu_zone, Some(45));
    }

    #[test]
    fn dxcc_enrichment_ignores_unknown_entity_codes() {
        let mut qso = QsoRecord {
            worked_dxcc: Some(9_999),
            ..QsoRecord::default()
        };

        enrich_from_dxcc(&mut qso);

        assert_eq!(qso.worked_country, None);
        assert_eq!(qso.worked_continent, None);
        assert_eq!(qso.worked_cq_zone, None);
        assert_eq!(qso.worked_itu_zone, None);
    }
}
