mundis_sdk::declare_builtin!(
    mundis_sdk::bpf_loader::ID,
    mundis_bpf_loader_program_with_jit,
    mundis_bpf_loader_program::process_instruction_jit
);
