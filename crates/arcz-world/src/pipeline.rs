//! Resumable world-generation job graph.
//! A world build is partitioned by cell/layer so failed or upgraded generators can be replayed
//! without rebuilding unrelated canonical authoring data.

use crate::WorldCellId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum WorldBuildStage {
    ResolveSources,
    Terrain,
    Transport,
    Buildings,
    Vegetation,
    Hydrology,
    UrbanFurniture,
    MaterialEnrichment,
    SemanticValidation,
    PublishRuntime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldBuildJob {
    pub id: String,
    pub cell: WorldCellId,
    pub stage: WorldBuildStage,
    pub source_revision: u64,
    pub generator_id: String,
    pub generator_version: String,
    pub seed: u64,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorldBuildPlan {
    pub project_id: String,
    pub jobs: Vec<WorldBuildJob>,
}

impl WorldBuildPlan {
    pub fn validate(&self) -> Result<(), WorldBuildPlanError> {
        if self.project_id.trim().is_empty() {
            return Err(WorldBuildPlanError::MissingProjectId);
        }

        let mut jobs = BTreeMap::new();
        for job in &self.jobs {
            if job.id.trim().is_empty()
                || job.generator_id.trim().is_empty()
                || job.generator_version.trim().is_empty()
            {
                return Err(WorldBuildPlanError::MissingJobIdentity(job.id.clone()));
            }
            if jobs.insert(job.id.clone(), job).is_some() {
                return Err(WorldBuildPlanError::DuplicateJob(job.id.clone()));
            }
        }

        for job in &self.jobs {
            for dependency in &job.depends_on {
                if dependency == &job.id || !jobs.contains_key(dependency) {
                    return Err(WorldBuildPlanError::InvalidDependency {
                        job: job.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }

        // Detect cycles without assuming declaration order.
        fn visit<'a>(
            id: &str,
            jobs: &BTreeMap<String, &'a WorldBuildJob>,
            temporary: &mut BTreeSet<String>,
            permanent: &mut BTreeSet<String>,
        ) -> Result<(), WorldBuildPlanError> {
            if permanent.contains(id) {
                return Ok(());
            }
            if !temporary.insert(id.to_owned()) {
                return Err(WorldBuildPlanError::DependencyCycle(id.to_owned()));
            }
            let job = jobs.get(id).expect("validated job id");
            for dependency in &job.depends_on {
                visit(dependency, jobs, temporary, permanent)?;
            }
            temporary.remove(id);
            permanent.insert(id.to_owned());
            Ok(())
        }

        let mut temporary = BTreeSet::new();
        let mut permanent = BTreeSet::new();
        for id in jobs.keys() {
            visit(id, &jobs, &mut temporary, &mut permanent)?;
        }

        Ok(())
    }

    pub fn ready_jobs<'a>(&'a self, completed: &BTreeSet<String>) -> Vec<&'a WorldBuildJob> {
        self.jobs
            .iter()
            .filter(|job| {
                !completed.contains(&job.id)
                    && job.depends_on.iter().all(|dependency| completed.contains(dependency))
            })
            .collect()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorldBuildPlanError {
    #[error("project id is required")]
    MissingProjectId,
    #[error("world build job identity is incomplete: {0}")]
    MissingJobIdentity(String),
    #[error("duplicate world build job: {0}")]
    DuplicateJob(String),
    #[error("invalid dependency {dependency} for job {job}")]
    InvalidDependency { job: String, dependency: String },
    #[error("world build dependency cycle includes job {0}")]
    DependencyCycle(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, stage: WorldBuildStage, depends_on: &[&str]) -> WorldBuildJob {
        WorldBuildJob {
            id: id.into(),
            cell: WorldCellId { level: 10, x: 1, y: 2 },
            stage,
            source_revision: 7,
            generator_id: format!("generator.{id}"),
            generator_version: "0.1.0".into(),
            seed: 42,
            depends_on: depends_on.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    #[test]
    fn validates_and_returns_resumable_ready_jobs() {
        let plan = WorldBuildPlan {
            project_id: "city".into(),
            jobs: vec![
                job("sources", WorldBuildStage::ResolveSources, &[]),
                job("terrain", WorldBuildStage::Terrain, &["sources"]),
                job("roads", WorldBuildStage::Transport, &["terrain"]),
            ],
        };
        plan.validate().unwrap();
        let mut completed = BTreeSet::new();
        assert_eq!(plan.ready_jobs(&completed)[0].id, "sources");
        completed.insert("sources".into());
        assert_eq!(plan.ready_jobs(&completed)[0].id, "terrain");
    }

    #[test]
    fn rejects_cycles() {
        let plan = WorldBuildPlan {
            project_id: "city".into(),
            jobs: vec![
                job("a", WorldBuildStage::Terrain, &["b"]),
                job("b", WorldBuildStage::Transport, &["a"]),
            ],
        };
        assert!(matches!(plan.validate(), Err(WorldBuildPlanError::DependencyCycle(_))));
    }
}
