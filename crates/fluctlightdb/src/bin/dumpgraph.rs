//! Dump a brain's synapse graph as a flat binary buffer numpy can mmap zero-copy.
//!
//! Reads `graph.seg` directly through the engine's own segment reader — no brain open,
//! so it takes no store lock and a live serve is undisturbed.
//!
//! Record layout, little-endian, tightly packed, 20 bytes per synapse:
//!   u64 from | u64 to | f32 weight
//!
//! Usage: fluctlight-dumpgraph <generation-dir> <out.bin>

use std::io::{BufWriter, Write};

use fluctlightdb::graph::BrainGraph;

fn main() {
    let mut args = std::env::args().skip(1);
    let (dir, out) = match (args.next(), args.next()) {
        (Some(d), Some(o)) => (std::path::PathBuf::from(d), std::path::PathBuf::from(o)),
        _ => {
            eprintln!("usage: fluctlight-dumpgraph <generation-dir> <out.bin>");
            std::process::exit(2);
        }
    };
    let graph: BrainGraph = match fluctlightdb::segment::read_segment(&dir, "graph") {
        Ok(g) => g,
        Err(e) => {
            eprintln!("read graph.seg failed: {e}");
            std::process::exit(1);
        }
    };
    let file = std::fs::File::create(&out).expect("create out");
    let mut w = BufWriter::new(file);
    for s in &graph.synapses {
        w.write_all(&s.from.0.to_le_bytes()).unwrap();
        w.write_all(&s.to.0.to_le_bytes()).unwrap();
        w.write_all(&s.weight.to_le_bytes()).unwrap();
    }
    w.flush().unwrap();

    // Optional: engrams as JSONL (id, content, dg neuron code) so the graph's own
    // structure can be scored against the text that produced it.
    if let Some(eout) = args.next() {
        let hip: fluctlightdb::hippocampus::Hippocampus =
            fluctlightdb::segment::read_segment(&dir, "hippocampus").expect("read hippocampus.seg");
        let f = std::fs::File::create(&eout).expect("create engram out");
        let mut ew = BufWriter::new(f);
        for e in &hip.engrams {
            let dg: Vec<u64> = e.dg_neurons.iter().map(|n| n.0).collect();
            let rec = serde_json::json!({
                "id": e.id.to_string(),
                "content": e.episode.content,
                "context": e.episode.context,
                "tick": e.encoded_at_tick,
                "dg": dg,
            });
            writeln!(ew, "{rec}").unwrap();
        }
        ew.flush().unwrap();
        eprintln!("engrams written: {}", hip.engrams.len());
    }

    println!(
        "{{\"synapses\":{},\"regions\":{},\"bytes\":{},\"out\":\"{}\"}}",
        graph.synapses.len(),
        graph.neuron_regions.len(),
        graph.synapses.len() * 20,
        out.display()
    );
}
