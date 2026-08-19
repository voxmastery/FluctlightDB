//! Probe which segment of a v4 generation fails to deserialize (format triage).
use std::path::PathBuf;

fn probe<T: serde::de::DeserializeOwned>(dir: &PathBuf, name: &str) {
    let t0 = std::time::Instant::now();
    match fluctlightdb::segment::read_segment::<T>(dir, name) {
        Ok(_) => println!("  {name:<22} OK   ({:?})", t0.elapsed()),
        Err(e) => println!("  {name:<22} FAIL {e} ({:?})", t0.elapsed()),
    }
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("usage: segprobe <gen-dir>"));
    use fluctlightdb::*;
    probe::<life::LifeState>(&dir, "life");
    probe::<development::DevelopmentState>(&dir, "development");
    probe::<graph::BrainGraph>(&dir, "graph");
    probe::<hippocampus::Hippocampus>(&dir, "hippocampus");
    probe::<cortex::Cortex>(&dir, "cortex");
    probe::<amygdala::Amygdala>(&dir, "amygdala");
    probe::<prefrontal::Prefrontal>(&dir, "prefrontal");
    probe::<life::CoreMemoryStore>(&dir, "core_memories");
    probe::<autonomic::AutonomicState>(&dir, "autonomic");
    probe::<semantic::SemanticField>(&dir, "semantic");
    probe::<muon::MuonLane>(&dir, "muon");
    probe::<tau::TauLane>(&dir, "tau");
}
