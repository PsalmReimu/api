use std::env;

fn main() {
    if env::var("CI").is_ok() {
        println!("cargo:rustc-cfg=feature=\"ci\"");
    }
}
