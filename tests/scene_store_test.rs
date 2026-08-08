use arcz_jobs::{JobQueue, JobStatus, JobTask};
use arcz_provenance::{ExternalSourceRecord, LicenseType};
use arcz_scene::{Command, CommandBus, NodeType, SceneNode};

#[test]
fn test_authoritative_scene_graph_mutations() {
    let mut bus = CommandBus::new();
    assert_eq!(bus.revision, 0);

    let building = SceneNode::new(101, "Edifício Zênite", NodeType::Building);
    bus.apply(Command::AddNode(building));

    assert_eq!(bus.revision, 1);
    assert_eq!(bus.nodes.len(), 1);

    bus.apply(Command::RenameNode {
        id: 101,
        name: "Edifício Zênite - Fase 1".to_string(),
    });

    assert_eq!(bus.revision, 2);
    assert_eq!(bus.nodes.get(&101).unwrap().name, "Edifício Zênite - Fase 1");

    bus.undo();
    assert_eq!(bus.nodes.get(&101).unwrap().name, "Edifício Zênite");
}

#[test]
fn test_persistent_job_queue_crash_recovery() {
    let mut queue = JobQueue::new();
    let mut task = JobTask::new("job-100", "Renderização Offscreen PBR", "render");
    task.status = JobStatus::Running;
    queue.enqueue(task);

    // Simula recuperação após encerramento inesperado
    let recovered = queue.recover_orphans();
    assert_eq!(recovered, 1);
    assert_eq!(queue.jobs.get("job-100").unwrap().status, JobStatus::Pending);
}

#[test]
fn test_external_provenance_validation() {
    let source = ExternalSourceRecord::new(
        "osm-sc-floripa",
        "OpenStreetMap Contributors",
        LicenseType::ODbL,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );

    assert!(source.is_commercially_safe());
    assert!(source.cache_allowed);
}
