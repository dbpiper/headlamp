use std::process::Command;
use std::time::Duration;

pub(crate) fn run_cmd_tty_portable_pty(
    cmd: &Command,
    columns: usize,
    timeout: Duration,
) -> Option<(i32, String)> {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};
    use std::io::Read;

    let program = cmd.get_program().to_string_lossy().to_string();
    if program.trim().is_empty() {
        return None;
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 40,
            cols: columns as u16,
            pixel_width: 0,
            pixel_height: 0,
        })
        .ok()?;

    let mut builder = CommandBuilder::new(program);
    builder
        .get_argv_mut()
        .extend(cmd.get_args().map(|a| a.to_os_string()));
    if let Some(cwd) = cmd.get_current_dir() {
        builder.cwd(cwd);
    }
    cmd.get_envs().for_each(|(key, value)| match value {
        Some(v) => {
            builder.env(key, v);
        }
        None => {
            builder.env_remove(key);
        }
    });

    let mut child = pair.slave.spawn_command(builder).ok()?;
    drop(pair.slave);

    let mut output = String::new();
    let mut reader = pair.master.try_clone_reader().ok()?;
    let mut buf: Vec<u8> = vec![];

    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            output.push_str(&format!(
                "[headlamp_parity_support] timeout after {}s (killed)\n",
                timeout.as_secs()
            ));
            return Some((124, output));
        }
        buf.clear();
        let n = reader.read_to_end(&mut buf).unwrap_or(0);
        if n > 0 {
            output.push_str(&String::from_utf8_lossy(&buf));
        }
        let status = child.try_wait().ok().flatten();
        if let Some(status) = status {
            let code = status.exit_code() as i32;
            return Some((code, output));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
