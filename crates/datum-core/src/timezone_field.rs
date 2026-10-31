//! The `timezone` field (da-yw6o2): reserved by name in every database, with
//! no kit and no install. An ordinary switch, seen in the tail and the
//! pictures, whose states are IANA zones or `IGNORED`. vividn, 2026-10-30:
//! «a special switch field called timezone, where you can put in the IANA
//! timezone … enforce that it is an IANA timezone, and sets it's timezone».

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::Value as JsonValue;

use crate::db::IGNORED_STATE_VALUE;
use crate::entry::Entry;
use crate::error::Result;
use crate::state::FieldStateRows;
use crate::time::DatumTime;

pub const TIMEZONE_FIELD: &str = "timezone";

/// What the word `timezone` runs before the field has a spec to derive it.
pub const TIMEZONE_ALIAS: &str = "switch timezone";

/// The canonical name of the IANA zone `state` names, if it names one.
pub fn state_zone(state: &JsonValue) -> Option<String> {
    datum_parse::time::iana_zone(state.as_str()?).ok()
}

/// Whether two zone names are one zone: the same name, or a tzdb alias of it
/// ([`SPELLINGS`]). Same rules are not enough: Berlin and Paris differ (vividn,
/// 2026-10-30: «we should follow timezones even that do have the same rules»).
pub fn same_zone(a: &str, b: &str) -> bool {
    let name = |zone| match SPELLINGS.binary_search_by_key(&zone, |s| s.0) {
        Ok(i) => SPELLINGS[i].1,
        Err(_) => zone,
    };
    name(a) == name(b)
}

/// A state the field may hold: a zone, or `IGNORED`.
pub fn holds(state: &JsonValue) -> bool {
    state == IGNORED_STATE_VALUE || state_zone(state).is_some()
}

/// What the field holds over time: each stretch's first instant and its zone,
/// oldest first, `None` where it claims none (`IGNORED`). Before the first
/// stretch it claims nothing either.
pub struct Declared(Vec<(DateTime<Utc>, Option<String>)>);

impl Declared {
    pub fn load(conn: &Connection) -> Result<Self> {
        let rows = FieldStateRows::load_transitions(conn, TIMEZONE_FIELD)?;
        let stretches = (rows.changes().into_iter())
            .map(|c| (c.at, state_zone(&c.state)))
            .collect();
        Ok(Self(stretches))
    }

    /// The zone the field holds at `at`.
    pub fn at(&self, at: DateTime<Utc>) -> Option<&str> {
        let holding = self.0.partition_point(|(from, _)| *from <= at);
        self.0.get(holding.checked_sub(1)?)?.1.as_deref()
    }

    /// The zone of the latest stretch that `read`, given its zone, lands
    /// inside: the wall clock a line names belongs to the stretch whose own
    /// clock it reads inside it (da-mvt5i). Two can, where a move west
    /// repeated the hour, and the later is taken, the stretch being begun.
    pub fn reading_itself(&self, read: impl Fn(&str) -> Option<DateTime<Utc>>) -> Option<&str> {
        let until = |i: usize| self.0.get(i + 1).map(|(at, _)| *at);
        (0..self.0.len()).rev().find_map(|i| {
            let (from, zone) = &self.0[i];
            let zone = zone.as_deref()?;
            let at = read(zone)?;
            (*from <= at && until(i).is_none_or(|end| at < end)).then_some(zone)
        })
    }
}

/// The zone an entry of this field is in: the state after its trailing edge
/// where it has one. `None` for any other field's, `IGNORED` and a state
/// imported that names none.
pub fn zone_it_names(entry: &Entry) -> Option<String> {
    if entry.field != TIMEZONE_FIELD {
        return None;
    }
    [&entry.following_state, &entry.state]
        .into_iter()
        .flatten()
        .find_map(state_zone)
}

/// Put an entry of this field in the zone it names. The instant stands, so an
/// offset is believed as to when and the field as to where.
pub fn keep_in_named_zone(entry: &mut Entry) -> Result<()> {
    if let (Some(zone), Some(t)) = (zone_it_names(entry), entry.occur_time.as_mut()) {
        if t.timezone.as_ref() != Some(&zone) {
            *t = DatumTime {
                uncertainty: t.uncertainty.take(),
                ..DatumTime::from_utc_and_tz(t.utc, &zone)?
            };
        }
    }
    Ok(())
}

