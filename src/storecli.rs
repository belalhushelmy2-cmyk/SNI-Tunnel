//! Profile management CLI: import, list, select and remove VMess servers and
//! custom-SNI profiles. All secrets stay DPAPI-sealed on disk (see `store`).

use crate::logging::Logger;
use crate::model::{AppConfig, SniProfile};
use crate::{model, store, vmess};
use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub fn is_store_command(c: &str) -> bool {
    matches!(
        c,
        "list"
            | "profiles"
            | "add"
            | "import"
            | "remove"
            | "rm"
            | "use"
            | "select"
            | "mux"
            | "sni"
            | "export"
    )
}

pub fn help() -> &'static str {
    r#"PROFILE COMMANDS:
  list | profiles                     List servers and SNI profiles
  add <link|file|json> [--name N] [--select]
         [--mux|--no-mux] [--mux-concurrency N]
         [--mux-xudp|--no-mux-xudp] [--mux-xudp-concurrency N]
         [--mux-udp443 reject|allow|skip]
  import <file>        [--name N] [--select]     (same as add)
  remove <ref>                        Remove a server (ref = index, id or name)
  use <ref>                           Select the active server
  mux <ref> [--mux|--no-mux] [--mux-concurrency N]
       [--mux-xudp|--no-mux-xudp] [--mux-xudp-concurrency N]
       [--mux-udp443 reject|allow|skip]
                                    Change the MUX settings of a server
  export [--out FILE]                 Export servers with UUIDs removed
  sni list
  sni add --name N [--server ref] --sni HOST [--ws-host H] [--ws-path P]
          [--connect-addr A] [--connect-port P] [--alpn a,b]
          [--no-tls] [--allow-insecure]
  sni use <ref> | sni off | sni remove <ref>
"#
}

fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}

struct Args {
    pos: Vec<String>,
    vals: HashMap<String, String>,
    flags: HashSet<String>,
}

fn parse(rest: &[String], value_opts: &[&str], bool_opts: &[&str]) -> Result<Args> {
    let mut a = Args {
        pos: Vec::new(),
        vals: HashMap::new(),
        flags: HashSet::new(),
    };
    let mut i = 0;
    while i < rest.len() {
        let t = &rest[i];
        if t.starts_with("--") {
            let key = t.clone();
            if value_opts.contains(&key.as_str()) {
                i += 1;
                let v = rest
                    .get(i)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("missing value for --{key}"))?;
                a.vals.insert(key, v);
            } else if bool_opts.contains(&key.as_str()) {
                a.flags.insert(key);
            } else {
                bail!("unknown option --{key}");
            }
        } else {
            a.pos.push(t.clone());
        }
        i += 1;
    }
    Ok(a)
}

fn find_server(cfg: &AppConfig, r: &str) -> Result<usize> {
    if let Ok(i) = r.parse::<usize>() {
        if i >= 1 && i <= cfg.servers.len() {
            return Ok(i - 1);
        }
    }
    if let Some(i) = cfg.servers.iter().position(|s| s.id == r) {
        return Ok(i);
    }
    let exact: Vec<usize> = cfg
        .servers
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.eq_ignore_ascii_case(r))
        .map(|(i, _)| i)
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    let sub: Vec<usize> = cfg
        .servers
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.to_lowercase().contains(&r.to_lowercase()))
        .map(|(i, _)| i)
        .collect();
    match sub.len() {
        0 => bail!("no server matches '{r}'"),
        1 => Ok(sub[0]),
        n => bail!("'{r}' is ambiguous ({n} servers match)"),
    }
}

fn find_sni(cfg: &AppConfig, r: &str) -> Result<usize> {
    if let Ok(i) = r.parse::<usize>() {
        if i >= 1 && i <= cfg.sni_profiles.len() {
            return Ok(i - 1);
        }
    }
    if let Some(i) = cfg.sni_profiles.iter().position(|s| s.id == r) {
        return Ok(i);
    }
    let sub: Vec<usize> = cfg
        .sni_profiles
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.to_lowercase().contains(&r.to_lowercase()))
        .map(|(i, _)| i)
        .collect();
    match sub.len() {
        0 => bail!("no SNI profile matches '{r}'"),
        1 => Ok(sub[0]),
        n => bail!("'{r}' is ambiguous ({n} SNI profiles match)"),
    }
}

