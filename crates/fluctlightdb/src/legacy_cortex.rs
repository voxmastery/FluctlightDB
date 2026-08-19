//! Backward-compatible cortex segment load.
//!
//! Production brains checkpointed by fluctlight-0.5.18 (Jul 2026) wrote `Cortex` before the
//! `schemas` (SchemaStore) and `eligibility` (EligibilityStore) fields existed. bincode is not
//! self-describing, so `#[serde(default)]` cannot rescue a missing trailing field — reading an
//! old segment with the current struct fails with "unexpected end of file" and the whole brain
//! refuses to open. That EOF is exactly what blocked upgrading production off 0.5.18.
//!
//! Mirrors `legacy_hippocampus`: try the current layout first, then progressively older ones,
//! defaulting the fields that did not exist yet.
use std::collections::HashMap;
use std::path::Path;

use crate::cortex::Cortex;
use crate::error::Result;
use crate::id::NeuronId;
use crate::segment;

/// 0.5.18-era layout: no schemas, no eligibility.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CortexPreSchemas {
    pub facts: HashMap<String, f32>,
    pub token_strength: HashMap<NeuronId, f32>,
    #[serde(default)]
    pub semantic_centroid: Vec<f32>,
    #[serde(default)]
    pub semantic_strength: f32,
}

/// Earliest layout: facts + token strengths only.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CortexFactsOnly {
    pub facts: HashMap<String, f32>,
    pub token_strength: HashMap<NeuronId, f32>,
}

pub(crate) fn read_cortex_segment(dir: &Path) -> Result<Cortex> {
    match segment::read_segment::<Cortex>(dir, "cortex") {
        Ok(cortex) => Ok(cortex),
        Err(current_err) => {
            if let Ok(old) = segment::read_segment::<CortexPreSchemas>(dir, "cortex") {
                return Ok(Cortex {
                    facts: old.facts,
                    token_strength: old.token_strength,
                    semantic_centroid: old.semantic_centroid,
                    semantic_strength: old.semantic_strength,
                    ..Cortex::default()
                });
            }
            if let Ok(old) = segment::read_segment::<CortexFactsOnly>(dir, "cortex") {
                return Ok(Cortex {
                    facts: old.facts,
                    token_strength: old.token_strength,
                    ..Cortex::default()
                });
            }
            Err(current_err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn legacy_pre_schemas_cortex_loads_with_defaults() {
        let dir = tempdir().unwrap();
        let old = CortexPreSchemas {
            facts: HashMap::from([("ambulance".into(), 2.5)]),
            token_strength: HashMap::from([(NeuronId(7), 0.9)]),
            semantic_centroid: vec![0.1, 0.2],
            semantic_strength: 0.4,
        };
        segment::write_segment(dir.path(), "cortex", &old).unwrap();

        let cortex = read_cortex_segment(dir.path()).expect("legacy cortex must load");
        assert_eq!(cortex.facts.get("ambulance"), Some(&2.5));
        assert_eq!(cortex.token_strength.get(&NeuronId(7)), Some(&0.9));
    }

    #[test]
    fn current_cortex_still_roundtrips() {
        let dir = tempdir().unwrap();
        let mut cur = Cortex::default();
        cur.facts.insert("fare".into(), 1.0);
        segment::write_segment(dir.path(), "cortex", &cur).unwrap();
        let back = read_cortex_segment(dir.path()).unwrap();
        assert_eq!(back.facts.get("fare"), Some(&1.0));
    }
}
