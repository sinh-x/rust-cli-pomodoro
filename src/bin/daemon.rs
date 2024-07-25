use daemonize::Daemonize;
use std::process::Command;

fn main() {
    let daemonize = Daemonize::new()
        .pid_file("/tmp/test.pid") // Every method except `new` and `start`
        .chown_pid_file(true) // is optional, see `Daemonize` documentation
        .working_directory("/tmp") // for default behaviour.
        .user("sinh")
        .group("sinh") // Group name
        .group(2) // Or group id
        .umask(0o777) // Set umask, `0o027` by default.
        .privileged_action(|| "Executed before drop privileges");

    match daemonize.start() {
        Ok(_) => {
            let output = Command::new("cargo")
                .arg("run")
                .output()
                .expect("Failed to execute command");

            if !output.stdout.is_empty() {
                println!("{}", String::from_utf8_lossy(&output.stdout));
            }

            if !output.stderr.is_empty() {
                eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            }
        }
        Err(e) => {
            eprintln!("Error, {}", e);
        }
    }
}
