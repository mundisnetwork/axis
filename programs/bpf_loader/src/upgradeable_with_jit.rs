mundis_sdk::declare_builtin!(
    mundis_sdk::bpf_loader_upgradeable::ID,
    mundis_bpf_loader_upgradeable_program_with_jit,
    mundis_bpf_loader_program::process_instruction_jit,
    upgradeable_with_jit::id
);
