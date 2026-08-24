use crate::model::*;
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CONTEXT_DEFAULT: &str = "Inspect the current repository state yourself before answering.";
const LEDGER_HEADER: &str = "ts\trun_id\tcomposition\tstage\toperator\tharness\tduration_secs\tbytes_out\toutcome\n";
const VARIABLES: [&str; 10] = [
    "ask", "objective", "constraints", "plan", "candidates", "n", "claim", "context", "x", "y",
];

pub struct Loaded {
    pub operators: Operators,
    pub composition: Composition,
    pub harnesses: HarnessConfig,
}

pub fn load(composition_name: &str) -> Result<Loaded> {
    let root = asset_root()?;
    let operator_path = root.join("arsenal/operators.toml");
    let composition_path = root.join("compositions").join(format!("{composition_name}.toml"));
    let harness_path = harness_path()?;
    let operators: Operators = read_toml(&operator_path)?;
    let composition: Composition = read_toml(&composition_path)?;
    if composition.name != composition_name {
        bail!("{}: composition name '{}' does not match requested '{}'", composition_path.display(), composition.name, composition_name);
    }
    let harnesses: HarnessConfig = read_toml(&harness_path)?;
    validate(&composition, &operators, &harnesses, &composition_path, &harness_path)?;
    Ok(Loaded { operators, composition, harnesses })
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn asset_root() -> Result<PathBuf> {
    let cwd = env::current_dir().context("resolve current directory")?;
    let root = if let Some(home) = env::var_os("DAKKA_HOME") {
        PathBuf::from(home)
    } else if cwd.join("arsenal").is_dir() && cwd.join("compositions").is_dir() {
        cwd
    } else {
        config_dir()?
    };
    if !root.join("arsenal").is_dir() || !root.join("compositions").is_dir() {
        bail!("dakka data not found under {} (need arsenal/ and compositions/)", root.display());
    }
    Ok(root)
}

fn harness_path() -> Result<PathBuf> {
    let local = env::current_dir().context("resolve current directory")?.join("harnesses.toml");
    if local.is_file() {
        return Ok(local);
    }
    let configured = config_dir()?.join("harnesses.toml");
    if configured.is_file() {
        return Ok(configured);
    }
    bail!(
        "missing harnesses.toml; copy harnesses.toml.example to {} or {}",
        local.display(),
        configured.display()
    )
}

fn config_dir() -> Result<PathBuf> {
    let home = env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set; cannot locate ~/.config/dakka"))?;
    Ok(PathBuf::from(home).join(".config/dakka"))
}

fn validate(
    composition: &Composition,
    operators: &Operators,
    harnesses: &HarnessConfig,
    composition_path: &Path,
    harness_path: &Path,
) -> Result<()> {
    if composition.name.is_empty() || composition.stage.is_empty() {
        bail!("{}: composition name and stages must be nonempty", composition_path.display());
    }
    let mut seen = HashMap::new();
    for stage in &composition.stage {
        if seen.contains_key(stage.id.as_str()) {
            bail!("{}: duplicate stage id '{}'", composition_path.display(), stage.id);
        }
        if !operators.contains_key(&stage.operator) {
            bail!("{}: stage '{}' references unknown operator '{}'", composition_path.display(), stage.id, stage.operator);
        }
        match stage.kind.as_str() {
            "single" => {}
            "fanout" => {
                if stage.n == Some(0) {
                    bail!("{}: fanout stage '{}' has n = 0", composition_path.display(), stage.id);
                }
            }
            "judge" => {
                if stage.inputs.is_empty() {
                    bail!("{}: judge stage '{}' has no inputs", composition_path.display(), stage.id);
                }
            }
            "loop" => {
                if stage.max_rounds == Some(0) || stage.max_rounds.is_none() || stage.stop_sentinel.as_deref().unwrap_or("").is_empty() {
                    bail!("{}: loop stage '{}' needs positive max_rounds and stop_sentinel", composition_path.display(), stage.id);
                }
            }
            other => bail!("{}: stage '{}' has unknown kind '{}'", composition_path.display(), stage.id, other),
        }
        for input in &stage.inputs {
            match seen.get(input.as_str()) {
                Some(&"fanout") => {}
                Some(kind) => bail!("{}: stage '{}' input '{}' is a {}, not a fanout", composition_path.display(), stage.id, input, kind),
                None => bail!("{}: stage '{}' input '{}' must name an earlier stage", composition_path.display(), stage.id, input),
            }
        }
        if stage.operator == "route-contested" && !operators.contains_key("adjudicate") {
            bail!("{}: stage '{}' requires missing operator 'adjudicate'", composition_path.display(), stage.id);
        }
        seen.insert(stage.id.as_str(), stage.kind.as_str());
    }
    if harnesses.fanout.is_empty() && composition.stage.iter().any(|s| s.kind == "fanout") {
        bail!("{}: fanout pool is empty", harness_path.display());
    }
    let mut referenced = harnesses.fanout.clone();
    referenced.push(harnesses.default.clone());
    for name in referenced {
        let harness = harnesses.harness.get(&name).ok_or_else(|| anyhow!("{}: unknown harness '{}'", harness_path.display(), name))?;
        if harness.cmd.is_empty() {
            bail!("{}: harness '{}' has an empty cmd", harness_path.display(), name);
        }
        if harness.timeout_secs == 0 {
            bail!("{}: harness '{}' has timeout_secs = 0", harness_path.display(), name);
        }
    }
    Ok(())
}

pub fn doctor() -> Result<bool> {
    let path = harness_path()?;
    let config: HarnessConfig = read_toml(&path)?;
    println!("default: {}", config.default);
    println!("fanout: {}", config.fanout.join(", "));
    let mut names: Vec<_> = config.harness.keys().collect();
    names.sort();
    for name in names {
        let command = config.harness[name].cmd.first().ok_or_else(|| anyhow!("{}: harness '{}' has an empty cmd", path.display(), name))?;
        println!("{name}: {} ({command})", if resolve_command(command).is_some() { "found" } else { "missing" });
    }
    let default_found = config
        .harness
        .get(&config.default)
        .and_then(|h| h.cmd.first())
        .and_then(|cmd| resolve_command(cmd))
        .is_some();
    Ok(default_found)
}

fn resolve_command(command: &str) -> Option<PathBuf> {
    let candidate = Path::new(command);
    if candidate.components().count() > 1 {
        return is_executable(candidate).then(|| candidate.to_path_buf());
    }
    env::split_paths(&env::var_os("PATH")?).map(|dir| dir.join(command)).find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return fs::metadata(path).map(|metadata| metadata.permissions().mode() & 0o111 != 0).unwrap_or(false);
    }
    #[cfg(not(unix))]
    true
}

