//! The Mint that represents the native token

/// There are 10^9 lamports in one MUN
pub const DECIMALS: u8 = 9;

// The Mint for native MUN Token accounts
mundis_sdk::declare_id!("Mun1111111111111111111111111111111111111112");

#[cfg(test)]
mod tests {
    use super::*;
    use mundis_sdk::native_token::{lamports_to_mun, mun_to_lamports};

    #[test]
    fn test_decimals() {
        assert!(
            (lamports_to_mun(42) - crate::amount_to_ui_amount(42, DECIMALS)).abs() < f64::EPSILON
        );
        assert_eq!(
            mun_to_lamports(42.),
            crate::ui_amount_to_amount(42., DECIMALS)
        );
    }
}
