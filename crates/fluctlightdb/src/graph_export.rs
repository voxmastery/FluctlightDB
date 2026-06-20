use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::engram::Engram;
use crate::graph::BrainGraph;
use crate::hippocampus::Hippocampus;
use crate::id::NeuronId;
use crate::life::CoreMemoryStore;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphExport {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
    pub stats: GraphStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphStats {
    pub engrams: usize,
    pub neurons: usize,
    pub synapses: usize,
    pub core_memories: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub region: String,
    pub domain: String,
    pub size: f32,
    pub salience: f32,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphLink {
    pub source: String,
    pub target: String,
    pub weight: f32,
    pub kind: String,
}

/// Anatomical region keys — matched to 3D brain hull seeds in the live viewer.
fn engram_region(engram: &Engram) -> &'static str {
    if engram.is_core {
        return "corpus-callosum";
    }
    let ctx = engram.episode.context.to_lowercase();
    let content = engram.episode.content.to_lowercase();
    if engram.salience >= 0.85 || content.contains("error") || content.contains("death") {
        return "temporal-l";
    }
    if ctx.contains("command") || ctx.contains("prefrontal") {
        return "prefrontal-l";
    }
    if ctx.contains("model") || content.contains("conversation") {
        return "frontal-r";
    }
    if ctx.contains("wallet") || content.contains("reward") || content.contains("fine") {
        return "parietal-r";
    }
    if content.contains("sleep") || content.contains("consolid") {
        return "occipital-l";
    }
    if engram.separation_index > 0.7 {
        return "parietal-l";
    }
    "temporal-r"
}

fn domain_for(kind: &str, engram: Option<&Engram>) -> &'static str {
    match kind {
        "region" => "server",
        "core" => "core",
        "neuron" => "active",
        _ => {
            if let Some(e) = engram {
                if e.is_core {
                    return "core";
                }
                if e.salience >= 0.75 {
                    return "active";
                }
                if e.episode.content.to_lowercase().contains("error") {
                    return "pending";
                }
            }
            "client"
        }
    }
}

fn color_for(domain: &str) -> &'static str {
    match domain {
        "core" => "#c8a0ff",
        "active" => "#70c1b3",
        "client" => "#5b9bd5",
        "pending" => "#ffd166",
        "server" => "#9b5de5",
        "partner" => "#f4a261",
        _ => "#8048ff",
    }
}

fn truncate(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n {
        t.to_string()
    } else {
        format!("{}…", t.chars().take(n).collect::<String>())
    }
}

