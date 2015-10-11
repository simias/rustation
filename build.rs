extern crate gl_generator;
extern crate khronos_api;

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir);

    let mut extensions = Vec::new();
    if cfg!(not(target_os = "macos")) {
        extensions.push("GL_KHR_debug".to_string());
    }

    let mut file = BufWriter::new(File::create(&dest.join("bindings.rs")).unwrap());
    gl_generator::generate_bindings(gl_generator::GlobalGenerator,
                                    gl_generator::registry::Ns::Gl,
                                    gl_generator::Fallbacks::None,
                                    khronos_api::GL_XML,
                                    extensions,
                                    "3.3",
                                    "core",
                                    &mut file).unwrap();
}
