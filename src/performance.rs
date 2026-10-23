//! Bounded local-relay tuning; never changes server, TLS, MUX, DNS or routes.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceMode {
    #[default]
    Balanced,
    LowLatency,
    MaximumThroughput,
}
impl PerformanceMode {
    pub fn nodelay(self) -> bool {
        self != Self::Balanced
    }
    pub fn buffer_size(self) -> usize {
        if self == Self::MaximumThroughput {
            64 * 1024
        } else {
            8 * 1024
        }
    }
    pub fn index(self) -> i32 {
        match self {
            Self::Balanced => 0,
            Self::LowLatency => 1,
            Self::MaximumThroughput => 2,
        }
    }
    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Self::LowLatency,
            2 => Self::MaximumThroughput,
            _ => Self::Balanced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_preserve_existing_profiles() {
        let settings: crate::model::Settings =
            serde_json::from_str(r#"{"mode":"tun","default_mux_enabled":true}"#).unwrap();
        assert_eq!(settings.performance_mode, PerformanceMode::Balanced);
        assert!(settings.default_mux_enabled);
        assert_eq!(settings.mode, "tun");
        for mode in [
            PerformanceMode::Balanced,
            PerformanceMode::LowLatency,
            PerformanceMode::MaximumThroughput,
        ] {
            assert_eq!(PerformanceMode::from_index(mode.index()), mode);
            assert!((8192..=65536).contains(&mode.buffer_size()));
            let encoded = serde_json::to_string(&mode).unwrap();
            assert_eq!(
                serde_json::from_str::<PerformanceMode>(&encoded).unwrap(),
                mode
            );
        }
    }
}
