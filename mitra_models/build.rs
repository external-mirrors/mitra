// https://github.com/rust-db/refinery/issues/309
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
