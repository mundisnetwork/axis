use {
    crate::{
        stakes::{create_and_add_stakes, StakerInfo},
        unlocks::UnlockInfo,
    },
    mundis_sdk::{genesis_config::GenesisConfig, native_token::LAMPORTS_PER_MUNDIS},
};

// Team schedule: unlock after 9 months, then monthly for 24 months
const UNLOCK_AFTER_9_MONTHS_VESTED_FOR_24: UnlockInfo = UnlockInfo {
    cliff_fraction: 1.0 / 24.0,
    cliff_years: 0.75,
    unlocks: 23,
    unlock_years: 1.0 / 12.0,
    custodian: "MuneGztCcF9wXt5aNCuqe6Uq34nGDm86ccHWuEHhYPj",
};

// Private sale schedule: unlock after 9 months, then monthly for 12 months
const UNLOCK_AFTER_9_MONTHS_VESTED_FOR_12: UnlockInfo = UnlockInfo {
    cliff_fraction: 1.0 / 12.0,
    cliff_years: 0.75,
    unlocks: 11,
    unlock_years: 1.0 / 12.0,
    custodian: "MuneGztCcF9wXt5aNCuqe6Uq34nGDm86ccHWuEHhYPj",
};

// Foundation schedule: unlock from day 0, release monthly for 12 months
const UNLOCK_FROM_DAY_ZERO_VESTED_FOR_12: UnlockInfo = UnlockInfo {
    cliff_fraction: 1.0 / 12.0,
    cliff_years: 0.0,
    unlocks: 11,
    unlock_years: 1.0 / 12.0,
    custodian: "MuneGztCcF9wXt5aNCuqe6Uq34nGDm86ccHWuEHhYPj",
};

// Community fund schedule: unlock from day 0, release monthly for 24 months
const UNLOCK_FROM_DAY_ZERO_VESTED_FOR_24: UnlockInfo = UnlockInfo {
    cliff_fraction: 1.0 / 24.0,
    cliff_years: 0.0,
    unlocks: 23,
    unlock_years: 1.0 / 12.0,
    custodian: "MuneGztCcF9wXt5aNCuqe6Uq34nGDm86ccHWuEHhYPj",
};

// Private sale schedule: unlock from day 0
const UNLOCK_FROM_DAY_ZERO: UnlockInfo = UnlockInfo {
    cliff_fraction: 1.0,
    cliff_years: 0.0,
    unlocks: 0,
    unlock_years: 0.0,
    custodian: "MuneGztCcF9wXt5aNCuqe6Uq34nGDm86ccHWuEHhYPj",
};

const TEAM_LAMPORTS: u64 = 50_000_000 * LAMPORTS_PER_MUNDIS;
const FOUNDATION_LAMPORTS: u64 = 50_000_000 * LAMPORTS_PER_MUNDIS;
const COMMUNITY_LAMPORTS: u64 = 140_000_000 * LAMPORTS_PER_MUNDIS;
const PRIVATE_SALE_LAMPORTS: u64 = 140_000_000 * LAMPORTS_PER_MUNDIS;
const PUBLIC_SALE_LAMPORTS: u64 = 20_000_000 * LAMPORTS_PER_MUNDIS;

pub const TEAM_STAKER_INFOS: &[StakerInfo] = &[
    StakerInfo {
        name: "team members",
        staker: "Mungt5u1FJvZSouWrmMScXPRhPiPkgaPe9VepeouWGx",
        lamports: TEAM_LAMPORTS,
        withdrawer: Some("47cDa9zStSNbUfGksjQ2NktFPMYzV4MXADZjW73Gtfkt"),
    },
];

pub const FOUNDATION_STAKER_INFOS: &[StakerInfo] = &[
    StakerInfo {
        name: "mundis foundation",
        staker: "MuniJ25yadvJLo6jBAsdkqktAFwNNsGwct1czmqokPQ",
        lamports: FOUNDATION_LAMPORTS,
        withdrawer: Some("9uNarXA3yf3KyHwPi7vdvUhVw3t3odWJyVT1Hkx8R9nn"),
    },
];