pub fn export_graph(
    hippocampus: &Hippocampus,
    graph: &BrainGraph,
    core: &CoreMemoryStore,
) -> GraphExport {
    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut neuron_to_engram: HashMap<NeuronId, String> = HashMap::new();

    let region_hubs = [
        ("hub-ec", "Entorhinal (EC)", "frontal-r"),
        ("hub-dg", "Dentate (DG)", "parietal-l"),
        ("hub-ca3", "CA3 completion", "temporal-r"),
        ("hub-amy", "Amygdala", "temporal-l"),
        ("hub-pfc", "Prefrontal", "prefrontal-l"),
        ("hub-cortex", "Cortex", "parietal-r"),
        ("hub-stem", "Brainstem", "brainstem"),
    ];
    for (id, label, region) in region_hubs {
        nodes.push(GraphNode {
            id: id.into(),
            label: label.into(),
            kind: "region".into(),
            region: region.into(),
            domain: domain_for("region", None).into(),
            size: 7.0,
            salience: 1.0,
            color: color_for("server").into(),
            desc: Some(format!("{label} — cognitive region hub")),
        });
    }

    for engram in &hippocampus.engrams {
        let eid = format!("e:{}", engram.id);
        let region = engram_region(engram);
        let domain = domain_for("engram", Some(engram));
        let label = truncate(&engram.episode.content, 42);
        nodes.push(GraphNode {
            id: eid.clone(),
            label: label.clone(),
            kind: "engram".into(),
            region: region.into(),
            domain: domain.into(),
            size: 4.0 + engram.salience * 5.0,
            salience: engram.salience,
            color: color_for(domain).into(),
            desc: Some(format!(
                "{}\ncontext: {}\nseparation: {:.0}%",
                engram.episode.content,
                engram.episode.context,
                engram.separation_index * 100.0
            )),
        });

        let hub = match region {
            "prefrontal-l" | "frontal-r" => "hub-pfc",
            "temporal-l" => "hub-amy",
            "parietal-l" | "parietal-r" => "hub-dg",
            "occipital-l" | "occipital-r" => "hub-cortex",
            "corpus-callosum" => "hub-ca3",
            _ => "hub-ca3",
        };
        links.push(GraphLink {
            source: eid.clone(),
            target: hub.into(),
            weight: 0.35 + engram.salience * 0.4,
            kind: "region".into(),
        });

        for (i, neuron) in engram.dg_neurons.iter().enumerate() {
            let nid = format!("n:{}:{}", engram.id, i);
            neuron_to_engram.insert(*neuron, eid.clone());
            nodes.push(GraphNode {
                id: nid.clone(),
                label: format!("dg-{i}"),
                kind: "neuron".into(),
                region: region.into(),
                domain: "active".into(),
                size: 2.0 + engram.salience,
                salience: engram.salience * 0.6,
                color: "#06d6a0".into(),
                desc: None,
            });
            links.push(GraphLink {
                source: eid.clone(),
                target: nid,
                weight: 0.5,
                kind: "ensemble".into(),
            });
        }
    }

    // Engram–engram associative links via shared DG neurons (Obsidian-style note links).
    let mut engram_pairs: HashSet<(String, String)> = HashSet::new();
    for engram in &hippocampus.engrams {
        let eid = format!("e:{}", engram.id);
        let mut shared: HashMap<String, u32> = HashMap::new();
        for n in &engram.dg_neurons {
            if let Some(other) = neuron_to_engram.get(n) {
                if other != &eid {
                    *shared.entry(other.clone()).or_default() += 1;
                }
            }
        }
        for (other, count) in shared {
            let (a, b) = if eid < other {
                (eid.clone(), other)
            } else {
                (other, eid.clone())
            };
            if count >= 1 && engram_pairs.insert((a.clone(), b.clone())) {
                links.push(GraphLink {
                    source: a,
                    target: b,
                    weight: (count as f32 * 0.15).min(0.9),
                    kind: "associate".into(),
                });
            }
        }
    }

    for mem in &core.memories {
        if hippocampus
            .engrams
            .iter()
            .any(|e| Some(e.id) == mem.engram_id)
        {
            continue;
        }
        let id = format!("c:{}", mem.key);
        nodes.push(GraphNode {
            id: id.clone(),
            label: truncate(&mem.content, 36),
            kind: "core".into(),
            region: "corpus-callosum".into(),
            domain: "core".into(),
            size: 8.0,
            salience: 1.0,
            color: color_for("core").into(),
            desc: Some(mem.content.clone()),
        });
        links.push(GraphLink {
            source: id,
            target: "hub-ca3".into(),
            weight: 0.9,
            kind: "core".into(),
        });
    }

    // Strong synapses between exported neurons (connectome filaments inside the hull).
    let exported_neurons: HashSet<String> = nodes
        .iter()
        .filter(|n| n.kind == "neuron")
        .map(|n| n.id.clone())
        .collect();
    let id_by_neuron: HashMap<NeuronId, String> = neuron_to_engram
        .iter()
        .flat_map(|(nid, eid)| {
            exported_neurons
                .iter()
                .filter(|id| id.contains(&eid[2..10]))
                .map(move |id| (*nid, id.clone()))
        })
        .collect();

    for syn in &graph.synapses {
        let Some(from) = id_by_neuron.get(&syn.from) else {
            continue;
        };
        let Some(to) = id_by_neuron.get(&syn.to) else {
            continue;
        };
        if from == to {
            continue;
        }
        links.push(GraphLink {
            source: from.clone(),
            target: to.clone(),
            weight: syn.weight,
            kind: "synapse".into(),
        });
    }

    let stats = GraphStats {
        engrams: hippocampus.engrams.len(),
        neurons: nodes.iter().filter(|n| n.kind == "neuron").count(),
        synapses: graph.synapse_count(),
        core_memories: core.memories.len(),
    };

    GraphExport {
        nodes,
        links,
        stats,
    }
}

/// Dashboard connectome — engrams, hubs, and associative links only (no per-neuron/synapse mesh).
pub fn export_graph_lite(
    hippocampus: &Hippocampus,
    graph: &BrainGraph,
    core: &CoreMemoryStore,
) -> GraphExport {
    let mut full = export_graph(hippocampus, graph, core);
    full.nodes
        .retain(|n| matches!(n.kind.as_str(), "engram" | "region" | "core"));
    let keep: HashSet<String> = full.nodes.iter().map(|n| n.id.clone()).collect();
    full.links.retain(|l| {
        matches!(l.kind.as_str(), "associate" | "region" | "core")
            && keep.contains(&l.source)
            && keep.contains(&l.target)
    });
    full
}
