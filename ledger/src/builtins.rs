use mundis_runtime::builtins::{Builtin, BuiltinFeatureTransition, Builtins};

#[allow(unused_macros)]
macro_rules! to_builtin {
    ($b:expr) => {
        Builtin::new(&$b.0, $b.1, $b.2)
    };
}

/// Builtin programs that are always available
fn genesis_builtins() -> Vec<Builtin> {
    // nothing here for now
    vec![]
}

/// Dynamic feature transitions for builtin programs
fn builtin_feature_transitions() -> Vec<BuiltinFeatureTransition> {
    vec![]
}

pub(crate) fn get() -> Builtins {
    Builtins {
        genesis_builtins: genesis_builtins(),
        feature_transitions: builtin_feature_transitions(),
    }
}