fn render(operator: &Operator, state: &State, extras: &HashMap<&str, String>, live: bool) -> Result<String> {
    // Resolution is decided against the TEMPLATE, never the rendered text:
    // substituted content (a plan about dakka, say) may legitimately contain
    // literal "{plan}" tokens.
    let template = &operator.wording;
    let mut values: HashMap<&str, &str> = HashMap::new();
    values.insert("constraints", state.constraints.as_str());
    values.insert("context", CONTEXT_DEFAULT);
    if let Some(ask) = state.ask.as_deref() {
        values.insert("ask", ask);
    }
    if let Some(objective) = state.objective.as_deref() {
        values.insert("objective", objective);
    }
    if let Some(plan) = state.plan.as_deref() {
        values.insert("plan", plan);
    }
    for (name, value) in extras {
        values.insert(name, value.as_str());
    }
    let mut unresolved = Vec::new();
    let mut payload = String::with_capacity(template.len());
    let mut rest = template.as_str();
    while let Some(open) = rest.find('{') {
        payload.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if VARIABLES.contains(&&after[..close]) => {
                let name = &after[..close];
                match values.get(name) {
                    Some(value) => payload.push_str(value),
                    None => {
                        unresolved.push(name);
                        payload.push_str(&rest[open..open + close + 2]);
                    }
                }
                rest = &after[close + 1..];
            }
            _ => {
                payload.push('{');
                rest = after;
            }
        }
    }
    payload.push_str(rest);
    if live && !unresolved.is_empty() {
        bail!("operator payload has unresolved variables: {}", unresolved.join(", "));
    }
    Ok(payload)
}

pub fn pack(loaded: &Loaded, ask: Option<String>, plan: Option<String>, n_override: Option<usize>) -> Result<()> {
    let mut state = State { ask, plan, ..State::default() };
    let mut counts = HashMap::new();
    for stage in &loaded.composition.stage {
        let count = if stage.kind == "fanout" {
            n_override.or(stage.n).unwrap_or(1)
        } else {
            stage.inputs.iter().filter_map(|input| counts.get(input)).sum()
        };
        let mut extras = HashMap::new();
        if count > 0 {
            extras.insert("n", count.to_string());
        }
        let operator = &loaded.operators[&stage.operator];
        println!("=== {} [{}: {}] ===", stage.id, stage.kind, stage.operator);
        println!("{}", render(operator, &state, &extras, false)?);
        counts.insert(stage.id.clone(), count);
        if stage.id == "bind" {
            state.objective = None;
        }
    }
    Ok(())
}

struct RawCall {
    output: String,
    duration_secs: f64,
    bytes_out: usize,
}

pub struct Runner<'a> {
    loaded: &'a Loaded,
    run_dir: PathBuf,
    run_id: String,
    record: RunRecord,
    ledger_path: PathBuf,
}