pub fn run(args: &[String], _logger: &Logger) -> Result<i32> {
    let cmd = args[0].as_str();
    let rest = &args[1..];
    let read_only = matches!(cmd, "list" | "profiles" | "export")
        || (cmd == "sni" && matches!(rest.first().map(String::as_str), None | Some("list")));
    let _instance = if read_only {
        None
    } else {
        Some(crate::util::lock_instance()?)
    };
    match cmd {
        "list" | "profiles" => list(),
        "add" | "import" => import(rest),
        "remove" | "rm" => remove(rest),
        "use" | "select" => use_server(rest),
        "mux" => mux_cmd(rest),
        "sni" => sni_cmd(rest),
        "export" => export(rest),
        _ => bail!("unknown command '{cmd}'"),
    }
}

/// Change the multiplexing settings of a stored server.
///
/// The settings belong to the profile, so they are used the next time the
/// server is connected.
fn mux_cmd(rest: &[String]) -> Result<i32> {
    let a = parse(
        rest,
        &[
            "--mux-concurrency",
            "--mux-xudp-concurrency",
            "--mux-udp443",
        ],
        &["--mux", "--no-mux", "--mux-xudp", "--no-mux-xudp"],
    )?;
    let r = a
        .pos
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: mux <ref> [--mux|--no-mux] ..."))?;
    let mut cfg = store::load();
    let idx = find_server(&cfg, r)?;
    let p = &mut cfg.servers[idx];
    if a.flags.contains("--mux") {
        p.mux_enabled = true;
    }
    if a.flags.contains("--no-mux") {
        p.mux_enabled = false;
    }
    if let Some(c) = a.vals.get("--mux-concurrency") {
        p.mux_concurrency = c
            .parse()
            .map_err(|_| anyhow::anyhow!("--mux-concurrency must be a number"))?;
    }
    if a.flags.contains("--mux-xudp") {
        p.mux_xudp_enabled = true;
    }
    if a.flags.contains("--no-mux-xudp") {
        p.mux_xudp_enabled = false;
    }
    if let Some(c) = a.vals.get("--mux-xudp-concurrency") {
        p.mux_xudp_concurrency = c
            .parse()
            .map_err(|_| anyhow::anyhow!("--mux-xudp-concurrency must be a number"))?;
    }
    if let Some(v) = a.vals.get("--mux-udp443") {
        let v = v.trim().to_ascii_lowercase();
        if !model::UDP443_POLICIES.contains(&v.as_str()) {
            bail!(
                "unknown UDP/443 policy '{v}' (supported: {})",
                model::UDP443_POLICIES.join(", ")
            );
        }
        p.mux_xudp_udp443 = v;
    }
    let name = p.name.clone();
    let (enabled, concurrency, xudp, streams, policy) = (
        p.mux_enabled,
        p.mux_concurrency,
        p.mux_xudp_enabled,
        p.mux_xudp_concurrency,
        p.mux_xudp_udp443.clone(),
    );
    store::save(&cfg)?;
    println!(
        "{name}: mux {enabled} (concurrency {concurrency}), xudp {xudp} \
         (streams {streams}, udp/443 {policy})"
    );
    Ok(0)
}

fn list() -> Result<i32> {
    let cfg = store::load();
    println!("Servers ({}):", cfg.servers.len());
    for (i, s) in cfg.servers.iter().enumerate() {
        let sel = cfg.selected_server.as_deref() == Some(s.id.as_str());
        println!(
            "  {} {}. {:<20} {}:{}  {}/{}  sni={}  [{}]",
            if sel { "*" } else { " " },
            i + 1,
            trunc(&s.name, 20),
            s.address,
            s.port,
            s.network,
            if s.tls { "tls" } else { "none" },
            if s.sni.is_empty() { "-" } else { &s.sni },
            short(&s.id)
        );
    }
    println!("SNI profiles ({}):", cfg.sni_profiles.len());
    for (i, p) in cfg.sni_profiles.iter().enumerate() {
        let sel = cfg.selected_sni_profile.as_deref() == Some(p.id.as_str());
        let srv = cfg
            .server_by_id(&p.server_id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "(any)".into());
        println!(
            "  {} {}. {:<20} server={:<16} sni={} host={} [{}]",
            if sel { "*" } else { " " },
            i + 1,
            trunc(&p.name, 20),
            trunc(&srv, 16),
            if p.sni.is_empty() { "-" } else { &p.sni },
            if p.ws_host.is_empty() {
                "-"
            } else {
                &p.ws_host
            },
            short(&p.id)
        );
    }
    if cfg.servers.is_empty() {
        println!("  (none) — add one: sni-tunnel add \"vmess://...\"");
    }
    Ok(0)
}