pub const COMMUNITY_STAKER_INFOS: &[StakerInfo] = &[
    StakerInfo {
        name: "community fund",
        staker: "MunNDg3VKB2CheJaNfPqWZ8mkmYUhCNZpVD4Rrgs28t",
        lamports: COMMUNITY_LAMPORTS,
        withdrawer: Some("6WJdj215EX9y3dLJgEr91uSrHjRj84Ba2HcEZ2dKqi1c"),
    },
];

pub const PRIVATE_SALE_STAKER_INFOS: &[StakerInfo] = &[
    StakerInfo {
        name: "private sale",
        staker: "MunW6M8aJrWPevTEACPuwuSaRz8mRtCuKyognBDqzVL",
        lamports: PRIVATE_SALE_LAMPORTS,
        withdrawer: Some("J53uBDDJ2FhBDYcDWZwTM2nSYNohxGrMdBNceqzR145T"),
    },
];

pub const PUBLIC_SALE_STAKER_INFOS: &[StakerInfo] = &[
    StakerInfo {
        name: "public sale",
        staker: "MunitENa3moqh2xBmJ3a1BnL4H9Uukb3GDrgTYbqtPz",
        lamports: PUBLIC_SALE_LAMPORTS,
        withdrawer: Some("EdCwp8FbeB8dLCtUCMc9qieF8YwF8oowkgHd6cGT1TRn"),
    },
];

fn add_stakes(
    genesis_config: &mut GenesisConfig,
    staker_infos: &[StakerInfo],
    unlock_info: &UnlockInfo,
) -> u64 {
    staker_infos
        .iter()
        .map(|staker_info| create_and_add_stakes(genesis_config, staker_info, unlock_info, None))
        .sum::<u64>()
}

