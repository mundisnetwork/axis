#![allow(clippy::integer_arithmetic)]
/// There are 10^9 lamports in one MUN
pub const LAMPORTS_PER_MUN: u64 = 1_000_000_000;

/// Approximately convert fractional native tokens (lamports) into native tokens (MUN)
pub fn lamports_to_mun(lamports: u64) -> f64 {
    lamports as f64 / LAMPORTS_PER_MUN as f64
}

/// Approximately convert native tokens (MUN) into fractional native tokens (lamports)
pub fn mun_to_lamports(mun: f64) -> u64 {
    (mun * LAMPORTS_PER_MUN as f64) as u64
}

use std::fmt::{Debug, Display, Formatter, Result};
pub struct Mun(pub u64);

impl Mun {
    fn write_in_mun(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "◎{}.{:09}",
            self.0 / LAMPORTS_PER_MUN,
            self.0 % LAMPORTS_PER_MUN
        )
    }
}

impl Display for Mun {
    fn fmt(&self, f: &mut Formatter) -> Result {
        self.write_in_mun(f)
    }
}

impl Debug for Mun {
    fn fmt(&self, f: &mut Formatter) -> Result {
        self.write_in_mun(f)
    }
}
