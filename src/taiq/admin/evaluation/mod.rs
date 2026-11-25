use crate::agents::adk::run_conversation;
use crate::schema::ForgeConfig;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvalSet {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub cases: Vec<EvalCase>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvalCase {
    pub id: String,
    #[serde(default)]
    pub conversation: Vec<Value>,
    #[serde(default, rename = "case-sensitive")]
    pub case_sensitive: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct EvalFile {
    evals: EvalRoot,
}
#[derive(Debug, Clone, Deserialize)]
struct EvalRoot {
    evalset: Option<EvalSet>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalCaseResult {
    pub id: String,
    pub passed: bool,
    pub answer: String,
    pub missing_tools: Vec<String>,
    pub expected_regex: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalRun {
    pub evalset_id: String,
    pub agent_id: String,
    pub passed: usize,
    pub total: usize,
    pub cases: Vec<EvalCaseResult>,
}

pub fn load_evalsets(config: &ForgeConfig, config_path: &Path) -> Result<Vec<EvalSet>> {
    let project = config_path
        .parent()
        .and_then(Path::parent)
        .context("config must be under project/config")?;
    let roots = config.extensions.get("evalsets").and_then(Value::as_array);
    let mut sets = Vec::new();
    for entry in roots.into_iter().flatten() {
        let Some(root) = entry.get("root").and_then(Value::as_str) else {
            continue;
        };
        let folder = if Path::new(root).is_absolute() {
            PathBuf::from(root)
        } else {
            project.join(root)
        };
        if !folder.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&folder)? {
            let path = file?.path();
            if !matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("yaml" | "yml")
            ) {
                continue;
            }
            let parsed: EvalFile = serde_yaml::from_slice(&std::fs::read(&path)?)
                .with_context(|| format!("parse {}", path.display()))?;
            if let Some(set) = parsed.evals.evalset {
                sets.push(set);
            }
        }
    }
    sets.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(sets)
}

fn expectations(case: &EvalCase) -> Result<(Vec<String>, Option<String>, Vec<String>)> {
    let mut prompts = Vec::new();
    let mut tools = Vec::new();
    let mut regex = None;
    for step in &case.conversation {
        if let Some(user) = step.get("user").and_then(Value::as_str) {
            prompts.push(user.to_owned());
        }
        if let Some(tool) = step.get("tool") {
            match tool {
                Value::String(name) => tools.push(name.clone()),
                Value::Array(entries) => {
                    for entry in entries {
                        if let Some(name) = entry.get("name").and_then(Value::as_str) {
                            tools.push(name.into());
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(value) = step
            .get("result")
            .and_then(|r| r.get("regex"))
            .and_then(Value::as_str)
        {
            regex = Some(value.to_owned());
        }
    }
    if prompts.is_empty() {
        bail!("evaluation case {} has no user turn", case.id);
    }
    Ok((prompts, regex, tools))
}

pub async fn run_evalset(
    set: &EvalSet,
    agent_id: &str,
    config: &ForgeConfig,
    config_path: &Path,
) -> Result<EvalRun> {
    let mut results = Vec::new();
    for case in &set.cases {
        let (prompts, expected_regex, expected_tools) = expectations(case)?;
        match run_conversation(config, config_path, agent_id, &prompts).await {
            Ok(turns) => {
                let answer = turns
                    .last()
                    .map(|turn| turn.text.clone())
                    .unwrap_or_default();
                let observed: Vec<_> = turns
                    .iter()
                    .flat_map(|turn| turn.tools.iter().cloned())
                    .collect();
                let missing_tools: Vec<_> = expected_tools
                    .into_iter()
                    .filter(|name| !observed.contains(name))
                    .collect();
                let matched = expected_regex
                    .as_deref()
                    .map(|pattern| {
                        let pattern = if case.case_sensitive == Some(false) {
                            format!("(?i){pattern}")
                        } else {
                            pattern.to_owned()
                        };
                        regex::Regex::new(&pattern).map(|regex| regex.is_match(&answer))
                    })
                    .transpose()?;
                results.push(EvalCaseResult {
                    id: case.id.clone(),
                    passed: missing_tools.is_empty() && matched.unwrap_or(true),
                    answer,
                    missing_tools,
                    expected_regex,
                    error: None,
                });
            }
            Err(error) => results.push(EvalCaseResult {
                id: case.id.clone(),
                passed: false,
                answer: String::new(),
                missing_tools: expected_tools,
                expected_regex,
                error: Some(error.to_string()),
            }),
        }
    }
    let passed = results.iter().filter(|case| case.passed).count();
    Ok(EvalRun {
        evalset_id: set.id.clone(),
        agent_id: agent_id.into(),
        passed,
        total: results.len(),
        cases: results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_existing_evalsets_without_copying() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        if !path.exists() {
            return;
        }
        let config = crate::config::load_config_from_env(&path).unwrap();
        let sets = load_evalsets(&config, &path).unwrap();
        assert!(sets.iter().any(|set| set.id == "artifact-workflow"));
        let case = &sets
            .iter()
            .find(|set| set.id == "artifact-workflow")
            .unwrap()
            .cases[0];
        let (prompts, regex, tools) = expectations(case).unwrap();
        assert_eq!(prompts.len(), 2);
        assert_eq!(regex.as_deref(), Some("(?i)chlorophyll|updated|section"));
        assert!(tools.contains(&"artifact_create".into()));
    }
}
