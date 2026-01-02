use std::process::Command;
use std::time::Duration;

#[cfg(unix)]
fn set_env_var_for_test(key: &str, value: &std::ffi::OsStr) -> impl Drop {
    struct Guard {
        key: String,
        previous: Option<std::ffi::OsString>,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            match self.previous.as_ref() {
                Some(v) => unsafe { std::env::set_var(&self.key, v) },
                None => unsafe { std::env::remove_var(&self.key) },
            }
        }
    }
    let previous = std::env::var_os(key);
    unsafe { std::env::set_var(key, value) };
    Guard {
        key: key.to_string(),
        previous,
    }
}

#[cfg(unix)]
fn write_executable_sh_script(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::io::Write;
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(file, "#!/bin/sh").unwrap();
    writeln!(file, "{}", body).unwrap();
    drop(file);
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn portable_pty_tty_exec_inherits_process_path_for_command_resolution() {
    let temp_dir = tempfile::tempdir().unwrap();
    write_executable_sh_script(temp_dir.path(), "headlamp-parity-pty-path-test", "echo OK");

    let _path_guard = set_env_var_for_test("PATH", temp_dir.path().as_os_str());

    let cmd = Command::new("headlamp-parity-pty-path-test");
    let (exit, output, backend) = headlamp_parity_support::exec::run_cmd_tty_with_backend_timeout(
        cmd,
        80,
        Duration::from_secs(10),
    );

    assert_eq!(backend.to_string(), "portable_pty");
    assert_eq!(exit, 0);
    assert!(output.contains("OK"));
}