pub fn add_genesis_accounts(genesis_config: &mut GenesisConfig) {
    // add_stakes() and add_validators() award tokens for rent exemption and
    //  to cover an initial transfer-free period of the network

    let total_lamports = add_stakes(
        genesis_config,
        COMMUNITY_STAKER_INFOS,
        &UNLOCK_FROM_DAY_ZERO_VESTED_FOR_24,
    ) + add_stakes(
        genesis_config,
        PRIVATE_SALE_STAKER_INFOS,
        &UNLOCK_AFTER_9_MONTHS_VESTED_FOR_12,
    ) + add_stakes(
        genesis_config,
        PUBLIC_SALE_STAKER_INFOS,
        &UNLOCK_FROM_DAY_ZERO,
    ) + add_stakes(
        genesis_config,
        TEAM_STAKER_INFOS,
        &UNLOCK_AFTER_9_MONTHS_VESTED_FOR_24,
    ) + add_stakes(
        genesis_config,
        FOUNDATION_STAKER_INFOS,
        &UNLOCK_FROM_DAY_ZERO_VESTED_FOR_12,
    );

    assert_eq!(400_000_000 * LAMPORTS_PER_MUNDIS, total_lamports);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use mundis_sdk::clock::Epoch;
    use mundis_sdk::epoch_schedule::EpochSchedule;
    use mundis_sdk::native_token::lamports_to_mdis;
    use crate::unlocks::Unlocks;
    use super::*;

    #[test]
    fn test_add_genesis_accounts() {
        let mut genesis_config = GenesisConfig::default();

        add_genesis_accounts(&mut genesis_config);

        let lamports = genesis_config
            .accounts
            .iter()
            .map(|(_, account)| account.lamports)
            .sum::<u64>();

        assert_eq!(400_000_000 * LAMPORTS_PER_MUNDIS, lamports);
    }

    #[test]
    fn test_distribution() {
        let mut genesis_config = GenesisConfig::default();
        add_genesis_accounts(&mut genesis_config);

        // expected config
        const EPOCHS_PER_MONTH: Epoch = 2;
        // one tick/sec
        let tick_duration = Duration::new(1, 0);
        // one tick per slot
        let ticks_per_slot = 1;
        // two-week epochs at one second per slot
        let epoch_schedule = EpochSchedule::custom(14 * 24 * 60 * 60, 0, false);

        // Team
        println!("------------- Team Schedule -------------");
        assert_eq!(
            TEAM_LAMPORTS,
            Unlocks::new(
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_24.cliff_fraction,
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_24.cliff_years,
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_24.unlocks,
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_24.unlock_years,
                &epoch_schedule,
                &tick_duration,
                ticks_per_slot,
            ).map(|unlock| {
                println!("Month: {}, amount: {}", unlock.epoch / EPOCHS_PER_MONTH, lamports_to_mdis(unlock.amount(TEAM_LAMPORTS)));
                return unlock.amount(TEAM_LAMPORTS);
            })
                .sum::<u64>()
        );

        println!("------------- Foundation Schedule -------------");
        assert_eq!(
            FOUNDATION_LAMPORTS,
            Unlocks::new(
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_12.cliff_fraction,
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_12.cliff_years,
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_12.unlocks,
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_12.unlock_years,
                &epoch_schedule,
                &tick_duration,
                ticks_per_slot,
            ).map(|unlock| {
                println!("Month: {}, amount: {}", unlock.epoch / EPOCHS_PER_MONTH, lamports_to_mdis(unlock.amount(FOUNDATION_LAMPORTS)));
                return unlock.amount(FOUNDATION_LAMPORTS);
            })
                .sum::<u64>()
        );

        println!("------------- Community Schedule -------------");
        assert_eq!(
            COMMUNITY_LAMPORTS,
            Unlocks::new(
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_24.cliff_fraction,
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_24.cliff_years,
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_24.unlocks,
                UNLOCK_FROM_DAY_ZERO_VESTED_FOR_24.unlock_years,
                &epoch_schedule,
                &tick_duration,
                ticks_per_slot,
            ).map(|unlock| {
                println!("Month: {}, amount: {}", unlock.epoch / EPOCHS_PER_MONTH, lamports_to_mdis(unlock.amount(COMMUNITY_LAMPORTS)));
                return unlock.amount(COMMUNITY_LAMPORTS);
            })
                .sum::<u64>()
        );

        println!("------------- Private Sale Schedule -------------");
        assert_eq!(
            PRIVATE_SALE_LAMPORTS,
            Unlocks::new(
                UNLOCK_AFTER_9_MONTHS_VESTED_FOR_12.cliff_fraction,
                UNLOCK_AFTER_9_MONTHS_VESTED_FOR_12.cliff_years,
                UNLOCK_AFTER_9_MONTHS_VESTED_FOR_12.unlocks,
                UNLOCK_AFTER_9_MONTHS_VESTED_FOR_12.unlock_years,
                &epoch_schedule,
                &tick_duration,
                ticks_per_slot,
            ).map(|unlock| {
                println!("Month: {}, amount: {}", unlock.epoch / EPOCHS_PER_MONTH, lamports_to_mdis(unlock.amount(PRIVATE_SALE_LAMPORTS)));
                return unlock.amount(PRIVATE_SALE_LAMPORTS);
            })
                .sum::<u64>()
        );

        println!("------------- Public Sale Schedule -------------");
        assert_eq!(
            PUBLIC_SALE_LAMPORTS,
            Unlocks::new(
                UNLOCK_FROM_DAY_ZERO.cliff_fraction,
                UNLOCK_FROM_DAY_ZERO.cliff_years,
                UNLOCK_FROM_DAY_ZERO.unlocks,
                UNLOCK_FROM_DAY_ZERO.unlock_years,
                &epoch_schedule,
                &tick_duration,
                ticks_per_slot,
            ).map(|unlock| {
                println!("Month: {}, amount: {}", unlock.epoch / EPOCHS_PER_MONTH, lamports_to_mdis(unlock.amount(PUBLIC_SALE_LAMPORTS)));
                return unlock.amount(PUBLIC_SALE_LAMPORTS);
            })
                .sum::<u64>()
        );
    }
}
