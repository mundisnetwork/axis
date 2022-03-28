mundis_sdk::declare_builtin!(
    mundis_sdk::bpf_loader_upgradeable::ID,
    mundis_bpf_loader_upgradeable_program,
    mundis_bpf_loader_program::process_instruction,
    upgradeable::id
);
