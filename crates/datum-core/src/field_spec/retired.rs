//! The keys a stored spec may still carry that a spec no longer has: the
//! tracking state (v9) and the measurement unit (da-6v7z1). A spec reads
//! past them and the next write drops them, where a key this datum does
//! not know at all is kept ([`super::FieldSpec::extra`], da-klaf8).

pub(super) const RETIRED: &[&str] = &["tracked", "ignored", "unit"];
