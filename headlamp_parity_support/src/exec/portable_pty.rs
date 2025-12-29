use std::process::Command;
use std::time::Duration;

use portable_pty::{Child as PtyChild, CommandBuilder, PtySize, native_pty_system};

struct PtyExitResult {
    code: i32,
    timed_out: bool,
}

fn spawn_read_to_string_thread(
    mut reader: Box<dyn std::io::Read + Send>,
) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut output_bytes: Vec<u8> = Vec::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => return String::from_utf8_lossy(&output_bytes).to_string(),
                Ok(n) => output_bytes.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return String::from_utf8_lossy(&output_bytes).to_string(),
            }
        }
    })
}

fn wait_for_exit_code(child: &mut dyn PtyChild, timeout: Duration) -> PtyExitResult {
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return PtyExitResult {
                code: 124,
                timed_out: true,
            };
        }
        if let Some(status) = child.try_wait().ok().flatten() {
            return PtyExitResult {
                code: status.exit_code() as i32,
                timed_out: false,
            };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn run_cmd_tty_portable_pty(
    cmd: &Command,
    columns: usize,
    timeout: Duration,
) -> Option<(i32, String)> {
    let program = cmd.get_program().to_string_lossy().to_string();
    if program.trim().is_empty() {
        return None;
    }

    let pair = native_pty_system()
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

    let reader = pair.master.try_clone_reader().ok()?;
    drop(pair.master);

    let output_handle = spawn_read_to_string_thread(reader);
    let exit = wait_for_exit_code(&mut *child, timeout);
    let mut output = output_handle.join().unwrap_or_default();
    if exit.timed_out {
        output.push_str(&format!(
            "[headlamp_parity_support] timeout after {}s (killed)\n",
            timeout.as_secs()
        ));
    }
    Some((exit.code, output))
}
