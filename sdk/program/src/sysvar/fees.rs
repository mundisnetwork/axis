//! This account contains the current cluster fees
//!
#![allow(deprecated)]

use {
    crate::{
        clone_zeroed, copy_field, fee_calculator::FeeCalculator,
        sysvar::Sysvar,
    },
    std::mem::MaybeUninit,
};

crate::declare_deprecated_sysvar_id!("SysvarFees111111111111111111111111111111111", Fees);

#[repr(C)]
#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct Fees {
    pub fee_calculator: FeeCalculator,
}

impl Clone for Fees {
    fn clone(&self) -> Self {
        clone_zeroed(|cloned: &mut MaybeUninit<Self>| {
            let ptr = cloned.as_mut_ptr();
            unsafe {
                copy_field!(ptr, self, fee_calculator);
            }
        })
    }
}

impl Fees {
    pub fn new(fee_calculator: &FeeCalculator) -> Self {
        #[allow(deprecated)]
        Self {
            fee_calculator: *fee_calculator,
        }
    }
}

impl Sysvar for Fees {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone() {
        let fees = Fees {
            fee_calculator: FeeCalculator {
                lamports_per_signature: 1,
            },
        };
        let cloned_fees = fees.clone();
        assert_eq!(cloned_fees, fees);
    }
}
