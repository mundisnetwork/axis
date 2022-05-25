#![allow(clippy::integer_arithmetic)]
/// There are 10^9 lamports in one MUNDIS
pub const LAMPORTS_PER_MUNDIS: u64 = 1_000_000_000;

/// Approximately convert fractional native tokens (lamports) into native tokens (MUNDIS)
pub fn lamports_to_mdis(lamports: u64) -> f64 {
    lamports as f64 / LAMPORTS_PER_MUNDIS as f64
}

/// Approximately convert native tokens (MUNDIS) into fractional native tokens (lamports)
pub fn mdis_to_lamports(mdis: f64) -> u64 {
    (mdis * LAMPORTS_PER_MUNDIS as f64) as u64
}

use std::fmt::{Debug, Display, Formatter, Result};
pub struct Mdis(pub u64);

impl Mdis {
    fn write_in_mdis(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "◎{}.{:09}",
            self.0 / LAMPORTS_PER_MUNDIS,
            self.0 % LAMPORTS_PER_MUNDIS
        )
    }
}

impl Display for Mdis {
    fn fmt(&self, f: &mut Formatter) -> Result {
        self.write_in_mdis(f)
    }
}

impl Debug for Mdis {
    fn fmt(&self, f: &mut Formatter) -> Result {
        self.write_in_mdis(f)
    }
}