fn import(rest: &[String]) -> Result<i32> {
    let a = parse(
        rest,
        &[
            "--name",
            "--mux-concurrency",
            "--mux-xudp-concurrency",
            "--mux-udp443",
        ],
        &[
            "--select",
            "--mux",
            "--no-mux",
            "--mux-xudp",
            "--no-mux-xudp",
        ],
    )?;
    let src = a
        .pos
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: add <link|file|json>"))?;
    let text = if Path::new(src).is_file() {
        std::fs::read_to_string(src)?
    } else {
        src.clone()
    };
    let mut imported = vmess::parse_any(&text)?;
    if let Some(n) = a.vals.get("--name") {
        imported[0].name = n.clone();
    }
    // The MUX settings belong to the profile, so they are stored with it.
    for p in imported.iter_mut() {
        if a.flags.contains("--mux") {
            p.mux_enabled = true;
        }
        if a.flags.contains("--no-mux") {
            p.mux_enabled = false;
        }
        if let Some(c) = a.vals.get("--mux-concurrency") {
            p.mux_concurrency = c
                .parse()
                .map_err(|_| anyhow::anyhow!("--mux-concurrency must be a number"))?;
        }
        if a.flags.contains("--mux-xudp") {
            p.mux_xudp_enabled = true;
        }
        if a.flags.contains("--no-mux-xudp") {
            p.mux_xudp_enabled = false;
        }
        if let Some(c) = a.vals.get("--mux-xudp-concurrency") {
            p.mux_xudp_concurrency = c
                .parse()
                .map_err(|_| anyhow::anyhow!("--mux-xudp-concurrency must be a number"))?;
        }
        if let Some(v) = a.vals.get("--mux-udp443") {
            let v = v.trim().to_ascii_lowercase();
            if !model::UDP443_POLICIES.contains(&v.as_str()) {
                bail!(
                    "unknown UDP/443 policy '{v}' (supported: {})",
                    model::UDP443_POLICIES.join(", ")
                );
            }
            p.mux_xudp_udp443 = v;
        }
    }

    let mut cfg = store::load_checked()?;
    let added = crate::profile_import::append(&mut cfg, imported);
    if added == 0 {
        bail!("nothing imported");
    }
    if a.flags.contains("--select") {
        cfg.selected_server = cfg.servers.last().map(|s| s.id.clone());
    }
    store::save(&cfg)?;
    println!("saved {} server(s); total {}", added, cfg.servers.len());
    Ok(0)
}

fn remove(rest: &[String]) -> Result<i32> {
    let a = parse(rest, &[], &[])?;
    let r = a
        .pos
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: remove <ref>"))?;
    let mut cfg = store::load();
    let idx = find_server(&cfg, r)?;
    let removed = cfg.servers.remove(idx);
    cfg.sni_profiles.retain(|p| p.server_id != removed.id);
    if cfg.selected_server.as_deref() == Some(removed.id.as_str()) {
        cfg.selected_server = cfg.servers.first().map(|s| s.id.clone());
    }
    if cfg
        .selected_sni_profile
        .as_deref()
        .map(|id| !cfg.sni_profiles.iter().any(|p| p.id == id))
        .unwrap_or(false)
    {
        cfg.selected_sni_profile = None;
    }
    store::save(&cfg)?;
    println!("removed '{}'", removed.name);
    Ok(0)
}