impl<'a> Runner<'a> {
    pub fn new(loaded: &'a Loaded, composition: &str, ask: &str) -> Result<Self> {
        let cwd = env::current_dir().context("resolve current directory")?;
        let dakka_dir = cwd.join(".dakka");
        fs::create_dir_all(dakka_dir.join("runs")).with_context(|| format!("create {}", dakka_dir.display()))?;
        let (run_id, run_dir) = unique_run_dir(&dakka_dir.join("runs"))?;
        println!("run dir: {}", run_dir.display());
        let record = RunRecord {
            run_id: run_id.clone(),
            composition: composition.to_owned(),
            ask: ask.to_owned(),
            stage_order: Vec::new(),
            calls: Vec::new(),
            loop_stops: Vec::new(),
            grades: Vec::new(),
            outcome: "running".to_owned(),
        };
        Ok(Self { loaded, run_dir, run_id, record, ledger_path: dakka_dir.join("ledger.tsv") })
    }

    pub fn run(mut self, mut state: State, n_override: Option<usize>) -> Result<(String, PathBuf)> {
        let result = (|| {
            for stage in &self.loaded.composition.stage {
                self.record.stage_order.push(stage.id.clone());
                match stage.kind.as_str() {
                    "single" => self.run_single(stage, &mut state)?,
                    "fanout" => self.run_fanout(stage, &mut state, n_override)?,
                    "judge" => self.run_judge(stage, &mut state)?,
                    "loop" => self.run_loop(stage, &mut state)?,
                    _ => unreachable!("validated stage kind"),
                }
            }
            state.plan.ok_or_else(|| anyhow!("composition '{}' completed without producing a plan", self.loaded.composition.name))
        })();
        match result {
            Ok(plan) => {
                self.record.outcome = "ok".to_owned();
                self.write_record()?;
                Ok((plan, self.run_dir))
            }
            Err(error) => {
                self.record.outcome = "error".to_owned();
                self.write_record().with_context(|| format!("after run failure: {error:#}"))?;
                Err(error)
            }
        }
    }

    fn run_single(&mut self, stage: &Stage, state: &mut State) -> Result<()> {
        let extras = self.input_extras(stage, state)?;
        let payload = render(&self.loaded.operators[&stage.operator], state, &extras, true)
            .with_context(|| format!("render stage '{}'", stage.id))?;
        let raw = self.execute(stage, &stage.id, &stage.operator, &self.loaded.harnesses.default, &payload, false)?;
        self.accept(stage, &self.loaded.harnesses.default, &raw, "ok")?;
        let output = raw.output;
        if stage.id == "bind" {
            state.objective = Some(output);
        } else if stage.id == "questions" || stage.operator == "human-questions" {
            state.plan = Some(output);
            self.snapshot(stage, state)?;
        } else if stage.operator == "route-contested" {
            append_text(&mut state.constraints, &output);
            self.adjudicate(stage, state, &output)?;
        } else if stage.operator == "grade-assumptions" {
            self.record.grades.push(GradeRecord { stage: stage.id.clone(), output });
            self.write_record()?;
        } else if self.loaded.operators[&stage.operator].wording.contains("{plan}") {
            state.plan = Some(output);
            self.snapshot(stage, state)?;
        } else {
            append_text(&mut state.constraints, &output);
        }
        Ok(())
    }

    fn adjudicate(&mut self, stage: &Stage, state: &mut State, routed: &str) -> Result<()> {
        let claims: Vec<_> = routed.lines().filter_map(|line| line.strip_prefix("CONTESTED: ")).collect();
        if claims.len() > 8 {
            eprintln!("stage '{}': capped {} contested claims at 8", stage.id, claims.len());
        }
        let mut rulings = Vec::new();
        for (index, claim) in claims.into_iter().take(8).enumerate() {
            let extras = HashMap::from([("claim", claim.to_owned())]);
            let payload = render(&self.loaded.operators["adjudicate"], state, &extras, true)
                .with_context(|| format!("render adjudication {} for stage '{}'", index + 1, stage.id))?;
            let artifact = format!("{}-adjudicate-{}", stage.id, index + 1);
            let raw = self.execute(stage, &artifact, "adjudicate", &self.loaded.harnesses.default, &payload, false)?;
            self.accept_named(&artifact, "adjudicate", &self.loaded.harnesses.default, &raw, "ok")?;
            rulings.push(raw.output);
        }
        if !rulings.is_empty() {
            let plan = state.plan.get_or_insert_with(String::new);
            plan.push_str("\n\n## Disagreement log\n\n");
            plan.push_str(&rulings.join("\n\n"));
            plan.push('\n');
            self.snapshot(stage, state)?;
        }
        Ok(())
    }

