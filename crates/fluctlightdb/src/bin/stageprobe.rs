use std::path::PathBuf;
fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap());
    let dev: fluctlightdb::development::DevelopmentState =
        fluctlightdb::segment::read_segment(&dir, "development").unwrap();
    println!(
        "stage={:?} max_synapses={}",
        dev.stage,
        dev.stage.max_synapses()
    );
}
