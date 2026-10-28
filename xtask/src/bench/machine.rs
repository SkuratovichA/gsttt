//! The machine that measures and the moment it does: the facts the results carry about the run.

use std::fs;

use anyhow::{Context as _, bail};
use xshell::Shell;

/// One line naming the CPU, the cores, the memory and the operating system with its version. It never names the host or the user.
pub(crate) fn describe() -> anyhow::Result<String> {
    match std::env::consts::OS {
        "macos" => macos(),
        "linux" => linux(),
        other => bail!("cannot describe a {other} machine"),
    }
}

/// The current time in UTC as an RFC 3339 time.
pub(crate) fn now_utc() -> anyhow::Result<String> {
    Ok(Shell::new()?
        .cmd("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .quiet()
        .read()?)
}

fn macos() -> anyhow::Result<String> {
    let sh = Shell::new()?;
    let read = |program: &str, args: &[&str]| -> anyhow::Result<String> {
        sh.cmd(program)
            .args(args)
            .quiet()
            .read()
            .with_context(|| format!("cannot run {program}"))
    };
    let cpu = read("sysctl", &["-n", "machdep.cpu.brand_string"])?;
    let cores: usize = read("sysctl", &["-n", "hw.ncpu"])?
        .parse()
        .context("hw.ncpu is not a number")?;
    let memory: u64 = read("sysctl", &["-n", "hw.memsize"])?
        .parse()
        .context("hw.memsize is not a number")?;
    let os = format!(
        "{} {}",
        read("sw_vers", &["-productName"])?,
        read("sw_vers", &["-productVersion"])?
    );
    Ok(format_machine(&cpu, cores, memory, &os))
}

fn linux() -> anyhow::Result<String> {
    let read = |file: &str| fs::read_to_string(file).with_context(|| format!("cannot read {file}"));
    let cpuinfo = read("/proc/cpuinfo")?;
    let meminfo = read("/proc/meminfo")?;
    let os_release = read("/etc/os-release")?;
    let memory = memory_total(&meminfo).context("/proc/meminfo has no MemTotal")?;
    Ok(format_machine(
        cpu_model(&cpuinfo),
        processors(&cpuinfo),
        memory,
        &pretty_name(&os_release),
    ))
}

fn format_machine(cpu: &str, cores: usize, bytes: u64, os: &str) -> String {
    // Rounded to the nearest GiB.
    let gibibytes = (bytes + (1 << 29)) >> 30;
    format!("{cpu}, {cores} cores, {gibibytes} GiB, {os}")
}

/// The value of the first `key : value` line of a `/proc` file.
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == key).then_some(value.trim())
    })
}

/// The model of the CPU, or its architecture on the machines whose `/proc/cpuinfo` names none.
fn cpu_model(cpuinfo: &str) -> &str {
    field(cpuinfo, "model name")
        .or_else(|| field(cpuinfo, "Hardware"))
        .unwrap_or(std::env::consts::ARCH)
}

/// The number of logical processors in `/proc/cpuinfo`.
fn processors(cpuinfo: &str) -> usize {
    cpuinfo
        .lines()
        .filter(|line| {
            line.split_once(':')
                .is_some_and(|(name, _)| name.trim() == "processor")
        })
        .count()
}

/// The memory in bytes; `/proc/meminfo` counts in kilobytes of 1024 bytes.
fn memory_total(meminfo: &str) -> Option<u64> {
    let kilobytes: u64 = field(meminfo, "MemTotal")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    kilobytes.checked_mul(1024)
}

/// The name of the distribution with its version, from `/etc/os-release`.
fn pretty_name(os_release: &str) -> String {
    os_release
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map_or_else(
            || "Linux".to_owned(),
            |name| name.trim_matches('"').to_owned(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CPUINFO: &str = "processor\t: 0\nmodel name\t: Example CPU @ 3.00GHz\n\nprocessor\t: 1\nmodel name\t: Example CPU @ 3.00GHz\n";

    #[test]
    fn linux_files_are_read_into_the_facts() {
        assert_eq!(cpu_model(CPUINFO), "Example CPU @ 3.00GHz");
        assert_eq!(processors(CPUINFO), 2);
        assert_eq!(
            memory_total("MemTotal:       16384256 kB\nMemFree:         1000 kB\n"),
            Some(16_384_256 * 1024)
        );
        assert_eq!(
            pretty_name("NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\n"),
            "Ubuntu 24.04.1 LTS"
        );
        assert_eq!(pretty_name("ID=plan9\n"), "Linux");
    }

    #[test]
    fn a_machine_is_one_line() {
        assert_eq!(
            format_machine("Apple M4 Pro", 14, 51_539_607_552, "macOS 26.1"),
            "Apple M4 Pro, 14 cores, 48 GiB, macOS 26.1"
        );
    }
}
