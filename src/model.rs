use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Operator {
    pub wording: String,
    #[serde(rename = "precondition")]
    pub _precondition: String,
    #[serde(rename = "stance")]
    pub _stance: String,
}

pub type Operators = HashMap<String, Operator>;

#[derive(Debug, Deserialize)]
pub struct Composition {
    pub name: String,
    #[serde(rename = "description")]
    pub _description: String,
    pub stage: Vec<Stage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stage {
    pub id: String,
    pub kind: String,
    pub operator: String,
    pub n: Option<usize>,
    #[serde(default)]
    pub quarantine: bool,
    #[serde(default)]
    pub inputs: Vec<String>,
    pub max_rounds: Option<usize>,
    pub stop_sentinel: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HarnessConfig {
    pub default: String,
    pub fanout: Vec<String>,
    pub harness: HashMap<String, Harness>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Harness {
    pub cmd: Vec<String>,
    pub prompt: PromptMode,
    pub timeout_secs: u64,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub capture: CaptureMode,
    #[serde(default = "default_min_output_bytes")]
    pub min_output_bytes: usize,
}

fn default_min_output_bytes() -> usize {
    1
}

/// How to extract the deliverable from harness stdout.
/// `text`: stdout is the deliverable. `stream-json`: stdout is Claude Code's
/// stream-json event log; the deliverable is every assistant text block,
/// concatenated — this recovers text the model wrote before a trailing tool
/// call, which plain `claude -p` drops from its final-message output.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureMode {
    #[default]
    Text,
    StreamJson,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptMode {
    Arg,
    Stdin,
}

#[derive(Debug, Default)]
pub struct State {
    pub ask: Option<String>,
    pub objective: Option<String>,
    pub constraints: String,
    pub plan: Option<String>,
    pub outputs: HashMap<String, Vec<Candidate>>,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub text: String,
    pub stage: String,
    pub index: usize,
    pub harness: String,
}

#[derive(Debug)]
pub struct RunRecord {
    pub run_id: String,
    pub composition: String,
    pub ask: String,
    pub stage_order: Vec<String>,
    pub calls: Vec<CallRecord>,
    pub loop_stops: Vec<LoopStop>,
    pub grades: Vec<GradeRecord>,
    pub outcome: String,
}

#[derive(Debug)]
pub struct CallRecord {
    pub stage: String,
    pub operator: String,
    pub harness: String,
    pub duration_secs: f64,
    pub bytes_out: usize,
    pub outcome: String,
}

#[derive(Debug)]
pub struct GradeRecord {
    pub stage: String,
    pub output: String,
}

#[derive(Debug)]
pub struct LoopStop {
    pub stage: String,
    pub round: usize,
    pub reason: String,
}