    fn run_fanout(&mut self, stage: &Stage, state: &mut State, n_override: Option<usize>) -> Result<()> {
        let n = n_override.or(stage.n).unwrap_or(1);
        if n == 0 {
            bail!("stage '{}': fanout count must be positive", stage.id);
        }
        let pool = &self.loaded.harnesses.fanout;
        let extras = HashMap::from([("n", n.to_string())]);
        let payload = render(&self.loaded.operators[&stage.operator], state, &extras, true)
            .with_context(|| format!("render stage '{}'", stage.id))?;
        let mut walkers = Vec::with_capacity(n);
        for index in 0..n {
            let harness = pool[index % pool.len()].as_str();
            let prepared = self.prepare(stage, &format!("{}-{}", stage.id, index + 1), harness, &payload, stage.quarantine)?;
            walkers.push(prepared);
        }
        // Walkers run concurrently — that is what a fan-out is. Ledger and
        // record writes happen after the join, in walker order.
        let results: Vec<(Result<ProcessResult>, f64)> = thread::scope(|scope| {
            let handles: Vec<_> = walkers
                .iter()
                .map(|prepared| {
                    let harness = &self.loaded.harnesses.harness[&prepared.harness_name];
                    let (payload, temp) = (payload.as_str(), prepared.temp.as_deref());
                    scope.spawn(move || timed_invoke(harness, payload, temp))
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().map_err(|_| anyhow!("stage '{}': walker thread panicked", stage.id)))
                .collect::<Result<Vec<_>>>()
        })?;
        let mut outputs = Vec::with_capacity(n);
        for (index, (prepared, (result, duration))) in walkers.iter().zip(results).enumerate() {
            let raw = self.finish(stage, prepared, &stage.operator, result, duration)?;
            self.accept_named(&prepared.artifact, &stage.operator, &prepared.harness_name, &raw, "ok")?;
            outputs.push(Candidate { text: raw.output, stage: stage.id.clone(), index, harness: prepared.harness_name.clone() });
        }
        state.outputs.insert(stage.id.clone(), outputs);
        Ok(())
    }

    fn run_judge(&mut self, stage: &Stage, state: &mut State) -> Result<()> {
        let mut candidates = self.candidates(stage, state)?;
        if candidates.len() < 2 || candidates.len() > 26 {
            bail!("stage '{}': judge needs 2..=26 candidates, got {}", stage.id, candidates.len());
        }
        candidates.shuffle(&mut rand::thread_rng());
        let (blocks, mapping) = label_candidates(&candidates);
        fs::write(self.run_dir.join("mapping.json"), mapping)
            .with_context(|| format!("write {}/mapping.json", self.run_dir.display()))?;
        let extras = HashMap::from([("n", candidates.len().to_string()), ("candidates", blocks)]);
        let payload = render(&self.loaded.operators[&stage.operator], state, &extras, true)
            .with_context(|| format!("render stage '{}'", stage.id))?;
        let raw = self.execute(stage, &stage.id, &stage.operator, &self.loaded.harnesses.default, &payload, false)?;
        self.accept(stage, &self.loaded.harnesses.default, &raw, "ok")?;
        let winner = parse_winner(&raw.output).with_context(|| format!("stage '{}': judge output must contain one line 'WINNER: <letter>'", stage.id))?;
        let index = (winner as u8 - b'A') as usize;
        let chosen = candidates.get(index).ok_or_else(|| anyhow!("stage '{}': judge chose {}, but only {} candidates exist", stage.id, winner, candidates.len()))?;
        state.plan = Some(chosen.text.clone());
        append_text(&mut state.constraints, &raw.output);
        self.snapshot(stage, state)?;
        Ok(())
    }

    fn run_loop(&mut self, stage: &Stage, state: &mut State) -> Result<()> {
        let rounds = stage.max_rounds.expect("validated max_rounds");
        let sentinel = stage.stop_sentinel.as_deref().expect("validated sentinel");
        for round in 1..=rounds {
            let payload = render(&self.loaded.operators[&stage.operator], state, &HashMap::new(), true)
                .with_context(|| format!("render stage '{}' round {}", stage.id, round))?;
            let artifact = format!("{}-{}", stage.id, round);
            let raw = self.execute(stage, &artifact, &stage.operator, &self.loaded.harnesses.default, &payload, false)?;
            let hit = raw.output.contains(sentinel);
            let outcome = if hit { format!("fixpoint-round-{round}") } else { "ok".to_owned() };
            self.accept_named(&artifact, &stage.operator, &self.loaded.harnesses.default, &raw, &outcome)?;
            state.plan = Some(strip_sentinel_line(&raw.output, sentinel));
            self.snapshot(stage, state)?;
            if hit {
                self.record.loop_stops.push(LoopStop { stage: stage.id.clone(), round, reason: "sentinel".to_owned() });
                return Ok(());
            }
        }
        self.record.loop_stops.push(LoopStop { stage: stage.id.clone(), round: rounds, reason: "max_rounds".to_owned() });
        Ok(())
    }

    fn input_extras(&self, stage: &Stage, state: &State) -> Result<HashMap<&'static str, String>> {
        if stage.inputs.is_empty() {
            return Ok(HashMap::new());
        }
        let candidates = self.candidates(stage, state)?;
        let blocks = candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| format!("Candidate {}:\n{}", index + 1, candidate.text))
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(HashMap::from([("n", candidates.len().to_string()), ("candidates", blocks)]))
    }

    fn candidates(&self, stage: &Stage, state: &State) -> Result<Vec<Candidate>> {
        let mut candidates = Vec::new();
        for input in &stage.inputs {
            let values = state.outputs.get(input).ok_or_else(|| anyhow!("stage '{}': input '{}' has no output vector", stage.id, input))?;
            candidates.extend(values.iter().cloned());
        }
        Ok(candidates)
    }

    fn execute(
        &mut self,
        stage: &Stage,
        artifact: &str,
        operator: &str,
        harness_name: &str,
        payload: &str,
        quarantine: bool,
    ) -> Result<RawCall> {
        let prepared = self.prepare(stage, artifact, harness_name, payload, quarantine)?;
        let harness = &self.loaded.harnesses.harness[harness_name];
        let (result, duration) = timed_invoke(harness, payload, prepared.temp.as_deref());
        self.finish(stage, &prepared, operator, result, duration)
    }

    /// Write the payload artifact and create the quarantine dir; no spawn yet.
    fn prepare(&self, stage: &Stage, artifact: &str, harness_name: &str, payload: &str, quarantine: bool) -> Result<Prepared> {
        let payload_path = self.run_dir.join(format!("{artifact}-payload.txt"));
        let output_path = self.run_dir.join(format!("{artifact}-output.txt"));
        fs::write(&payload_path, payload).with_context(|| format!("stage '{}': write {}", stage.id, payload_path.display()))?;
        let temp = quarantine.then(|| env::temp_dir().join(format!("dakka-{}-{artifact}", self.run_id)));
        if let Some(path) = &temp {
            fs::create_dir(path).with_context(|| format!("stage '{}': create quarantine {}", stage.id, path.display()))?;
        }
        Ok(Prepared { artifact: artifact.to_owned(), harness_name: harness_name.to_owned(), payload_path, output_path, temp })
    }

    /// Record the outcome of an invocation: artifacts, ledger, error posture.
    fn finish(&mut self, stage: &Stage, prepared: &Prepared, operator: &str, result: Result<ProcessResult>, duration: f64) -> Result<RawCall> {
        let Prepared { artifact, harness_name, payload_path, output_path, temp } = prepared;
        let (artifact, harness_name) = (artifact.as_str(), harness_name.as_str());
        if let Some(path) = temp {
            fs::remove_dir_all(path).with_context(|| format!("stage '{}': remove quarantine {}", stage.id, path.display()))?;
        }
        match result {
            Ok(ProcessResult::Ok(bytes)) => {
                fs::write(&output_path, &bytes).with_context(|| format!("stage '{}': write {}", stage.id, output_path.display()))?;
                let output = match String::from_utf8(bytes) {
                    Ok(output) => output,
                    Err(error) => {
                        self.failed_call(artifact, operator, harness_name, duration, error.as_bytes().len(), "error")?;
                        bail!("harness '{}' stage '{}' emitted non-UTF-8 stdout; artifacts: {}, {}", harness_name, stage.id, payload_path.display(), output_path.display());
                    }
                };
                if output.trim().is_empty() {
                    self.failed_call(artifact, operator, harness_name, duration, 0, "error")?;
                    bail!("harness '{}' stage '{}' emitted empty stdout; artifacts: {}, {}", harness_name, stage.id, payload_path.display(), output_path.display());
                }
                Ok(RawCall { bytes_out: output.len(), output, duration_secs: duration })
            }
            Ok(ProcessResult::Timeout(bytes)) => {
                fs::write(&output_path, &bytes).with_context(|| format!("stage '{}': write {}", stage.id, output_path.display()))?;
                self.failed_call(artifact, operator, harness_name, duration, bytes.len(), "timeout")?;
                bail!("harness '{}' stage '{}' timed out; artifacts: {}, {}", harness_name, stage.id, payload_path.display(), output_path.display());
            }
            Ok(ProcessResult::Exit(code, stdout, stderr)) => {
                fs::write(&output_path, &stdout).with_context(|| format!("stage '{}': write {}", stage.id, output_path.display()))?;
                self.failed_call(artifact, operator, harness_name, duration, stdout.len(), "error")?;
                bail!("harness '{}' stage '{}' exited {}: {}; artifacts: {}, {}", harness_name, stage.id, code, String::from_utf8_lossy(&stderr).trim(), payload_path.display(), output_path.display());
            }
            Err(error) => {
                let _ = fs::write(&output_path, []);
                self.failed_call(artifact, operator, harness_name, duration, 0, "error")?;
                Err(error).with_context(|| format!("harness '{}' stage '{}'; artifacts: {}, {}", harness_name, stage.id, payload_path.display(), output_path.display()))
            }
        }
    }

    fn accept(&mut self, stage: &Stage, harness: &str, raw: &RawCall, outcome: &str) -> Result<()> {
        self.accept_named(&stage.id, &stage.operator, harness, raw, outcome)
    }

    fn accept_named(&mut self, stage: &str, operator: &str, harness: &str, raw: &RawCall, outcome: &str) -> Result<()> {
        self.log_call(stage, operator, harness, raw.duration_secs, raw.bytes_out, outcome)
    }

    fn failed_call(&mut self, stage: &str, operator: &str, harness: &str, duration: f64, bytes: usize, outcome: &str) -> Result<()> {
        self.log_call(stage, operator, harness, duration, bytes, outcome)
    }

    fn log_call(&mut self, stage: &str, operator: &str, harness: &str, duration: f64, bytes: usize, outcome: &str) -> Result<()> {
        append_ledger(&self.ledger_path, &self.run_id, &self.record.composition, stage, operator, harness, duration, bytes, outcome)?;
        self.record.calls.push(CallRecord {
            stage: stage.to_owned(),
            operator: operator.to_owned(),
            harness: harness.to_owned(),
            duration_secs: duration,
            bytes_out: bytes,
            outcome: outcome.to_owned(),
        });
        self.write_record()
    }

    fn snapshot(&self, stage: &Stage, state: &State) -> Result<()> {
        if let Some(plan) = &state.plan {
            let path = self.run_dir.join(format!("{}-plan.md", stage.id));
            fs::write(&path, plan).with_context(|| format!("stage '{}': write {}", stage.id, path.display()))?;
        }
        Ok(())
    }

    fn write_record(&self) -> Result<()> {
        let path = self.run_dir.join("run.json");
        fs::write(&path, run_json(&self.record)).with_context(|| format!("write {}", path.display()))
    }
}

