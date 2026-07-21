use std::process::Output;

#[derive(Debug)]
pub struct BuildCompiler {
    pub release: String,
    pub commit_hash: String,
    pub host: String,
    pub llvm_version: String,
}

pub fn compiler_identity_from_output(output: Output) -> Result<BuildCompiler, String> {
    if !output.status.success() {
        return Err(format!("rustc -Vv exited with status {}", output.status));
    }
    let stdout = std::str::from_utf8(&output.stdout)
        .map_err(|error| format!("rustc -Vv stdout is not UTF-8: {error}"))?;
    parse_rustc_verbose(stdout)
}

pub fn parse_rustc_verbose(output: &str) -> Result<BuildCompiler, String> {
    let mut release = None;
    let mut commit_hash = None;
    let mut host = None;
    let mut llvm_version = None;
    for line in output.lines() {
        if line.starts_with("rustc ") {
            continue;
        }
        let (key, value) = line
            .split_once(": ")
            .ok_or_else(|| format!("rustc -Vv contains malformed line `{line}`"))?;
        let value = value.trim();
        if value.is_empty() {
            return Err(format!("rustc -Vv contains an empty `{key}` field"));
        }
        let slot = match key {
            "release" => &mut release,
            "commit-hash" => &mut commit_hash,
            "host" => &mut host,
            "LLVM version" => &mut llvm_version,
            _ => continue,
        };
        if slot.replace(value.to_owned()).is_some() {
            return Err(format!("rustc -Vv contains duplicate `{key}` fields"));
        }
    }
    Ok(BuildCompiler {
        release: required("release", release)?,
        commit_hash: required("commit-hash", commit_hash)?,
        host: required("host", host)?,
        llvm_version: required("LLVM version", llvm_version)?,
    })
}

fn required(name: &str, value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| format!("rustc -Vv is missing required `{name}` field"))
}
