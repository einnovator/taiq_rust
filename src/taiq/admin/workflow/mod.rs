use crate::schema::ForgeConfig;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub connections: Vec<WorkflowConnection>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub name: String,
    pub category: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub parameters: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConnection {
    pub source: String,
    pub target: String,
    #[serde(default = "main_port")]
    pub source_port: String,
}

fn main_port() -> String {
    "main".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeTrace {
    pub node_id: String,
    pub status: String,
    pub output: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub status: String,
    pub outputs: Vec<Value>,
    pub trace: Vec<NodeTrace>,
}

impl Workflow {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            bail!("workflow ID and name are required");
        }
        let nodes: HashMap<_, _> = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        if nodes.len() != self.nodes.len() {
            bail!("workflow node IDs must be unique");
        }
        let valid = |node: &WorkflowNode| match node.category.as_str() {
            "trigger" => matches!(node.kind.as_str(), "manual" | "schedule" | "connector"),
            "agent" | "team" => node.kind == "run",
            "control" => matches!(
                node.kind.as_str(),
                "if" | "switch" | "merge" | "wait" | "stop" | "noop"
            ),
            "input" => matches!(node.kind.as_str(), "literal" | "connector"),
            "output" => matches!(node.kind.as_str(), "result" | "connector"),
            _ => false,
        };
        for node in &self.nodes {
            if node.id.trim().is_empty() || !valid(node) {
                bail!(
                    "invalid workflow node {}: {}/{}",
                    node.id,
                    node.category,
                    node.kind
                );
            }
            if node.category == "agent"
                && node
                    .parameters
                    .get("agent_id")
                    .and_then(Value::as_str)
                    .is_none()
            {
                bail!("{}: agent_id is required", node.id);
            }
            if node.category == "control" && node.kind == "wait" {
                let seconds = node
                    .parameters
                    .get("seconds")
                    .and_then(Value::as_f64)
                    .context("wait seconds are required")?;
                if !(0.0..=86400.0).contains(&seconds) {
                    bail!("wait seconds must be between 0 and 86400");
                }
            }
        }
        let mut edge_keys = HashSet::new();
        for edge in &self.connections {
            let source = nodes
                .get(edge.source.as_str())
                .context("workflow edge source is missing")?;
            let target = nodes
                .get(edge.target.as_str())
                .context("workflow edge target is missing")?;
            if edge.source == edge.target
                || target.category == "trigger"
                || source.category == "output"
                || source.kind == "stop"
            {
                bail!("invalid workflow edge {} -> {}", edge.source, edge.target);
            }
            if !edge_keys.insert((&edge.source, &edge.source_port, &edge.target)) {
                bail!("duplicate workflow edge");
            }
        }
        self.topological_order()?;
        Ok(())
    }

    fn topological_order(&self) -> Result<Vec<&WorkflowNode>> {
        let mut ordered = Vec::new();
        let mut remaining: HashSet<&str> = self.nodes.iter().map(|n| n.id.as_str()).collect();
        while !remaining.is_empty() {
            let ready: Vec<_> = self
                .nodes
                .iter()
                .filter(|node| {
                    remaining.contains(node.id.as_str())
                        && self
                            .connections
                            .iter()
                            .filter(|edge| edge.target == node.id)
                            .all(|edge| ordered.iter().any(|id: &&str| *id == edge.source))
                })
                .collect();
            if ready.is_empty() {
                bail!("workflow connections contain a cycle");
            }
            for node in ready {
                remaining.remove(node.id.as_str());
                ordered.push(node.id.as_str());
            }
        }
        Ok(ordered
            .into_iter()
            .map(|id| self.nodes.iter().find(|node| node.id == id).unwrap())
            .collect())
    }
}

fn field<'a>(value: &'a Value, path: &str) -> &'a Value {
    if matches!(path, "" | "$" | "input") {
        return value;
    }
    let mut current = value;
    for part in path.strip_prefix("input.").unwrap_or(path).split('.') {
        current = match current {
            Value::Object(map) => map.get(part).unwrap_or(&Value::Null),
            Value::Array(items) => part
                .parse::<usize>()
                .ok()
                .and_then(|i| items.get(i))
                .unwrap_or(&Value::Null),
            _ => &Value::Null,
        };
    }
    current
}

fn compare(actual: &Value, operator: &str, expected: &Value) -> bool {
    match operator {
        "eq" => actual == expected,
        "ne" => actual != expected,
        "contains" => {
            actual
                .as_str()
                .zip(expected.as_str())
                .is_some_and(|(a, b)| a.contains(b))
                || actual
                    .as_array()
                    .is_some_and(|values| values.contains(expected))
        }
        "gt" | "gte" | "lt" | "lte" => {
            let number = |value: &Value| value.as_f64().or_else(|| value.as_str()?.parse().ok());
            match (number(actual), number(expected)) {
                (Some(a), Some(b)) => match operator {
                    "gt" => a > b,
                    "gte" => a >= b,
                    "lt" => a < b,
                    _ => a <= b,
                },
                _ => false,
            }
        }
        _ => false,
    }
}

