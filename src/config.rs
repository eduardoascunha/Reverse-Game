use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const CONFIG_PATH: &str = "./config/config.toml";

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub max_seconds: u32,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_seconds: 10,
            input_device: None,
            output_device: None,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = Path::new(CONFIG_PATH);
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()));
            }
        };

        let config: Self = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.max_seconds == 0 {
            bail!("config error: max_seconds must be at least 1");
        }

        Ok(())
    }
}