enum ProcessResult {
    Ok(Vec<u8>),
    Timeout(Vec<u8>),
    Exit(i32, Vec<u8>, Vec<u8>),
}

struct Prepared {
    artifact: String,
    harness_name: String,
    payload_path: PathBuf,
    output_path: PathBuf,
    temp: Option<PathBuf>,
}

fn timed_invoke(harness: &Harness, payload: &str, cwd: Option<&Path>) -> (Result<ProcessResult>, f64) {
    let started = Instant::now();
    let result = invoke_process(harness, payload, cwd);
    (result, started.elapsed().as_secs_f64())
}

fn invoke_process(harness: &Harness, payload: &str, cwd: Option<&Path>) -> Result<ProcessResult> {
    let mut command = Command::new(&harness.cmd[0]);
    command.args(&harness.cmd[1..]).envs(&harness.env).stdout(Stdio::piped()).stderr(Stdio::piped());
    match harness.prompt {
        PromptMode::Arg => {
            command.arg(payload).stdin(Stdio::null());
        }
        PromptMode::Stdin => {
            command.stdin(Stdio::piped());
        }
    }
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command.spawn().with_context(|| format!("spawn {}", harness.cmd[0]))?;
    let stdin_writer = if matches!(harness.prompt, PromptMode::Stdin) {
        let mut stdin = child.stdin.take().expect("configured piped stdin");
        let prompt = payload.as_bytes().to_vec();
        Some(thread::spawn(move || stdin.write_all(&prompt)))
    } else {
        None
    };
    let mut stdout = child.stdout.take().expect("configured piped stdout");
    let mut stderr = child.stderr.take().expect("configured piped stderr");
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(harness.timeout_secs);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().context("wait for harness")? {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            child.kill().context("kill timed-out harness")?;
            break child.wait().context("reap timed-out harness")?;
        }
        thread::sleep(Duration::from_millis(20));
    };
    if let Some(writer) = stdin_writer {
        let write_result = writer.join().map_err(|_| anyhow!("stdin writer panicked"))?;
        if !timed_out {
            write_result.context("write harness stdin")?;
        }
    }
    let stdout = stdout_reader.join().map_err(|_| anyhow!("stdout reader panicked"))??;
    let stderr = stderr_reader.join().map_err(|_| anyhow!("stderr reader panicked"))??;
    if timed_out {
        Ok(ProcessResult::Timeout(stdout))
    } else if status.success() {
        Ok(ProcessResult::Ok(stdout))
    } else {
        Ok(ProcessResult::Exit(status.code().unwrap_or(-1), stdout, stderr))
    }
}