pub async fn run_manual(
    workflow: &Workflow,
    value: Value,
    config: &ForgeConfig,
    config_path: &Path,
) -> Result<WorkflowRun> {
    workflow.validate()?;
    if !workflow.enabled {
        bail!("workflow is disabled");
    }
    let starts: Vec<_> = workflow
        .nodes
        .iter()
        .filter(|n| n.category == "trigger" && n.kind == "manual")
        .collect();
    if starts.len() != 1 {
        bail!("manual runs require exactly one manual trigger");
    }
    let start = &starts[0].id;
    let mut propagated: HashMap<(String, String, String), Value> = HashMap::new();
    let mut outputs = Vec::new();
    let mut trace = Vec::new();
    for node in workflow.topological_order()? {
        let active: Vec<_> = workflow
            .connections
            .iter()
            .filter(|e| e.target == node.id)
            .filter_map(|e| {
                propagated.get(&(e.source.clone(), e.source_port.clone(), e.target.clone()))
            })
            .collect();
        if &node.id != start && active.is_empty() {
            continue;
        }
        if node.category == "trigger" && &node.id != start {
            continue;
        }
        let incoming = if &node.id == start {
            value.clone()
        } else if node.kind == "merge" {
            Value::Array(active.into_iter().cloned().collect())
        } else {
            (*active.last().unwrap()).clone()
        };
        let params = &node.parameters;
        let (result, port) = match (node.category.as_str(), node.kind.as_str()) {
            ("trigger", _) => (incoming, "main".to_owned()),
            ("input", "literal") => (
                params.get("value").cloned().unwrap_or(Value::Null),
                "main".into(),
            ),
            ("agent", "run") => {
                let agent_id = params
                    .get("agent_id")
                    .and_then(Value::as_str)
                    .context("agent_id is required")?;
                let prompt = params
                    .get("prompt")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| incoming.to_string());
                (
                    Value::String(
                        crate::agents::adk::ask_once(config, config_path, agent_id, &prompt)
                            .await?,
                    ),
                    "main".into(),
                )
            }
            ("control", "if") => {
                let actual = field(
                    &incoming,
                    params.get("field").and_then(Value::as_str).unwrap_or(""),
                );
                let expected = params.get("value").unwrap_or(&Value::Null);
                let op = params
                    .get("operator")
                    .and_then(Value::as_str)
                    .unwrap_or("eq");
                let passed = compare(actual, op, expected);
                (incoming, if passed { "true" } else { "false" }.into())
            }
            ("control", "switch") => {
                let actual = field(
                    &incoming,
                    params.get("field").and_then(Value::as_str).unwrap_or(""),
                );
                let port = params
                    .get("cases")
                    .and_then(Value::as_object)
                    .and_then(|cases| {
                        cases
                            .iter()
                            .find(|(_, v)| *v == actual)
                            .map(|(k, _)| k.clone())
                    })
                    .unwrap_or_else(|| "default".into());
                (incoming, port)
            }
            ("control", "wait") => {
                let seconds = params.get("seconds").and_then(Value::as_f64).unwrap_or(0.0);
                tokio::time::sleep(std::time::Duration::from_secs_f64(seconds)).await;
                (incoming, "main".into())
            }
            ("input" | "output", "connector") => bail!(
                "connector workflow node {} requires a configured Rust connector",
                node.id
            ),
            ("team", "run") => bail!(
                "team workflow node {} is not configured for Rust execution",
                node.id
            ),
            _ => (incoming, "main".into()),
        };
        if node.category == "output" {
            outputs.push(result.clone());
        }
        trace.push(NodeTrace {
            node_id: node.id.clone(),
            status: "completed".into(),
            output: result.clone(),
        });
        for edge in workflow
            .connections
            .iter()
            .filter(|e| e.source == node.id && e.source_port == port)
        {
            propagated.insert(
                (
                    edge.source.clone(),
                    edge.source_port.clone(),
                    edge.target.clone(),
                ),
                result.clone(),
            );
        }
    }
    Ok(WorkflowRun {
        status: "completed".into(),
        outputs,
        trace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    async fn branches_and_returns_result() {
        let workflow: Workflow = serde_json::from_value(json!({"id":"sample","name":"Sample","nodes":[
            {"id":"start","name":"Start","category":"trigger","type":"manual"},
            {"id":"check","name":"Check","category":"control","type":"if","parameters":{"field":"input.score","operator":"gte","value":5}},
            {"id":"yes","name":"Yes","category":"output","type":"result"},
            {"id":"no","name":"No","category":"output","type":"result"}],
            "connections":[{"source":"start","target":"check"},{"source":"check","source_port":"true","target":"yes"},{"source":"check","source_port":"false","target":"no"}]
        })).unwrap();
        let result = run_manual(
            &workflow,
            json!({"score":7}),
            &ForgeConfig::default(),
            Path::new("test"),
        )
        .await
        .unwrap();
        assert_eq!(result.outputs, vec![json!({"score":7})]);
        assert_eq!(result.trace.len(), 3);
    }
}
