//! Append-only imports shared by the GUI and CLI. Imported IDs never identify
//! an existing row, even when importing the same export more than once.
use crate::model::{AppConfig, ServerProfile};

pub fn append(cfg: &mut AppConfig, profiles: Vec<ServerProfile>) -> usize {
    let count = profiles.len();
    for mut profile in profiles {
        profile.id = uuid::Uuid::new_v4().to_string();
        let base = if profile.name.trim().is_empty() {
            format!("{}:{}", profile.address, profile.port)
        } else {
            profile.name.trim().to_string()
        };
        profile.name = base.clone();
        let mut suffix = 2;
        while cfg.servers.iter().any(|p| p.name == profile.name) {
            profile.name = format!("{base} ({suffix})");
            suffix += 1;
        }
        if cfg.selected_server.is_none() {
            cfg.selected_server = Some(profile.id.clone());
        }
        cfg.servers.push(profile);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeat_import_is_append_only_even_with_the_same_id_name_and_credentials() {
        let existing = ServerProfile {
            name: "Example".into(),
            ..Default::default()
        };
        let mut cfg = AppConfig {
            servers: vec![existing.clone()],
            selected_server: Some(existing.id.clone()),
            ..Default::default()
        };
        append(&mut cfg, vec![existing.clone(), existing.clone()]);
        assert_eq!(cfg.servers[0], existing);
        assert_eq!(cfg.selected_server.as_deref(), Some(existing.id.as_str()));
        assert_eq!(cfg.servers.len(), 3);
        assert_eq!(cfg.servers[1].name, "Example (2)");
        assert_eq!(cfg.servers[2].name, "Example (3)");
        let ids: std::collections::HashSet<_> = cfg.servers.iter().map(|s| &s.id).collect();
        assert_eq!(ids.len(), 3);
    }
}
