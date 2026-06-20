//! Partial / mmap-friendly graph access for read-only activate paths.

use std::path::Path;

use crate::graph::BrainGraph;
use crate::hippocampus::Hippocampus;
use crate::semantic::SemanticField;

/// Lightweight read view — graph + hippocampus + semantic without full brain mutation state.
#[derive(Debug, Clone)]
pub struct ActivateView {
    pub graph: BrainGraph,
    pub hippocampus: Hippocampus,
    pub semantic: SemanticField,
}

impl ActivateView {
    pub fn from_brain(brain: &crate::brain::FluctlightBrain) -> Self {
        Self {
            graph: brain.graph.clone(),
            hippocampus: brain.hippocampus.clone(),
            semantic: brain.semantic.clone(),
        }
    }

    /// Future: mmap graph segment from v4 storage dir without full RAM load.
    pub fn open_graph_segment(_brain_dir: &Path) -> Option<BrainGraph> {
        None
    }
}
