use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
struct Group {
    vars: serde_json::Map<String, Value>,
    hosts: BTreeMap<String, serde_json::Map<String, Value>>,
    children: BTreeSet<String>,
}
fn scalar(value: &str) -> Value {
    serde_yaml::from_str(value).unwrap_or_else(|_| value.into())
}
fn yaml_group(name: &str, v: &Value, groups: &mut BTreeMap<String, Group>) -> Result<()> {
    let g = groups.entry(name.into()).or_default();
    if let Some(vars) = v["vars"].as_object() {
        g.vars.extend(vars.clone());
    }
    if let Some(hosts) = v["hosts"].as_object() {
        for (label, vars) in hosts {
            g.hosts
                .entry(label.clone())
                .or_default()
                .extend(vars.as_object().cloned().unwrap_or_default());
        }
    }
    if let Some(children) = v["children"].as_object() {
        g.children.extend(children.keys().cloned());
        for (child, v) in children {
            yaml_group(child, v, groups)?;
        }
    }
    Ok(())
}
fn depth(
    name: &str,
    groups: &BTreeMap<String, Group>,
    visiting: &mut BTreeSet<String>,
) -> Result<usize> {
    ensure!(visiting.len() < 64, "Inventory nesting exceeds 64 groups");
    ensure!(
        visiting.insert(name.into()),
        "Cyclic inventory group {name}"
    );
    let mut d = 0;
    for (parent, g) in groups {
        if g.children.contains(name) {
            d = d.max(depth(parent, groups, visiting)? + 1);
        }
    }
    visiting.remove(name);
    Ok(d)
}
fn ancestors(name: &str, groups: &BTreeMap<String, Group>, out: &mut BTreeSet<String>) {
    if !out.insert(name.into()) {
        return;
    }
    for (parent, g) in groups {
        if g.children.contains(name) {
            ancestors(parent, groups, out);
        }
    }
}
pub(super) fn parse(text: &str, p: &mut ImportPreview) -> Result<()> {
    ensure!(!text.trim_start().starts_with("$ANSIBLE_VAULT"),"Encrypted Ansible Vault files must first be exported with ansible-vault; no commands are executed by import");
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    if !text.lines().any(|l| l.trim_start().starts_with('['))
        && text.lines().any(|l| l.trim_end().ends_with(':'))
    {
        let root: Value = serde_yaml::from_str(text)?;
        let root = root
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Expected an inventory mapping"))?;
        for (name, value) in root {
            yaml_group(name, value, &mut groups)?;
        }
    } else {
        let mut section = "ungrouped".to_string();
        let mut mode = "hosts".to_string();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with(['#', ';']) {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                let name = &line[1..line.len() - 1];
                let (s, m) = name.split_once(':').unwrap_or((name, "hosts"));
                section = s.into();
                mode = m.into();
                groups.entry(section.clone()).or_default();
                continue;
            }
            let g = groups.entry(section.clone()).or_default();
            let w = words(line);
            if w.is_empty() {
                continue;
            }
            match mode.as_str() {
                "vars" => {
                    if let Some((k, v)) = line.split_once('=') {
                        g.vars.insert(k.trim().into(), scalar(v.trim()));
                    }
                }
                "children" => {
                    g.children.insert(w[0].clone());
                }
                "hosts" => {
                    let label = &w[0];
                    if label.contains('[') {
                        p.warnings.push(format!(
                            "{label}: inventory ranges are not expanded; skipped"
                        ));
                        continue;
                    }
                    let vars = g.hosts.entry(label.clone()).or_default();
                    for item in &w[1..] {
                        if let Some((k, v)) = item.split_once('=') {
                            vars.insert(k.into(), scalar(v));
                        }
                    }
                }
                _ => p.warnings.push(format!(
                    "Section {section}:{mode}: unsupported inventory section"
                )),
            }
        }
    }
    let missing = groups
        .values()
        .flat_map(|g| g.children.iter().cloned())
        .collect::<Vec<_>>();
    for name in missing {
        groups.entry(name).or_default();
    }
    let roots = groups
        .keys()
        .filter(|name| {
            name.as_str() != "all" && !groups.values().any(|g| g.children.contains(*name))
        })
        .cloned()
        .collect::<Vec<_>>();
    groups
        .entry("all".into())
        .or_default()
        .children
        .extend(roots);
    let mut order = vec![];
    for (name, g) in &groups {
        order.push((
            depth(name, &groups, &mut BTreeSet::new())?,
            g.vars
                .get("ansible_group_priority")
                .and_then(Value::as_i64)
                .unwrap_or(1),
            name.clone(),
        ));
    }
    order.sort();
    let mut ids = BTreeMap::new();
    for (_, _, name) in &order {
        let parents = groups
            .iter()
            .filter(|(_, g)| g.children.contains(name))
            .map(|(n, _)| n.clone())
            .collect::<Vec<_>>();
        let parent = parents
            .iter()
            .find_map(|n| ids.get(n))
            .cloned()
            .unwrap_or_default();
        let r = Record::new(
            "group",
            json!({"label":name,"groupId":parent,"inventoryParents":parents}),
        );
        ids.insert(name.clone(), r.id.clone());
        p.records.push(r);
    }
    let mut hosts: BTreeMap<String, (BTreeSet<String>, serde_json::Map<String, Value>)> =
        BTreeMap::new();
    for (_, _, name) in &order {
        for (label, vars) in &groups[name].hosts {
            let (member, hostvars) = hosts.entry(label.clone()).or_default();
            ancestors(name, &groups, member);
            hostvars.extend(vars.clone());
        }
    }
    for (label, (member, hostvars)) in hosts {
        let mut vars = serde_json::Map::new();
        let mut deepest = "all";
        for (_, _, name) in &order {
            if member.contains(name) {
                vars.extend(groups[name].vars.clone());
                deepest = name;
            }
        }
        vars.extend(hostvars);
        let mut h = host(&label, &label);
        h.data["groupId"] = ids[deepest].clone().into();
        h.data["tags"] = json!(member);
        h.data["inventoryGroups"] =
            json!(member.iter().map(|n| ids[n].clone()).collect::<Vec<_>>());
        for (key, value) in vars {
            let field = match key.as_str() {
                "ansible_host" | "ansible_ssh_host" => "address",
                "ansible_user" | "ansible_ssh_user" => "username",
                "ansible_password" | "ansible_ssh_pass" => "password",
                "ansible_port" | "ansible_ssh_port" => "port",
                "ansible_private_key_file" | "ansible_ssh_private_key_file" => "keyPath",
                "ansible_connection" => "protocol",
                "ansible_group_priority" => continue,
                "ansible_ssh_common_args" | "ansible_ssh_extra_args" => {
                    let opts = words(value.as_str().unwrap_or(""));
                    let mut i = 0;
                    while i < opts.len() {
                        let opt = if opts[i] == "-o" {
                            i += 1;
                            opts.get(i).map(String::as_str).unwrap_or("")
                        } else {
                            opts[i].strip_prefix("-o").unwrap_or(&opts[i])
                        };
                        if let Some(v) = opt.strip_prefix("ProxyJump=") {
                            h.data["chain"] = json!(super::openssh::chain(v, p)?);
                        } else {
                            p.warnings.push(format!(
                                "{label}: SSH argument {opt} not imported; no command executed"
                            ));
                        }
                        i += 1;
                    }
                    continue;
                }
                _ => {
                    p.warnings
                        .push(format!("{label}: unsupported inventory variable {key}"));
                    continue;
                }
            };
            if value
                .as_str()
                .is_some_and(|s| s.contains("{{") || s.contains("!vault"))
            {
                p.warnings.push(format!(
                    "{label}: unresolved {key}; enter its value manually"
                ));
                continue;
            }
            if field == "port" {
                let n = value
                    .as_u64()
                    .or_else(|| value.as_str()?.parse().ok())
                    .filter(|n| *n > 0 && *n <= 65535)
                    .ok_or_else(|| anyhow::anyhow!("{label}: invalid port"))?;
                h.data[field] = n.into();
            } else if field == "protocol" {
                let protocol = value.as_str().unwrap_or("");
                if ["ssh", "paramiko", "paramiko_ssh"].contains(&protocol) {
                    h.data[field] = "ssh".into();
                } else {
                    p.warnings.push(format!("{label}: unsupported inventory connection {protocol}; review protocol before connecting"));
                }
            } else {
                h.data[field] = value
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string())
                    .into();
            }
        }
        p.records.push(h);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ini_children_vars_and_host_priority() {
        let p=parse_text("[all:vars]\nansible_user=default\n[prod:children]\nweb\n[prod:vars]\nansible_port=2222\n[web]\napp ansible_host=10.0.0.8 ansible_user=deploy\n[web:vars]\nansible_user=webuser\n","ansible").unwrap();
        let h = p.records.iter().find(|r| r.kind == "host").unwrap();
        assert_eq!(h.data["username"], "deploy");
        assert_eq!(h.data["port"], 2222);
        assert_eq!(h.data["inventoryGroups"].as_array().unwrap().len(), 3);
    }
    #[test]
    fn yaml_non_all_root_and_cycles() {
        let p=parse_text("prod:\n  vars:\n    ansible_user: deploy\n  children:\n    web:\n      hosts:\n        app:\n          ansible_host: 10.0.0.1\n","ansible").unwrap();
        assert_eq!(
            p.records.iter().find(|r| r.kind == "host").unwrap().data["username"],
            "deploy"
        );
        assert!(parse_text("[a:children]\nb\n[b:children]\na", "ansible").is_err());
    }
}
