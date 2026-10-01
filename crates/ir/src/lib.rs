//! Typed intermediate representation of a whole workspace of `.spec` files.
//!
//! A workspace is the merge of every `specs/**/*.spec` file in a project.
//! IDs are globally unique across the workspace (enforced by the parser's
//! validation pass, not by this crate).

use serde::{Deserialize, Serialize};

/// Deterministic names/paths shared by codegen and the gate so that a
/// generated test can always be traced back to its spec item.
pub mod names {
    /// Rust and Python identifiers cannot contain `-`; normalize IDs.
    pub fn sanitize(id: &str) -> String {
        id.chars().map(|c| if c == '-' { '_' } else { c }).collect()
    }

    pub fn pytest_scenario_test_name(spec_id: &str, scenario_id: &str) -> String {
        format!(
            "test_scenario_{}_{}_unit",
            sanitize(spec_id),
            sanitize(scenario_id)
        )
    }

    pub fn pytest_invariant_test_name(spec_id: &str, invariant_id: &str) -> String {
        format!(
            "test_invariant_{}_{}_unit",
            sanitize(spec_id),
            sanitize(invariant_id)
        )
    }

    pub fn unit_test_file(spec_id: &str, scenario_id: &str) -> String {
        format!("test_{}_{}.py", sanitize(spec_id), sanitize(scenario_id))
    }

    pub fn invariant_test_file(spec_id: &str, invariant_id: &str) -> String {
        format!(
            "test_invariant_{}_{}.py",
            sanitize(spec_id),
            sanitize(invariant_id)
        )
    }

    /// Canonical expected JUnit id for a contract-layer scenario row. The id is
    /// embedded in the Gherkin `Scenario:` name by codegen, and the gate matches
    /// it against the testcase name, so the two agree by construction.
    ///
    /// Format assumption (unverified here: cucumber and karate are not
    /// installed): writers differ on how a Scenario title is split between
    /// `classname` and `name`, so the id is joined with `_` — a separator none
    /// of them insert (a space could become a word break in a classname). The
    /// result contains no `-`, so it survives the gate's `norm()` unchanged.
    /// If a reporter disagrees, the gate falls back to suite-level matching.
    pub fn contract_scenario_test_name(spec_id: &str, scenario_id: &str) -> String {
        format!("{}_{}", sanitize(spec_id), sanitize(scenario_id))
    }

    pub fn contract_feature_file(spec_id: &str) -> String {
        format!("{}.feature", spec_id)
    }

    /// Canonical expected JUnit id for an e2e-layer scenario row. Playwright
    /// titles the test `"{spec_id} {scenario_id}"`, so the space-separated form
    /// is what the reporter must be matched against; codegen already emitted
    /// this shape, which is why F6 changed only the gate side for e2e.
    ///
    /// Format assumption (unverified here: playwright is not installed): the
    /// reporter does not rewrite the title. `norm()` only rewrites `-` to `_`,
    /// so the space must be kept as-is rather than normalized away. If a
    /// reporter disagrees, the gate falls back to suite-level matching.
    pub fn e2e_test_name(spec_id: &str, scenario_id: &str) -> String {
        format!("{} {}", sanitize(spec_id), sanitize(scenario_id))
    }

    pub fn e2e_spec_file(scenario_id: &str) -> String {
        format!("{}.spec.ts", scenario_id)
    }

    pub fn infra_policy_file(policy_id: &str) -> String {
        format!("{}.rego", policy_id)
    }

    pub fn rust_unit_file(spec_id: &str) -> String {
        format!("{}.rs", spec_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Unit,
    Contract,
    E2e,
    Infra,
    Fitness,
}

impl Layer {
    pub const ALL: [Layer; 5] = [
        Layer::Unit,
        Layer::Contract,
        Layer::E2e,
        Layer::Infra,
        Layer::Fitness,
    ];

    pub fn parse(s: &str) -> Option<Layer> {
        match s {
            "unit" => Some(Layer::Unit),
            "contract" => Some(Layer::Contract),
            "e2e" => Some(Layer::E2e),
            "infra" => Some(Layer::Infra),
            "fitness" => Some(Layer::Fitness),
            _ => None,
        }
    }

    pub fn dir(&self) -> &'static str {
        match self {
            Layer::Unit => "unit",
            Layer::Contract => "contract",
            Layer::E2e => "e2e",
            Layer::Infra => "infra",
            Layer::Fitness => "fitness",
        }
    }

    pub fn as_str(&self) -> &'static str {
        self.dir()
    }
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Accepted,
    Proposed,
    Rejected,
    Superseded,
}

