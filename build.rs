// take emscriptens hand and show it how to build the project (INCOMPLETE)
use std::env;

fn main() {
    if let Ok(target) = env::var("TARGET") {
        if target == "wasm32-unknown-emscripten" {
            println!("cargo:rustc-link-arg=-sUSE_SDL=2");
            println!("cargo:rustc-link-arg=-sUSE_SDL_IMAGE=2");
            println!("cargo:rustc-link-arg=-sUSE_SDL_MIXER=2");
            println!("cargo:rustc-link-arg=-sSDL2_IMAGE_FORMATS=['png','jpg','bmp','gif','tiff']");
            println!("cargo:rustc-link-arg=-sUSE_SDL_NET=2");
            println!("cargo:rustc-link-arg=-sUSE_WEBGL2=1");
            println!("cargo:rustc-link-arg=-sMAX_WEBGL_VERSION=2");
            println!("cargo:rustc-link-arg=-sMIN_WEBGL_VERSION=1");
            println!("cargo:rustc-link-arg=-sALLOW_MEMORY_GROWTH=1");
            println!("cargo:rustc-link-arg=-o");
            println!("cargo:rustc-link-arg=index.html");
        }
    }
}