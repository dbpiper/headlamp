pub use headlamp_parity_support::{
    ParityRunGroup, ParityRunSpec, ParitySideLabel, assert_parity_with_diagnostics, mk_temp_dir,
    normalize, normalize_tty_ui, parity_meta, run_headlamp_with_args_tty, run_headlamp_with_args_tty_env,
    write_file,
};

// Re-export the full shared support surface (useful for any other tests that used to rely on this module).
pub use headlamp_parity_support::{
    binaries, cluster, diagnostics, diagnostics_assert, diff_report, env, exec, fs, git, hashing,
    parity_run, timing, token_ast, types,
};

pub mod runner_parity;