fn unique_run_dir(root: &Path) -> Result<(String, PathBuf)> {
    loop {
        let id = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let path = root.join(&id);
        match fs::create_dir(&path) {
            Ok(()) => return Ok((id, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => thread::sleep(Duration::from_millis(100)),
            Err(error) => return Err(error).with_context(|| format!("create {}", path.display())),
        }
    }
}

fn append_ledger(path: &Path, run_id: &str, composition: &str, stage: &str, operator: &str, harness: &str, duration: f64, bytes: usize, outcome: &str) -> Result<()> {
    let new = !path.exists();
    let mut file = OpenOptions::new().create(true).append(true).open(path).with_context(|| format!("open {}", path.display()))?;
    if new {
        file.write_all(LEDGER_HEADER.as_bytes()).with_context(|| format!("write {} header", path.display()))?;
    }
    writeln!(file, "{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{}\t{}", Utc::now().to_rfc3339(), run_id, composition, stage, operator, harness, duration, bytes, outcome)
        .with_context(|| format!("append {}", path.display()))
}

fn append_text(target: &mut String, text: &str) {
    if !target.is_empty() {
        target.push_str("\n\n");
    }
    target.push_str(text.trim_end());
    target.push('\n');
}

fn strip_sentinel_line(output: &str, sentinel: &str) -> String {
    let mut result = output.lines().filter(|line| line.trim() != sentinel).collect::<Vec<_>>().join("\n");
    if output.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}

fn parse_winner(output: &str) -> Result<char> {
    let matches: Vec<_> = output
        .lines()
        .filter_map(|line| line.strip_prefix("WINNER: "))
        .filter_map(|value| {
            let mut chars = value.trim().chars();
            let letter = chars.next()?;
            (letter.is_ascii_uppercase() && chars.next().is_none()).then_some(letter)
        })
        .collect();
    match matches.as_slice() {
        [winner] => Ok(*winner),
        [] => bail!("winner line absent"),
        _ => bail!("multiple winner lines"),
    }
}

fn label_candidates(candidates: &[Candidate]) -> (String, String) {
    let mut blocks = Vec::new();
    let mut entries = Vec::new();
    for (label_index, candidate) in candidates.iter().enumerate() {
        let label = (b'A' + label_index as u8) as char;
        blocks.push(format!("Candidate {label}:\n{}", candidate.text));
        entries.push(format!(
            "    \"{}\": {{\"stage\": \"{}\", \"index\": {}, \"harness\": \"{}\"}}",
            label,
            json_escape(&candidate.stage),
            candidate.index,
            json_escape(&candidate.harness)
        ));
    }
    (blocks.join("\n\n"), format!("{{\n{}\n}}\n", entries.join(",\n")))
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c < ' ' => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}

fn run_json(record: &RunRecord) -> String {
    let stages = record.stage_order.iter().map(|s| format!("\"{}\"", json_escape(s))).collect::<Vec<_>>().join(", ");
    let calls = record.calls.iter().map(|call| format!(
        "    {{\"stage\": \"{}\", \"operator\": \"{}\", \"harness\": \"{}\", \"duration_secs\": {:.3}, \"bytes_out\": {}, \"outcome\": \"{}\"}}",
        json_escape(&call.stage), json_escape(&call.operator), json_escape(&call.harness), call.duration_secs, call.bytes_out, json_escape(&call.outcome)
    )).collect::<Vec<_>>().join(",\n");
    let stops = record.loop_stops.iter().map(|stop| format!(
        "    {{\"stage\": \"{}\", \"round\": {}, \"reason\": \"{}\"}}",
        json_escape(&stop.stage), stop.round, json_escape(&stop.reason)
    )).collect::<Vec<_>>().join(",\n");
    let grades = record.grades.iter().map(|grade| format!(
        "    {{\"stage\": \"{}\", \"output\": \"{}\"}}",
        json_escape(&grade.stage), json_escape(&grade.output)
    )).collect::<Vec<_>>().join(",\n");
    format!(
        "{{\n  \"run_id\": \"{}\",\n  \"composition\": \"{}\",\n  \"ask\": \"{}\",\n  \"stage_order\": [{}],\n  \"calls\": [\n{}\n  ],\n  \"loop_stops\": [\n{}\n  ],\n  \"grades\": [\n{}\n  ],\n  \"outcome\": \"{}\"\n}}\n",
        json_escape(&record.run_id), json_escape(&record.composition), json_escape(&record.ask), stages, calls, stops, grades, json_escape(&record.outcome)
    )
}

pub fn run_composition(loaded: &Loaded, ask: Option<String>, plan: Option<String>, n: Option<usize>, out: &Path) -> Result<PathBuf> {
    let ask_text = ask.clone().unwrap_or_default();
    let state = State { ask, plan, ..State::default() };
    let runner = Runner::new(loaded, &loaded.composition.name, &ask_text)?;
    let (final_plan, _) = runner.run(state, n)?;
    fs::write(out, final_plan).with_context(|| format!("write final plan {}", out.display()))?;
    Ok(out.to_path_buf())
}

pub fn standalone_judge(loaded: &Loaded, files: &[PathBuf], ask: Option<String>) -> Result<()> {
    if files.len() < 2 || files.len() > 26 {
        bail!("judge needs 2..=26 candidate files, got {}", files.len());
    }
    let objective = ask.unwrap_or_default();
    let mut candidates = files
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let text = fs::read_to_string(path).with_context(|| format!("read candidate {}", path.display()))?;
            Ok(Candidate { text, stage: "standalone".to_owned(), index, harness: path.display().to_string() })
        })
        .collect::<Result<Vec<_>>>()?;
    candidates.shuffle(&mut rand::thread_rng());
    let mut runner = Runner::new(loaded, "judge", &objective)?;
    runner.record.stage_order.push("judge".to_owned());
    let (blocks, mapping) = label_candidates(&candidates);
    fs::write(runner.run_dir.join("mapping.json"), &mapping).with_context(|| format!("write {}/mapping.json", runner.run_dir.display()))?;
    let state = State { ask: Some(objective.clone()), objective: Some(objective), ..State::default() };
    let extras = HashMap::from([("n", candidates.len().to_string()), ("candidates", blocks)]);
    let payload = render(&loaded.operators["blind-judge"], &state, &extras, true)?;
    let stage = Stage { id: "judge".to_owned(), kind: "judge".to_owned(), operator: "blind-judge".to_owned(), n: None, quarantine: false, inputs: Vec::new(), max_rounds: None, stop_sentinel: None };
    let raw = runner.execute(&stage, "judge", "blind-judge", &loaded.harnesses.default, &payload, false)?;
    runner.accept(&stage, &loaded.harnesses.default, &raw, "ok")?;
    if let Err(error) = parse_winner(&raw.output).context("standalone judge output must contain one line 'WINNER: <letter>'") {
        runner.record.outcome = "error".to_owned();
        runner.write_record()?;
        return Err(error);
    }
    runner.record.outcome = "ok".to_owned();
    runner.write_record()?;
    print!("{}", raw.output);
    println!("mapping: {}", runner.run_dir.join("mapping.json").display());
    Ok(())
}