/// Another spelling of a zone and the zone, by spelling: tzdb's `backward` file
/// (2025b, chrono-tz 0.10.4), its sections of old names for one place and
/// `etcetera`'s link, taking a line's `#=` target where it has one. Left out
/// are its two sections of places merged for sharing clocks since 1970, so
/// Europe/Stockholm, a link to Europe/Berlin there, is a zone of its own here.
const SPELLINGS: &[(&str, &str)] = &[
    ("Africa/Asmera", "Africa/Asmara"),
    ("America/Buenos_Aires", "America/Argentina/Buenos_Aires"),
    ("America/Catamarca", "America/Argentina/Catamarca"),
    ("America/Cordoba", "America/Argentina/Cordoba"),
    ("America/Godthab", "America/Nuuk"),
    ("America/Indianapolis", "America/Indiana/Indianapolis"),
    ("America/Jujuy", "America/Argentina/Jujuy"),
    ("America/Knox_IN", "America/Indiana/Knox"),
    ("America/Louisville", "America/Kentucky/Louisville"),
    ("America/Mendoza", "America/Argentina/Mendoza"),
    ("America/Virgin", "America/St_Thomas"),
    ("Asia/Ashkhabad", "Asia/Ashgabat"),
    ("Asia/Calcutta", "Asia/Kolkata"),
    ("Asia/Chungking", "Asia/Chongqing"),
    ("Asia/Dacca", "Asia/Dhaka"),
    ("Asia/Istanbul", "Europe/Istanbul"),
    ("Asia/Katmandu", "Asia/Kathmandu"),
    ("Asia/Macao", "Asia/Macau"),
    ("Asia/Rangoon", "Asia/Yangon"),
    ("Asia/Saigon", "Asia/Ho_Chi_Minh"),
    ("Asia/Thimbu", "Asia/Thimphu"),
    ("Asia/Ujung_Pandang", "Asia/Makassar"),
    ("Asia/Ulan_Bator", "Asia/Ulaanbaatar"),
    ("Atlantic/Faeroe", "Atlantic/Faroe"),
    ("Australia/ACT", "Australia/Canberra"),
    ("Australia/LHI", "Australia/Lord_Howe"),
    ("Australia/NSW", "Australia/Sydney"),
    ("Australia/North", "Australia/Darwin"),
    ("Australia/Queensland", "Australia/Brisbane"),
    ("Australia/South", "Australia/Adelaide"),
    ("Australia/Tasmania", "Australia/Hobart"),
    ("Australia/Victoria", "Australia/Melbourne"),
    ("Australia/West", "Australia/Perth"),
    ("Australia/Yancowinna", "Australia/Broken_Hill"),
    ("Brazil/Acre", "America/Porto_Acre"),
    ("Brazil/DeNoronha", "America/Noronha"),
    ("Brazil/East", "America/Sao_Paulo"),
    ("Brazil/West", "America/Manaus"),
    ("CET", "Europe/Brussels"),
    ("CST6CDT", "America/Chicago"),
    ("Canada/Atlantic", "America/Halifax"),
    ("Canada/Central", "America/Winnipeg"),
    ("Canada/Eastern", "America/Toronto"),
    ("Canada/Mountain", "America/Edmonton"),
    ("Canada/Newfoundland", "America/St_Johns"),
    ("Canada/Pacific", "America/Vancouver"),
    ("Canada/Saskatchewan", "America/Regina"),
    ("Canada/Yukon", "America/Whitehorse"),
    ("Chile/Continental", "America/Santiago"),
    ("Chile/EasterIsland", "Pacific/Easter"),
    ("Cuba", "America/Havana"),
    ("EET", "Europe/Athens"),
    ("EST", "America/Panama"),
    ("EST5EDT", "America/New_York"),
    ("Egypt", "Africa/Cairo"),
    ("Eire", "Europe/Dublin"),
    ("Etc/GMT+0", "Etc/GMT"),
    ("Etc/GMT-0", "Etc/GMT"),
    ("Etc/GMT0", "Etc/GMT"),
    ("Etc/Greenwich", "Etc/GMT"),
    ("Etc/UCT", "Etc/UTC"),
    ("Etc/Universal", "Etc/UTC"),
    ("Etc/Zulu", "Etc/UTC"),
    ("Europe/Kiev", "Europe/Kyiv"),
    ("Europe/Nicosia", "Asia/Nicosia"),
    ("GB", "Europe/London"),
    ("GB-Eire", "Europe/London"),
    ("GMT", "Etc/GMT"),
    ("GMT+0", "Etc/GMT"),
    ("GMT-0", "Etc/GMT"),
    ("GMT0", "Etc/GMT"),
    ("Greenwich", "Etc/GMT"),
    ("HST", "Pacific/Honolulu"),
    ("Hongkong", "Asia/Hong_Kong"),
    ("Iceland", "Atlantic/Reykjavik"),
    ("Iran", "Asia/Tehran"),
    ("Israel", "Asia/Jerusalem"),
    ("Jamaica", "America/Jamaica"),
    ("Japan", "Asia/Tokyo"),
    ("Kwajalein", "Pacific/Kwajalein"),
    ("Libya", "Africa/Tripoli"),
    ("MET", "Europe/Brussels"),
    ("MST", "America/Phoenix"),
    ("MST7MDT", "America/Denver"),
    ("Mexico/BajaNorte", "America/Tijuana"),
    ("Mexico/BajaSur", "America/Mazatlan"),
    ("Mexico/General", "America/Mexico_City"),
    ("NZ", "Pacific/Auckland"),
    ("NZ-CHAT", "Pacific/Chatham"),
    ("Navajo", "America/Shiprock"),
    ("PRC", "Asia/Shanghai"),
    ("PST8PDT", "America/Los_Angeles"),
    ("Pacific/Ponape", "Pacific/Pohnpei"),
    ("Pacific/Samoa", "Pacific/Pago_Pago"),
    ("Pacific/Truk", "Pacific/Chuuk"),
    ("Poland", "Europe/Warsaw"),
    ("Portugal", "Europe/Lisbon"),
    ("ROC", "Asia/Taipei"),
    ("ROK", "Asia/Seoul"),
    ("Singapore", "Asia/Singapore"),
    ("Turkey", "Europe/Istanbul"),
    ("UCT", "Etc/UTC"),
    ("US/Alaska", "America/Anchorage"),
    ("US/Aleutian", "America/Adak"),
    ("US/Arizona", "America/Phoenix"),
    ("US/Central", "America/Chicago"),
    ("US/East-Indiana", "America/Indiana/Indianapolis"),
    ("US/Eastern", "America/New_York"),
    ("US/Hawaii", "Pacific/Honolulu"),
    ("US/Indiana-Starke", "America/Indiana/Knox"),
    ("US/Michigan", "America/Detroit"),
    ("US/Mountain", "America/Denver"),
    ("US/Pacific", "America/Los_Angeles"),
    ("US/Samoa", "Pacific/Pago_Pago"),
    ("UTC", "Etc/UTC"),
    ("Universal", "Etc/UTC"),
    ("W-SU", "Europe/Moscow"),
    ("Zulu", "Etc/UTC"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::DatumTime;

    /// A name for one place is one zone; two places sharing a clock, or tzdb's
    /// merge of them, are two.
    #[test]
    fn one_zone_is_a_name_and_its_spellings() {
        for (a, b) in [
            ("Asia/Calcutta", "Asia/Kolkata"),
            ("Europe/Kiev", "Europe/Kyiv"),
            ("US/Eastern", "America/New_York"),
            ("Europe/Berlin", "Europe/Berlin"),
            // A line's `#=` target, and not the zone tzdb points it at.
            ("Australia/ACT", "Australia/Canberra"),
        ] {
            assert!(same_zone(a, b) && same_zone(b, a), "{a} {b}");
        }
        for (a, b) in [
            ("Europe/Berlin", "Europe/Paris"),
            ("Europe/Berlin", "Europe/Stockholm"),
            ("Africa/Lagos", "Europe/Berlin"),
        ] {
            assert!(!same_zone(a, b) && !same_zone(b, a), "{a} {b}");
        }
    }

    /// Sorted for its search, every name a zone, one step to the zone, and
    /// each pair keeping one clock through a year.
    #[test]
    fn the_spellings_are_zones_keeping_one_clock() {
        assert!(SPELLINGS.windows(2).all(|w| w[0].0 < w[1].0));
        let offset = |zone: &str, month: u32| {
            let at = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2025, month, 1, 0, 0, 0);
            DatumTime::from_utc_and_tz(at.unwrap(), zone)
                .unwrap()
                .offset_minutes
        };
        for (spelling, zone) in SPELLINGS {
            assert!(SPELLINGS.binary_search_by_key(zone, |s| s.0).is_err());
            assert!(state_zone(&(*spelling).into()).is_some(), "{spelling}");
            assert!(state_zone(&(*zone).into()).is_some(), "{zone}");
            for month in [1, 7] {
                assert_eq!(offset(spelling, month), offset(zone, month), "{spelling}");
            }
        }
    }
}
