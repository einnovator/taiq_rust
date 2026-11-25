use crate::agents::adk::ask_once;
use crate::schema::ForgeConfig;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Schedule {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub prompt: String,
    pub kind: String,
    #[serde(default)]
    pub every: Option<u64>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub cron: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub agent_id: Option<String>,
}

fn enabled() -> bool {
    true
}

impl Schedule {
    pub fn interval(&self) -> Result<Duration> {
        if !self.enabled {
            bail!("schedule {} is disabled", self.id);
        }
        if self.id.trim().is_empty() || self.prompt.trim().is_empty() {
            bail!("schedule ID and prompt are required");
        }
        if self.kind != "every" {
            bail!("NotImplemented: schedule kind {}", self.kind);
        }
        let every = self
            .every
            .filter(|n| *n > 0)
            .ok_or_else(|| anyhow::anyhow!("every must be positive"))?;
        let seconds = match self.unit.as_deref() {
            Some("seconds") => Some(every),
            Some("minutes") => every.checked_mul(60),
            Some("hours") => every.checked_mul(3600),
            Some("days") => every.checked_mul(86400),
            Some("weeks") => every.checked_mul(604800),
            _ => bail!("invalid schedule unit"),
        };
        let seconds = seconds.ok_or_else(|| anyhow::anyhow!("schedule interval is too large"))?;
        Ok(Duration::from_secs(seconds))
    }
}

pub async fn run_once(
    schedule: &Schedule,
    config: &ForgeConfig,
    config_path: &Path,
    default_agent: &str,
) -> Result<Value> {
    schedule.interval()?;
    let agent = schedule.agent_id.as_deref().unwrap_or(default_agent);
    let output = ask_once(config, config_path, agent, &schedule.prompt).await?;
    Ok(json!({"schedule_id":schedule.id,"agent_id":agent,"output":output}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_interval() {
        let schedule = Schedule {
            id: "daily".into(),
            name: None,
            prompt: "hello".into(),
            kind: "every".into(),
            every: Some(2),
            unit: Some("days".into()),
            cron: None,
            timezone: None,
            enabled: true,
            agent_id: None,
        };
        assert_eq!(schedule.interval().unwrap(), Duration::from_secs(172800));
    }
}
