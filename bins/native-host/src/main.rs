//! The browser starts this program for the Lockdown extension; see the
//! library for what it does. It exits when the browser closes the port.

use std::process::ExitCode;

use lockdown_native_messaging::caller::Caller;
use lockdown_native_messaging::endpoint::data_dir_from_env;

fn main() -> ExitCode {
    let mut stdout = std::io::stdout().lock();
    let caller = match Caller::from_args(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            lockdown_native_host::say(&mut stdout, "refused", &e.to_string());
            return ExitCode::from(2);
        }
    };
    let data = match data_dir_from_env() {
        Ok(d) => d,
        Err(e) => {
            lockdown_native_host::say(&mut stdout, "no_app", &e.to_string());
            return ExitCode::from(2);
        }
    };
    let mut stdin = std::io::stdin().lock();
    match lockdown_native_host::run(caller, &data, &mut stdin, &mut stdout) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(1),
    }
}
