use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub validity_mode: ValidityMode,
    pub max_depth: u32,
    pub max_total_nodes: u32,
    pub repetition_bounds: (u32, u32),
    pub dictionary: Vec<Vec<u8>>,
    pub havoc_intensity: f32,
    pub rule_overrides: HashMap<String, RuleOverride>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidityMode {
    Strict,
    NearValid,
    Havoc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleOverride {
    pub weight: Option<f32>,
    pub max_depth: Option<u32>,
    pub repetition_bounds: Option<(u32, u32)>,
    pub dictionary: Option<Vec<Vec<u8>>>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            validity_mode: ValidityMode::Strict,
            max_depth: 10,
            max_total_nodes: 10_000,
            repetition_bounds: (0, 5),
            dictionary: Vec::new(),
            havoc_intensity: 0.5,
            rule_overrides: HashMap::new(),
        }
    }
}

impl Profile {
    pub fn builder() -> ProfileBuilder {
        ProfileBuilder(Profile::default())
    }
}

pub struct ProfileBuilder(Profile);

impl ProfileBuilder {
    pub fn validity_mode(mut self, mode: ValidityMode) -> Self {
        self.0.validity_mode = mode;
        self
    }

    pub fn max_depth(mut self, depth: u32) -> Self {
        self.0.max_depth = depth;
        self
    }

    pub fn max_total_nodes(mut self, nodes: u32) -> Self {
        self.0.max_total_nodes = nodes;
        self
    }

    pub fn repetition_bounds(mut self, min: u32, max: u32) -> Self {
        self.0.repetition_bounds = (min, max);
        self
    }

    pub fn dictionary(mut self, dict: Vec<Vec<u8>>) -> Self {
        self.0.dictionary = dict;
        self
    }

    pub fn havoc_intensity(mut self, intensity: f32) -> Self {
        self.0.havoc_intensity = intensity;
        self
    }

    pub fn rule_override(mut self, name: impl Into<String>, ov: RuleOverride) -> Self {
        self.0.rule_overrides.insert(name.into(), ov);
        self
    }

    pub fn build(self) -> Profile {
        self.0
    }
}
