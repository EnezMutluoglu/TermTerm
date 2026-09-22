use super::*;
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

#[derive(Clone)]
struct Line {
    tokens: Vec<String>,
    source: String,
}
fn lines(text: &str, source: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let normalized = if let Some((k, v)) = l.trim().split_once('=') {
                if !k.contains(char::is_whitespace) {
                    format!("{k} {v}")
                } else {
                    l.into()
                }
            } else {
                l.into()
            };
            let tokens = words(&normalized);
            (tokens.len() > 1).then(|| Line {
                tokens,
                source: format!("{source}:{}", i + 1),
            })
        })
        .collect()
}
fn expand(
    path: &Path,
    base: &Path,
    stack: &mut HashSet<PathBuf>,
    budget: &mut usize,
    p: &mut ImportPreview,
) -> Result<Vec<Line>> {
    ensure!(stack.len() < 16, "OpenSSH Include nesting exceeds 16 files");
    let canonical = std::fs::canonicalize(path)?;
    ensure!(
        stack.insert(canonical.clone()),
        "OpenSSH Include cycle at {}",
        path.display()
    );
    let text = read_text(path)?;
    *budget += text.len();
    ensure!(
        *budget <= 32 * 1024 * 1024,
        "OpenSSH Include files exceed 32 MiB"
    );
    let mut out = vec![];
    for line in lines(&text, &path.to_string_lossy()) {
        if !line.tokens[0].eq_ignore_ascii_case("include") {
            out.push(line);
            continue;
        }
        for item in &line.tokens[1..] {
            let pattern = resolve_path(item, base, None);
            let mut files = glob::glob(&pattern)?.collect::<std::result::Result<Vec<_>, _>>()?;
            files.sort();
            if files.is_empty() {
                p.warnings
                    .push(format!("{}: Include {item} matched no files", line.source));
            }
            for file in files {
                ensure!(out.len() < 100000, "Too many SSH directives");
                out.extend(expand(&file, base, stack, budget, p)?);
            }
        }
    }
    stack.remove(&canonical);
    Ok(out)
}
fn resolve_path(value: &str, base: &Path, alias: Option<&str>) -> String {
    let mut value = value.to_string();
    if let Some(alias) = alias {
        value = value.replace("%n", alias);
    }
    let path = if let Some(tail) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        crate::platform::home().join(tail)
    } else {
        PathBuf::from(value)
    };
    if path.is_absolute() {
        path.to_string_lossy().into_owned()
    } else {
        base.join(path).to_string_lossy().into_owned()
    }
}
fn matches(patterns: &[String], alias: &str) -> bool {
    let mut positive = false;
    for pattern in patterns {
        let (negative, p) = pattern
            .strip_prefix('!')
            .map(|s| (true, s))
            .unwrap_or((false, pattern.as_str()));
        if glob::Pattern::new(&p.to_lowercase()).is_ok_and(|p| p.matches(&alias.to_lowercase())) {
            if negative {
                return false;
            }
            positive = true;
        }
    }
    positive
}
pub(super) fn parse(text: &str, path: Option<&Path>, p: &mut ImportPreview) -> Result<()> {
    // OpenSSH resolves relative Include/IdentityFile names from the user's SSH directory.
    let ssh_dir = crate::platform::home().join(".ssh");
    let all = if let Some(path) = path {
        expand(path, &ssh_dir, &mut HashSet::new(), &mut 0, p)?
    } else {
        lines(text, "config")
    };
    let mut aliases = vec![];
    for line in &all {
        if line.tokens[0].eq_ignore_ascii_case("host") {
            for alias in &line.tokens[1..] {
                if !alias.contains(['*', '?', '!']) && !aliases.contains(alias) {
                    aliases.push(alias.clone());
                }
            }
        }
    }
    let mut jumps = BTreeMap::new();
    for alias in aliases {
        let mut h = host(&alias, &alias);
        let mut obtained = HashSet::new();
        let mut active = true;
        let mut forwards = vec![];
        let mut paths = vec![];
        for line in &all {
            let key = line.tokens[0].to_lowercase();
            let values = &line.tokens[1..];
            let value = values.join(" ");
            if key == "host" {
                active = matches(values, &alias);
                continue;
            }
            if key == "match" {
                active = false;
                p.warnings.push(format!(
                    "{alias}: {}: Match block skipped; commands are never executed",
                    line.source
                ));
                continue;
            }
            if !active {
                continue;
            }
            if ["localforward", "remoteforward", "dynamicforward"].contains(&key.as_str()) {
                forwards.push((key, value));
                continue;
            }
            if !obtained.insert(key.clone()) {
                if key == "identityfile" {
                    p.warnings.push(format!("{alias}: additional IdentityFile omitted; select the required identity in Keychain"));
                }
                continue;
            }
            let field = match key.as_str() {
                "hostname" => "address",
                "user" => "username",
                "port" => "port",
                "identityfile" => "keyPath",
                "certificatefile" => "certificatePath",
                "proxyjump" => {
                    jumps.insert(h.id.clone(), value);
                    continue;
                }
                "proxycommand" | "localcommand" | "remotecommand" | "include" => {
                    p.warnings.push(format!(
                        "{alias}: {} is not executed{}",
                        line.tokens[0],
                        if key == "include" {
                            " when importing text without a source file"
                        } else {
                            ""
                        }
                    ));
                    continue;
                }
                _ => {
                    p.warnings.push(format!(
                        "{alias}: unsupported SSH option {}",
                        line.tokens[0]
                    ));
                    continue;
                }
            };
            if field.ends_with("Path") {
                paths.push((field.to_string(), value));
                continue;
            }
            h.data[field] = if field == "port" {
                json!(value.parse::<u16>()?)
            } else {
                value.replace("%h", &alias).replace("%n", &alias).into()
            };
        }
        for (field, value) in paths {
            let value = value
                .replace("%h", h.data["address"].as_str().unwrap_or(&alias))
                .replace("%r", h.data["username"].as_str().unwrap_or(""));
            if value.replace("%n", &alias).contains('%') || value.contains("${") {
                p.warnings
                    .push(format!("{alias}: unresolved path expansion in {field}"));
            }
            h.data[&field] = resolve_path(&value, &ssh_dir, Some(&alias)).into();
        }
        for (kind, value) in forwards {
            match tunnel(&h, &kind, &value) {
                Ok(r) => p.records.push(r),
                Err(e) => p.warnings.push(format!("{alias}: {kind} omitted: {e}")),
            }
        }
        p.records.push(h);
    }
    for (id, value) in jumps {
        let chain = chain(&value, p)?;
        if let Some(h) = p.records.iter_mut().find(|r| r.id == id) {
            h.data["chain"] = json!(chain);
        }
    }
    Ok(())
}
pub(super) fn chain(value: &str, p: &mut ImportPreview) -> Result<Vec<String>> {
    let mut out = vec![];
    for alias in value.split(',').filter(|s| *s != "none" && !s.is_empty()) {
        let (user, endpoint) = alias.rsplit_once('@').unwrap_or(("", alias));
        let (address, port) = if endpoint.starts_with('[') {
            let (a, tail) = endpoint
                .split_once(']')
                .ok_or_else(|| anyhow::anyhow!("Invalid jump IPv6 endpoint"))?;
            ensure!(
                tail.is_empty() || tail.starts_with(':'),
                "Invalid jump IPv6 suffix"
            );
            (
                a.trim_start_matches('['),
                tail.strip_prefix(':')
                    .map(str::parse)
                    .transpose()?
                    .unwrap_or(22),
            )
        } else {
            match endpoint.rsplit_once(':') {
                Some((address, port)) => {
                    ensure!(
                        !address.contains(':'),
                        "Enclose jump IPv6 addresses in brackets"
                    );
                    (address, port.parse::<u16>()?)
                }
                None => (endpoint, 22),
            }
        };
        ensure!(
            !address.is_empty() && port > 0,
            "Invalid jump host address or port"
        );
        let found = p
            .records
            .iter()
            .find(|r| r.kind == "host" && r.data["label"] == address)
            .cloned();
        if user.is_empty() && endpoint == address {
            if let Some(h) = found {
                out.push(h.id);
                continue;
            }
        }
        let mut h = found.unwrap_or_else(|| host(alias, address));
        h.id = uuid::Uuid::new_v4().to_string();
        h.data["label"] = alias.into();
        if !user.is_empty() {
            h.data["username"] = user.into();
        }
        if endpoint != address {
            h.data["port"] = port.into();
        }
        out.push(h.id.clone());
        p.records.push(h);
    }
    Ok(out)
}
fn endpoint(value: &str, default_host: &str) -> Result<(String, u16)> {
    let (addr, port) = value.rsplit_once(':').unwrap_or((default_host, value));
    let port = port.parse::<u16>()?;
    ensure!(port > 0, "Port must be nonzero");
    Ok((addr.trim_matches(['[', ']']).to_string(), port))
}
pub(super) fn tunnel(h: &Record, kind: &str, value: &str) -> Result<Record> {
    let parts = words(value);
    ensure!(!parts.is_empty(), "Missing forwarding endpoint");
    let (bind, port) = endpoint(&parts[0], "127.0.0.1")?;
    let mode = match kind {
        "localforward" => "local",
        "remoteforward" => "remote",
        _ => "dynamic",
    };
    let mut data = json!({"label":format!("{} · {} {}",h.data["label"].as_str().unwrap_or("Host"),mode,port),"hostId":h.id,"mode":mode,"bindAddress":bind,"bindPort":port});
    if mode != "dynamic" {
        ensure!(
            parts.len() == 2,
            "Expected TCP target; Unix sockets are not supported"
        );
        let (target, target_port) = endpoint(&parts[1], "127.0.0.1")?;
        data["targetAddress"] = target.into();
        data["targetPort"] = target_port.into();
    }
    Ok(Record::new("tunnel", data))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_value_wildcard_jump_and_tunnels() {
        let p=parse_text("Host prod\n HostName 10.0.0.2\n User alice\n ProxyJump bob@[::1]:2222\n LocalForward 8000 localhost:80\nHost * !other\n User fallback\n Port 222\nHost other\n HostName example.test\n","openssh").unwrap();
        let h = p
            .records
            .iter()
            .find(|r| r.data["label"] == "prod")
            .unwrap();
        assert_eq!(h.data["username"], "alice");
        assert_eq!(h.data["port"], 222);
        assert_eq!(h.data["chain"].as_array().unwrap().len(), 1);
        assert!(p
            .records
            .iter()
            .any(|r| r.kind == "tunnel" && r.data["targetPort"] == 80));
    }
    #[test]
    fn include_order_cycle_and_source_unchanged() {
        let d = tempfile::tempdir().unwrap();
        let included = d.path().join("nested.conf");
        std::fs::write(&included, "Host test\n User from_include\n").unwrap();
        let config = d.path().join("config");
        let source = format!(
            "Include \"{}\"\nHost *\n User fallback\n",
            included.display()
        );
        std::fs::write(&config, &source).unwrap();
        let p = super::super::parse(&config, "openssh").unwrap();
        assert_eq!(p.records[0].data["username"], "from_include");
        assert_eq!(std::fs::read_to_string(&config).unwrap(), source);
        std::fs::write(&included, format!("Include \"{}\"", config.display())).unwrap();
        assert!(super::super::parse(&config, "openssh").is_err());
    }
}
