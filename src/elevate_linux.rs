//! Self-elevation for platforms without the Windows UAC shell verb.
//!
//! Uses `pkexec` (PolicyKit), which is the standard way to start a graphical
//! application with administrator rights on a Linux desktop.

use anyhow::{bail, Context, Result};
use std::process::Command;

/// Relaunch the current executable with the given arguments as Administrator.
pub fn relaunch_elevated(args: &[String]) -> Result<()> {
    let exe = std::env::current_exe().context("cannot locate our own executable")?;
    let mut cmd = Command::new("pkexec");
    cmd.arg(&exe);
    for a in args {
        cmd.arg(a);
    }
    match cmd.spawn() {
        Ok(_) => Ok(()),
        Err(e) => bail!("could not start pkexec (is PolicyKit installed?): {e}"),
    }
}