fn use_server(rest: &[String]) -> Result<i32> {
    let a = parse(rest, &[], &[])?;
    let r = a
        .pos
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: use <ref>"))?;
    let mut cfg = store::load();
    let idx = find_server(&cfg, r)?;
    let id = cfg.servers[idx].id.clone();
    let name = cfg.servers[idx].name.clone();
    cfg.selected_server = Some(id);
    store::save(&cfg)?;
    println!("selected server '{name}'");
    Ok(0)
}

fn sni_cmd(rest: &[String]) -> Result<i32> {
    let Some(sub) = rest.first().cloned() else {
        bail!("usage: sni <list|add|use|off|remove>");
    };
    let tail = &rest[1..];
    match sub.as_str() {
        "list" => list(),
        "add" => sni_add(tail),
        "use" => {
            let a = parse(tail, &[], &[])?;
            let r = a
                .pos
                .first()
                .ok_or_else(|| anyhow::anyhow!("usage: sni use <ref>"))?;
            let mut cfg = store::load();
            let idx = find_sni(&cfg, r)?;
            let id = cfg.sni_profiles[idx].id.clone();
            let name = cfg.sni_profiles[idx].name.clone();
            cfg.selected_sni_profile = Some(id);
            store::save(&cfg)?;
            println!("active SNI profile: '{name}'");
            Ok(0)
        }
        "off" => {
            let mut cfg = store::load();
            cfg.selected_sni_profile = None;
            store::save(&cfg)?;
            println!("SNI profile override disabled (using the server's own TLS settings)");
            Ok(0)
        }
        "remove" => {
            let a = parse(tail, &[], &[])?;
            let r = a
                .pos
                .first()
                .ok_or_else(|| anyhow::anyhow!("usage: sni remove <ref>"))?;
            let mut cfg = store::load();
            let idx = find_sni(&cfg, r)?;
            let removed = cfg.sni_profiles.remove(idx);
            if cfg.selected_sni_profile.as_deref() == Some(removed.id.as_str()) {
                cfg.selected_sni_profile = None;
            }
            store::save(&cfg)?;
            println!("removed SNI profile '{}'", removed.name);
            Ok(0)
        }
        other => bail!("unknown sni subcommand '{other}'"),
    }
}

fn sni_add(rest: &[String]) -> Result<i32> {
    let a = parse(
        rest,
        &[
            "--name",
            "--server",
            "--sni",
            "--ws-host",
            "--ws-path",
            "--connect-addr",
            "--connect-port",
            "--alpn",
        ],
        &["--no-tls", "--allow-insecure"],
    )?;
    let name = a
        .vals
        .get("--name")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("--name is required"))?;

    let mut cfg = store::load();
    let server_id = match a.vals.get("--server") {
        Some(r) => cfg.servers[find_server(&cfg, r)?].id.clone(),
        None => cfg
            .selected_server
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no --server given and no server selected"))?,
    };

    let profile = SniProfile {
        inherit_tls: false,
        id: uuid::Uuid::new_v4().to_string(),
        name,
        server_id,
        tls: !a.flags.contains("--no-tls"),
        sni: a.vals.get("--sni").cloned().unwrap_or_default(),
        connect_address: a.vals.get("--connect-addr").cloned().unwrap_or_default(),
        connect_port: a
            .vals
            .get("--connect-port")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        ws_host: a.vals.get("--ws-host").cloned().unwrap_or_default(),
        ws_path: a.vals.get("--ws-path").cloned().unwrap_or_default(),
        alpn: a
            .vals
            .get("--alpn")
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        allow_insecure: a.flags.contains("--allow-insecure"),
        fingerprint: String::new(),
    };
    println!(
        "added SNI profile [{}] {}",
        short(&profile.id),
        profile.name
    );
    if cfg.selected_sni_profile.is_none() {
        cfg.selected_sni_profile = Some(profile.id.clone());
    }
    cfg.sni_profiles.push(profile);
    store::save(&cfg)?;
    Ok(0)
}

fn export(rest: &[String]) -> Result<i32> {
    let a = parse(rest, &["--out"], &[])?;
    let cfg = store::load();
    let text = store::export_scrubbed(&cfg)?;
    match a.vals.get("--out") {
        Some(p) => {
            std::fs::write(p, &text)?;
            println!("exported {} server(s) (no UUIDs) to {p}", cfg.servers.len());
        }
        None => println!("{text}"),
    }
    Ok(0)
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}