pub fn bench(loaded: &Loaded, operator_id: &str, file: &Path) -> Result<()> {
    let operator = loaded.operators.get(operator_id).ok_or_else(|| anyhow!("unknown operator '{}'", operator_id))?;
    let defend_operator = loaded.operators.get("defend").ok_or_else(|| anyhow!("arsenal is missing required operator 'defend'"))?;
    let original = fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
    let mut runner = Runner::new(loaded, "bench", "")?;
    runner.record.stage_order = vec!["bench".to_owned(), "defend".to_owned()];
    let mut state = State { plan: Some(original), ..State::default() };
    let stage = Stage { id: "bench".to_owned(), kind: "single".to_owned(), operator: operator_id.to_owned(), n: None, quarantine: false, inputs: Vec::new(), max_rounds: None, stop_sentinel: None };
    let payload = render(operator, &state, &HashMap::new(), true)?;
    let first = runner.execute(&stage, "bench", operator_id, &loaded.harnesses.default, &payload, false)?;
    runner.accept(&stage, &loaded.harnesses.default, &first, "ok")?;
    state.plan = Some(first.output);
    let defend_payload = render(defend_operator, &state, &HashMap::new(), true)?;
    let defend = runner.execute(&stage, "defend", "defend", &loaded.harnesses.default, &defend_payload, false)?;
    runner.accept_named("defend", "defend", &loaded.harnesses.default, &defend, "ok")?;
    runner.record.outcome = "ok".to_owned();
    runner.write_record()?;
    println!("operator output: {}", runner.run_dir.join("bench-output.txt").display());
    println!("defense output: {}", runner.run_dir.join("defend-output.txt").display());
    println!("grading is manual in v0");
    Ok(())
}

pub fn print_ledger() -> Result<()> {
    let cwd = env::current_dir().context("resolve current directory")?;
    println!("your runs");
    print_tsv_if_present(&cwd.join(".dakka/ledger.tsv"))?;
    println!("\narsenal evidence");
    print_tsv_if_present(&asset_root()?.join("arsenal/yields.tsv"))
}

fn print_tsv_if_present(path: &Path) -> Result<()> {
    if !path.exists() {
        println!("(none)");
        return Ok(());
    }
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let rows: Vec<Vec<&str>> = text.lines().map(|line| line.split('\t').collect()).collect();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| rows.iter().filter_map(|row| row.get(column)).map(|cell| cell.chars().count()).max().unwrap_or(0))
        .collect();
    for row in rows {
        for (column, cell) in row.iter().enumerate() {
            if column + 1 == row.len() {
                print!("{cell}");
            } else {
                print!("{cell:width$}  ", width = widths[column]);
            }
        }
        println!();
    }
    Ok(())
}