impl Status {
    pub fn parse(s: &str) -> Option<Status> {
        match s {
            "accepted" => Some(Status::Accepted),
            "proposed" => Some(Status::Proposed),
            "rejected" => Some(Status::Rejected),
            "superseded" => Some(Status::Superseded),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Accepted => "accepted",
            Status::Proposed => "proposed",
            Status::Rejected => "rejected",
            Status::Superseded => "superseded",
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a definition came from, for file:line diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Src {
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Decision {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub context: Option<String>,
    pub consequences: Option<String>,
    pub superseded_by: Option<String>,
    pub src: Src,
}

#[derive(Debug, Clone)]
pub struct Container {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct Rel {
    pub from: String,
    pub to: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub enum FlowStep {
    Message {
        from: String,
        to: String,
        label: String,
        dashed: bool,
    },
    Alt {
        name: String,
        then_branch: Vec<FlowStep>,
        else_branch: Vec<FlowStep>,
    },
}

#[derive(Debug, Clone)]
pub struct Flow {
    pub id: String,
    pub steps: Vec<FlowStep>,
    pub src: Src,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub containers: Vec<Container>,
    pub rels: Vec<Rel>,
    pub flows: Vec<Flow>,
    pub file: String,
}

#[derive(Debug, Clone)]
pub struct Requirement {
    pub id: String,
    pub style: Option<String>,
    pub text: String,
    pub layers: Vec<Layer>,
    pub scenarios: Vec<String>,
    pub src: Src,
}

#[derive(Debug, Clone)]
pub struct Scenario {
    pub id: String,
    pub given: String,
    pub when: String,
    pub then: String,
    pub src: Src,
}

#[derive(Debug, Clone)]
pub struct Invariant {
    pub id: String,
    pub expr: String,
    pub layers: Vec<Layer>,
    pub src: Src,
}

#[derive(Debug, Clone)]
pub struct InfraPolicy {
    pub id: String,
    pub description: String,
    pub layers: Vec<Layer>,
    pub src: Src,
}

/// A `spec <ID>` block. `<ID>` must equal an existing `decision` ID, so the
/// spec id *is* the decision id (1:1, enforced by global ID uniqueness).
#[derive(Debug, Clone)]
pub struct Spec {
    pub id: String,
    pub requirements: Vec<Requirement>,
    pub scenarios: Vec<Scenario>,
    pub invariants: Vec<Invariant>,
    pub infra_policies: Vec<InfraPolicy>,
    pub src: Src,
}

impl Spec {
    pub fn scenario(&self, id: &str) -> Option<&Scenario> {
        self.scenarios.iter().find(|s| s.id == id)
    }

    /// Layers a scenario is exercised on: the union of the layers of every
    /// requirement that references it (stable order, deduped).
    pub fn layers_for_scenario(&self, scenario_id: &str) -> Vec<Layer> {
        let mut out = Vec::new();
        for req in &self.requirements {
            if req.scenarios.iter().any(|s| s == scenario_id) {
                for layer in &req.layers {
                    if !out.contains(layer) {
                        out.push(*layer);
                    }
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default)]
pub struct Workspace {
    pub decisions: Vec<Decision>,
    pub models: Vec<Model>,
    pub specs: Vec<Spec>,
}

impl Workspace {
    pub fn decision(&self, id: &str) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.id == id)
    }

    pub fn spec(&self, id: &str) -> Option<&Spec> {
        self.specs.iter().find(|s| s.id == id)
    }

    pub fn spec_for_decision(&self, decision_id: &str) -> Option<&Spec> {
        self.spec(decision_id)
    }

    pub fn all_flows(&self) -> impl Iterator<Item = &Flow> {
        self.models.iter().flat_map(|m| m.flows.iter())
    }

    pub fn all_rels(&self) -> impl Iterator<Item = &Rel> {
        self.models.iter().flat_map(|m| m.rels.iter())
    }

    /// Flows defined in the same file as `file`; used by `decispec query` to
    /// show the model fragments closest to a spec.
    pub fn flows_in_file(&self, file: &str) -> Vec<&Flow> {
        self.models
            .iter()
            .filter(|m| m.file == file)
            .flat_map(|m| m.flows.iter())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::names;

    #[test]
    fn contract_scenario_test_name_joins_and_sanitizes() {
        assert_eq!(
            names::contract_scenario_test_name("AUTH-001", "valid_token"),
            "AUTH_001_valid_token"
        );
        assert_eq!(
            names::contract_scenario_test_name("AUTH-001", "invalid-token"),
            "AUTH_001_invalid_token"
        );
    }

    #[test]
    fn e2e_test_name_matches_playwright_title() {
        assert_eq!(
            names::e2e_test_name("AUTH-001", "valid_token"),
            "AUTH_001 valid_token"
        );
    }

    #[test]
    fn contract_and_e2e_names_survive_norm() {
        for name in [
            names::contract_scenario_test_name("AUTH-001", "valid_token"),
            names::e2e_test_name("AUTH-001", "valid_token"),
        ] {
            assert!(!name.contains('-'), "{name} contains a dash");
        }
    }
}
