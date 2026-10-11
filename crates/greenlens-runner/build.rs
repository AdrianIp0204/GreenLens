#[path = "../../scripts/build-provenance.rs"]
mod build_provenance;

fn main() {
    build_provenance::emit();
}
