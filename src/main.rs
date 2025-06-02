// i wish python could build dlls man...

extern crate sdl2;

use std::{collections::HashSet, fs};

use configparser::ini::Ini;
use imgui::{Condition, ConfigFlags, Context};
use imgui_glow_renderer::{
    AutoRenderer,
    glow::{self, HasContext},
};
use imgui_sdl2_support::SdlPlatform;

#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;

use sdl2::{event::Event, surface::Surface, sys::exit as sdl_exit};
use sdl2::pixels::PixelFormatEnum;

use chrono::{Datelike, Utc};
use toml::Value;
use image::GenericImageView;

use std::fs::File;
use std::io::{BufWriter, Write};
use uuid::Uuid;
use zip::ZipArchive;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{HtmlInputElement, FileReader};
#[cfg(target_arch = "wasm32")]
use web_sys::File as WebFile;
#[cfg(target_arch = "wasm32")]
use once_cell::sync::Lazy;
#[cfg(target_arch = "wasm32")]
use std::sync::Mutex;
#[cfg(target_arch = "wasm32")]
use web_sys::ProgressEvent;
#[cfg(target_arch = "wasm32")]
use js_sys::Uint8Array;

#[cfg(target_arch = "wasm32")]
use std::{cell::RefCell, rc::Rc};
#[cfg(target_arch = "wasm32")]
use web_sys::window;

#[cfg(target_arch = "wasm32")]
static PICKED_FILE: Lazy<Mutex<Option<(String, Vec<u8>)>>> =
    Lazy::new(|| Mutex::new(None));

const CHEATS: &[&str] = &[
    "UseOfficialTitleOnTitleBar",
    "UseArrowsForTimeOfDayTransition",
    "FixUnleashOutOfControlDrain",
    "AllowCancellingUnleash",
    "SkipIntroLogos",
    "SaveScoreAtCheckpoints",
    "DisableBoostFilter",
    "DisableAutoSaveWarning",
    "HUDToggleKey",
    "DisableDWMRoundedCorners",
    "DisableDLCIcon",
    "EnableObjectCollisionDebugView",
    "EnableEventCollisionDebugView",
    "EnableStageCollisionDebugView",
    "EnableGIMipLevelDebugView",
    "FixEggmanlandUsingEventGalleryTransition",
    "DisableDPadMovement",
    "HomingAttackOnJump",
];

#[derive(Clone)]
struct ModEntry {
    id: String,
    path: String,
    title: String,
}

fn show_message_box(msg: String, window: &sdl2::video::Window) {
    _ = sdl2::messagebox::show_simple_message_box(
        sdl2::messagebox::MessageBoxFlag::INFORMATION,
        "Error",
        msg.as_str(),
        window,
    );
}

#[cfg(feature = "xbox_build")]
fn ensure_cpkredir_ini() {
    use std::io::Write;

    let ini_path = Path::new("E:/Unleashed/cpkredir.ini");
    if ini_path.exists() {
        return;
    }

    if let Some(parent) = ini_path.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("Failed to create directory {:?}: {}", parent, e);
                return;
            }
        }
    }

    let content = r#"[CPKREDIR]
Enabled=1
PlaceTocAtEnd=1
HandleCpksWithoutExtFiles=0
LogFile="cpkredir.log"
ReadBlockSizeKB=4096
ModsDbIni="E:\Unleashed\mods\ModsDB.ini"
EnableSaveFileRedirection=0
SaveFileFallback=""
SaveFileOverride=""
LogType=""

[HedgeModManager]
EnableFallbackSaveRedirection=0
ModProfile="Default"
UseLauncher=1
"#;

    match fs::File::create(ini_path).and_then(|mut f| f.write_all(content.as_bytes())) {
        Ok(_) => println!("Created default cpkredir.ini at {}", ini_path.display()),
        Err(e) => eprintln!("Failed to create cpkredir.ini: {}", e),
    }
}


#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}


#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn receive_file(file_name: String, data: Vec<u8>) {
    let mut guard = PICKED_FILE.lock().unwrap();
    *guard = Some((file_name, data));
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn open_file_picker() {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();

    let input: HtmlInputElement = document
        .create_element("input")
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    input.set_type("file");
    input.set_multiple(false);

    let input_for_cb = input.clone();

    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        if let Some(files) = input_for_cb.files() {
            if files.length() > 0 {
                let file: WebFile = files.get(0).unwrap();
                let file_name = file.name();
                let reader = FileReader::new().unwrap();

                let onloadend_cb = {
                    let file_name_clone = file_name.clone();
                    let reader_clone = reader.clone();
                    Closure::wrap(Box::new(move |_: web_sys::ProgressEvent| {
                        let array_buffer = reader_clone.result().unwrap();
                        let uint8_arr = Uint8Array::new(&array_buffer);
                        let mut data = vec![0; uint8_arr.length() as usize];
                        uint8_arr.copy_to(&mut data[..]);

                        receive_file(file_name_clone.clone(), data);
                    }) as Box<dyn FnMut(_)>)
                };

                reader.set_onloadend(Some(onloadend_cb.as_ref().unchecked_ref()));
                reader.read_as_array_buffer(&file).unwrap();
                onloadend_cb.forget();
            }
        }
    }) as Box<dyn FnMut(_)>);

    input.set_onchange(Some(closure.as_ref().unchecked_ref()));
    input.click();

    closure.forget();
}

fn main() {
    let mut ini_path: Option<String> = None;
    let mut cfg = Ini::new();
    let mut active = HashSet::<String>::new();
    let mut cheats = HashSet::<String>::new();
    let mut mods = Vec::<ModEntry>::new();
    let mut show_about = false;
    let mut show_import_zip = false;

    #[cfg(feature = "xbox_build")]
    ensure_cpkredir_ini();

    const RAW_TOML: &str = include_str!("../Cargo.toml");
    let toml: Value = toml::from_str(RAW_TOML).unwrap();
    let package = toml.get("package").unwrap();

    let raw_img = include_bytes!("../Assets/icon.png");
    let img = image::load_from_memory(raw_img).unwrap();
    let rgba = img.to_rgba8();
    let (width, height) = img.dimensions();
    let pitch = width * 4;

    let mut rgba_vec = rgba.into_raw();
    let surface = Surface::from_data(
        rgba_vec.as_mut_slice(),
        width,
        height,
        pitch,
        PixelFormatEnum::RGBA32,
    )
    .expect("Failed to create surface from embedded PNG");

    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    #[cfg(any(feature = "gl_profile_es", target_arch = "wasm32"))]
    {
        let gl_attr = video.gl_attr();
        gl_attr.set_context_profile(sdl2::video::GLProfile::GLES);
        gl_attr.set_context_version(3, 0);
    }

    #[cfg(feature = "gl_profile_core")]
    {
        let a = video.gl_attr();
        a.set_context_profile(sdl2::video::GLProfile::Core);
        a.set_context_version(4, 1);
    }
    #[cfg(not(any(feature = "gl_profile_es", feature = "gl_profile_core")))]
    {
        let a = video.gl_attr();
        a.set_context_profile(sdl2::video::GLProfile::GLES);
        a.set_context_version(3, 0);
    }

    let mut window = video
        .window("USMM", 1280, 720)
        .opengl()
        .allow_highdpi()
        .resizable()
        .position_centered()
        .build()
        .unwrap();
    let _ctx = window.gl_create_context().unwrap();
    window.gl_make_current(&_ctx).unwrap();

    window.set_icon(surface);

    let gl = unsafe { glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as _) };
    let mut ig = Context::create();
    ig.set_ini_filename(None);
    ig.set_log_filename(None);

    let mut platform = SdlPlatform::new(&mut ig);
    let mut renderer = AutoRenderer::new(gl, &mut ig).unwrap();
    let mut pump = sdl.event_pump().unwrap();

    'running: loop {
        for e in pump.poll_iter() {
            platform.handle_event(&mut ig, &e);
            if matches!(e, Event::Quit { .. }) {
                break 'running;
            }
        }

        platform.prepare_frame(&mut ig, &window, &pump);
        ig.io_mut().config_flags |= ConfigFlags::NAV_ENABLE_KEYBOARD;
        ig.io_mut().config_flags |= ConfigFlags::NAV_ENABLE_GAMEPAD;
        let ui = ig.new_frame();
        let gl_version = unsafe { renderer.gl_context().get_parameter_string(glow::VERSION) };
        let renderer_gl = if gl_version.contains("OpenGL ES") {
            "GLES"
        } else {
            "GL"
        };

        let [w, h] = ui.io().display_size;
        ui.window("root")
            .position([0.0, 0.0], Condition::Always)
            .size([w, h], Condition::Always)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            .title_bar(false)
            .menu_bar(true)
            .build(|| {
                if let Some(_mb) = ui.begin_menu_bar() {
                    if let Some(_file) = ui.begin_menu("File") {
                        // OPEN
                        #[cfg(feature = "xbox_build")]
                        if ui.menu_item("Open") {
                            let path = "E:/Unleashed/mods/ModsDB.ini".to_string();
                            ini_path = Some(path.clone());

                            let ini_content = fs::read_to_string(&path);
                            let ini_content = match ini_content {
                                Ok(content) => content,
                                Err(e) => {
                                    show_message_box(
                                        format!("Error opening INI file: {}", e),
                                        &window,
                                    );
                                    return;
                                }
                            };
                            if let Err(e) = cfg.read(ini_content) {
                                show_message_box(format!("Error opening INI file: {}", e), &window);
                                return;
                            }

                            active.clear();
                            if let Some(file) = cfg.get_map() {
                                if let Some(main) = file.get("main") {
                                    if let Some(Some(c)) = main.get("activemodcount") {
                                        if let Ok(n) = c.parse::<usize>() {
                                            for i in 0..n {
                                                let k = format!("activemod{}", i);
                                                if let Some(Some(id)) = main.get(&k) {
                                                    active.insert(id.trim_matches('"').into());
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            cheats.clear();
                            if let Some(file) = cfg.get_map() {
                                if let Some(codes) = file.get("codes") {
                                    for (_k, ov) in codes {
                                        if let Some(v) = ov.as_ref() {
                                            let clean = v.trim_matches('"');
                                            if CHEATS.contains(&clean) {
                                                cheats.insert(clean.to_string());
                                            }
                                        }
                                    }
                                }
                            }

                            mods.clear();
                            if let Some(file) = cfg.get_map() {
                                if let Some(mods_sec) = file.get("mods") {
                                    for (id, opt_path) in mods_sec {
                                        if let Some(p) = opt_path.as_ref() {
                                            let mod_ini = p.trim_matches('"');
                                            let title = fs::read_to_string(mod_ini)
                                                .ok()
                                                .and_then(|txt| {
                                                    let mut m = Ini::new();
                                                    m.read(txt).ok()?;
                                                    m.get("desc", "title")
                                                })
                                                .map(|s| s.trim_matches('"').to_string())
                                                .unwrap_or_else(|| "<NoTitle>".into());
                                            mods.push(ModEntry {
                                                id: id.clone(),
                                                path: mod_ini.into(),
                                                title,
                                            });
                                        }
                                    }
                                }
                            }
                        }


                        #[cfg(target_arch = "wasm32")]
                        {
                            if ui.menu_item("Open") {
                                open_file_picker();
                            }
                            if let Some((file_name, data)) = PICKED_FILE.lock().unwrap().take() {
                                match std::str::from_utf8(&data) {
                                    Ok(ini_str) => {
                                        ini_path = Some(file_name.clone());
                                        if let Err(e) = cfg.read(ini_str.to_string()) {
                                            show_message_box(format!("Error parsing INI: {}", e), &window);
                                        } else {
                                            active.clear();
                                            if let Some(file) = cfg.get_map() {
                                                if let Some(main) = file.get("main") {
                                                    if let Some(Some(c)) = main.get("activemodcount") {
                                                        if let Ok(n) = c.parse::<usize>() {
                                                            for i in 0..n {
                                                                let key = format!("activemod{}", i);
                                                                if let Some(Some(id)) = main.get(&key) {
                                                                    active.insert(id.trim_matches('"').into());
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            cheats.clear();
                                            if let Some(file) = cfg.get_map() {
                                                if let Some(codes) = file.get("codes") {
                                                    for (_k, ov) in codes {
                                                        if let Some(v) = ov.as_ref() {
                                                            let clean = v.trim_matches('"');
                                                            if CHEATS.contains(&clean) {
                                                                cheats.insert(clean.to_string());
                                                            }
                                                        }
                                                    }
                                                }
                                            }
       
                                            mods.clear();
                                            if let Some(file) = cfg.get_map() {
                                                if let Some(mods_sec) = file.get("mods") {
                                                    for (id, opt_path) in mods_sec {
                                                        if let Some(p) = opt_path.as_ref() {
                                                            mods.push(ModEntry {
                                                                id: id.clone(),
                                                                path: p.trim_matches('"').to_string(),
                                                                title: "<NoTitle>".into(),
                                                            });
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        show_message_box(
                                            format!("Picked file is not valid UTF-8: {}", err),
                                            &window,
                                        );
                                    }
                                }
                            }
                        }

                        #[cfg(all(not(feature = "xbox_build"), not(target_arch = "wasm32")))]
                        if ui.menu_item("Open") {
                            if let Some(pb) = FileDialog::new()
                                .add_filter("INI files", &["ini"])
                                .pick_file()
                            {
                                let path = pb.to_string_lossy().into_owned();
                                ini_path = Some(path.clone());

                                let ini_content = fs::read_to_string(&path);
                                let ini_content = match ini_content {
                                    Ok(content) => content,
                                    Err(e) => {
                                        show_message_box(
                                            format!("Error opening INI file: {}", e),
                                            &window,
                                        );
                                        return;
                                    }
                                };
                                if let Err(e) = cfg.read(ini_content) {
                                    show_message_box(
                                        format!("Error opening INI file: {}", e),
                                        &window,
                                    );
                                    return;
                                }

                                active.clear();
                                if let Some(file) = cfg.get_map() {
                                    if let Some(main) = file.get("main") {
                                        if let Some(Some(c)) = main.get("activemodcount") {
                                            if let Ok(n) = c.parse::<usize>() {
                                                for i in 0..n {
                                                    let k = format!("activemod{}", i);
                                                    if let Some(Some(id)) = main.get(&k) {
                                                        active.insert(id.trim_matches('"').into());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                cheats.clear();
                                if let Some(file) = cfg.get_map() {
                                    if let Some(codes) = file.get("codes") {
                                        for (_k, ov) in codes {
                                            if let Some(v) = ov.as_ref() {
                                                let clean = v.trim_matches('"');
                                                if CHEATS.contains(&clean) {
                                                    cheats.insert(clean.to_string());
                                                }
                                            }
                                        }
                                    }
                                }

                                mods.clear();
                                if let Some(file) = cfg.get_map() {
                                    if let Some(mods_sec) = file.get("mods") {
                                        for (id, opt_path) in mods_sec {
                                            if let Some(p) = opt_path.as_ref() {
                                                let mod_ini = p.trim_matches('"');
                                                let title = fs::read_to_string(mod_ini)
                                                    .ok()
                                                    .and_then(|txt| {
                                                        let mut m = Ini::new();
                                                        m.read(txt).ok()?;
                                                        m.get("desc", "title")
                                                    })
                                                    .map(|s| s.trim_matches('"').to_string())
                                                    .unwrap_or_else(|| "<NoTitle>".into());
                                                mods.push(ModEntry {
                                                    id: id.clone(),
                                                    path: mod_ini.into(),
                                                    title,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // SAVE
                        #[cfg(feature = "xbox_build")]
                        if ui.menu_item("Save") {
                            let path = "E:/Unleashed/mods/ModsDB.ini".to_string();
                            ini_path = Some(path.clone());

                            if let Some(file) = cfg.get_map() {
                                if let Some(main) = file.get("main") {
                                    for key in main
                                        .keys()
                                        .filter(|k| {
                                            k.starts_with("activemod") || *k == "activemodcount"
                                        })
                                        .cloned()
                                        .collect::<Vec<_>>()
                                    {
                                        cfg.remove_key("main", &key);
                                    }
                                }
                                if let Some(codes) = file.get("codes") {
                                    for key in codes
                                        .keys()
                                        .filter(|k| k.starts_with("code") || *k == "codecount")
                                        .cloned()
                                        .collect::<Vec<_>>()
                                    {
                                        cfg.remove_key("codes", &key);
                                    }
                                }
                            }
                            cfg.set("main", "activemodcount", Some(active.len().to_string()));
                            for (i, id) in active.iter().enumerate() {
                                cfg.set(
                                    "main",
                                    &format!("activemod{}", i),
                                    Some(format!("\"{}\"", id)),
                                );
                            }
                            cfg.set("codes", "codecount", Some(cheats.len().to_string()));
                            for (i, c) in cheats.iter().enumerate() {
                                cfg.set("codes", &format!("code{}", i), Some(format!("\"{}\"", c)));
                            }

                            let fmt_main = |k: &str| -> String {
                                if k.starts_with("activemod") {
                                    let suffix = &k["activemod".len()..];
                                    match suffix {
                                        "count" => "ActiveModCount".into(),
                                        digits if digits.chars().all(char::is_numeric) => {
                                            format!("ActiveMod{}", digits)
                                        }
                                        other => format!(
                                            "ActiveMod{}",
                                            other
                                                .chars()
                                                .next()
                                                .unwrap()
                                                .to_uppercase()
                                                .chain(other.chars().skip(1))
                                                .collect::<String>()
                                        ),
                                    }
                                } else if k == "manifestversion" {
                                    "ManifestVersion".into()
                                } else if k == "reverseloadorder" {
                                    "ReverseLoadOrder".into()
                                } else if k == "favoritemodcount" {
                                    "FavoriteModCount".into()
                                } else {
                                    let mut c = k.chars();
                                    if let Some(f) = c.next() {
                                        f.to_uppercase().collect::<String>() + c.as_str()
                                    } else {
                                        k.into()
                                    }
                                }
                            };
                            let fmt_code = |k: &str| -> String {
                                if k.starts_with("code") {
                                    let suffix = &k["code".len()..];
                                    match suffix {
                                        "count" => "CodeCount".into(),
                                        digits if digits.chars().all(char::is_numeric) => {
                                            format!("Code{}", digits)
                                        }
                                        other => format!(
                                            "Code{}",
                                            other
                                                .chars()
                                                .next()
                                                .unwrap()
                                                .to_uppercase()
                                                .chain(other.chars().skip(1))
                                                .collect::<String>()
                                        ),
                                    }
                                } else {
                                    let mut c = k.chars();
                                    if let Some(f) = c.next() {
                                        f.to_uppercase().collect::<String>() + c.as_str()
                                    } else {
                                        k.into()
                                    }
                                }
                            };

                            match File::create(path) {
                                Ok(f) => {
                                    let mut buf = BufWriter::new(f);
                                    let map = cfg.get_map().unwrap();

                                    if let Some(sec) = map.get("main") {
                                        writeln!(buf, "[Main]").unwrap();
                                        for (k, v_opt) in sec {
                                            if let Some(v) = v_opt {
                                                writeln!(buf, "{}={}", fmt_main(k), v).unwrap();
                                            }
                                        }
                                        writeln!(buf).unwrap();
                                    }
                                    if let Some(sec) = map.get("mods") {
                                        writeln!(buf, "[Mods]").unwrap();
                                        for (k, v_opt) in sec {
                                            if let Some(v) = v_opt {
                                                writeln!(buf, "{}={}", k, v).unwrap();
                                            }
                                        }
                                        writeln!(buf).unwrap();
                                    }
                                    if let Some(sec) = map.get("codes") {
                                        writeln!(buf, "[Codes]").unwrap();
                                        for (k, v_opt) in sec {
                                            if let Some(v) = v_opt {
                                                writeln!(buf, "{}={}", fmt_code(k), v).unwrap();
                                            }
                                        }
                                        writeln!(buf).unwrap();
                                    }
                                }
                                Err(e) => {
                                    show_message_box(
                                        format!("Failed opening INI for write: {}", e),
                                        &window,
                                    );
                                }
                            }
                        }
                        #[cfg(not(feature = "xbox_build"))]
                        if ini_path.is_some() && ui.menu_item("Save") {
                            if let Some(ref path) = ini_path {
                                if let Some(file) = cfg.get_map() {
                                    if let Some(main) = file.get("main") {
                                        for key in main
                                            .keys()
                                            .filter(|k| {
                                                k.starts_with("activemod") || *k == "activemodcount"
                                            })
                                            .cloned()
                                            .collect::<Vec<_>>()
                                        {
                                            cfg.remove_key("main", &key);
                                        }
                                    }
                                    if let Some(codes) = file.get("codes") {
                                        for key in codes
                                            .keys()
                                            .filter(|k| k.starts_with("code") || *k == "codecount")
                                            .cloned()
                                            .collect::<Vec<_>>()
                                        {
                                            cfg.remove_key("codes", &key);
                                        }
                                    }
                                }
                                cfg.set("main", "activemodcount", Some(active.len().to_string()));
                                for (i, id) in active.iter().enumerate() {
                                    cfg.set(
                                        "main",
                                        &format!("activemod{}", i),
                                        Some(format!("\"{}\"", id)),
                                    );
                                }
                                cfg.set("codes", "codecount", Some(cheats.len().to_string()));
                                for (i, c) in cheats.iter().enumerate() {
                                    cfg.set(
                                        "codes",
                                        &format!("code{}", i),
                                        Some(format!("\"{}\"", c)),
                                    );
                                }

                                let fmt_main = |k: &str| -> String {
                                    if k.starts_with("activemod") {
                                        let suffix = &k["activemod".len()..];
                                        match suffix {
                                            "count" => "ActiveModCount".into(),
                                            digits if digits.chars().all(char::is_numeric) => {
                                                format!("ActiveMod{}", digits)
                                            }
                                            other => format!(
                                                "ActiveMod{}",
                                                other
                                                    .chars()
                                                    .next()
                                                    .unwrap()
                                                    .to_uppercase()
                                                    .chain(other.chars().skip(1))
                                                    .collect::<String>()
                                            ),
                                        }
                                    } else if k == "manifestversion" {
                                        "ManifestVersion".into()
                                    } else if k == "reverseloadorder" {
                                        "ReverseLoadOrder".into()
                                    } else if k == "favoritemodcount" {
                                        "FavoriteModCount".into()
                                    } else {
                                        let mut c = k.chars();
                                        if let Some(f) = c.next() {
                                            f.to_uppercase().collect::<String>() + c.as_str()
                                        } else {
                                            k.into()
                                        }
                                    }
                                };
                                let fmt_code = |k: &str| -> String {
                                    if k.starts_with("code") {
                                        let suffix = &k["code".len()..];
                                        match suffix {
                                            "count" => "CodeCount".into(),
                                            digits if digits.chars().all(char::is_numeric) => {
                                                format!("Code{}", digits)
                                            }
                                            other => format!(
                                                "Code{}",
                                                other
                                                    .chars()
                                                    .next()
                                                    .unwrap()
                                                    .to_uppercase()
                                                    .chain(other.chars().skip(1))
                                                    .collect::<String>()
                                            ),
                                        }
                                    } else {
                                        let mut c = k.chars();
                                        if let Some(f) = c.next() {
                                            f.to_uppercase().collect::<String>() + c.as_str()
                                        } else {
                                            k.into()
                                        }
                                    }
                                };

                                match File::create(path) {
                                    Ok(f) => {
                                        let mut buf = BufWriter::new(f);
                                        let map = cfg.get_map().unwrap();

                                        if let Some(sec) = map.get("main") {
                                            writeln!(buf, "[Main]").unwrap();
                                            for (k, v_opt) in sec {
                                                if let Some(v) = v_opt {
                                                    writeln!(buf, "{}={}", fmt_main(k), v).unwrap();
                                                }
                                            }
                                            writeln!(buf).unwrap();
                                        }
                                        if let Some(sec) = map.get("mods") {
                                            writeln!(buf, "[Mods]").unwrap();
                                            for (k, v_opt) in sec {
                                                if let Some(v) = v_opt {
                                                    writeln!(buf, "{}={}", k, v).unwrap();
                                                }
                                            }
                                            writeln!(buf).unwrap();
                                        }
                                        if let Some(sec) = map.get("codes") {
                                            writeln!(buf, "[Codes]").unwrap();
                                            for (k, v_opt) in sec {
                                                if let Some(v) = v_opt {
                                                    writeln!(buf, "{}={}", fmt_code(k), v).unwrap();
                                                }
                                            }
                                            writeln!(buf).unwrap();
                                        }
                                    }
                                    Err(e) => {
                                        show_message_box(
                                            format!("Failed opening INI for write: {}", e),
                                            &window,
                                        );
                                    }
                                }
                            }
                        }

                        #[cfg(feature = "xbox_build")]
                        if ini_path.is_some() && ui.menu_item("Import Zip") {
                            show_import_zip = true;
                        }

                        #[cfg(target_arch = "wasm32")]
                        if ini_path.is_some() && ui.menu_item("Import Zip") {
                            open_file_picker();
                        }
                        #[cfg(target_arch = "wasm32")]
                        {
                            if let Some((file_name, data)) = PICKED_FILE.lock().unwrap().take() {
                                if file_name.to_lowercase().ends_with(".zip") {
                                    let reader = std::io::Cursor::new(data);
                                    match ZipArchive::new(reader) {
                                        Ok(mut archive) => {
                                            let stem = file_name.trim_end_matches(".zip");
                                            let extract_dir = std::path::Path::new("/")
                                                .join(stem);

                                            if let Err(e) = std::fs::create_dir_all(&extract_dir) {
                                                show_message_box(
                                                    format!("Failed to create dir: {}", e),
                                                    &window,
                                                );
                                            }

                                            for i in 0..archive.len() {
                                                if let Ok(mut file) = archive.by_index(i) {
                                                    let outpath = file.sanitized_name();
                                                    let dest_path = extract_dir.join(&outpath);
                                                    if let Some(parent) = dest_path.parent() {
                                                        let _ = std::fs::create_dir_all(parent);
                                                    }
                                                    if let Ok(mut outfile) =
                                                        File::create(&dest_path)
                                                    {
                                                        let _ = std::io::copy(&mut file, &mut outfile);
                                                    }
                                                }
                                            }

                                            let mut mod_ini_path = extract_dir.join("mod.ini");
                                            if !mod_ini_path.exists() {
                                                if let Ok(entries) =
                                                    std::fs::read_dir(&extract_dir)
                                                {
                                                    for entry in entries.flatten() {
                                                        let p = entry.path();
                                                        if p.is_dir() && p.join("mod.ini").exists() {
                                                            mod_ini_path = p.join("mod.ini");
                                                            break;
                                                        }
                                                    }
                                                }
                                            }

                                            if !mod_ini_path.exists() {
                                                show_message_box(
                                                    "Extracted zip does not contain a mod.ini"
                                                        .into(),
                                                    &window,
                                                );
                                            } else {
                                                let id = Uuid::new_v4().to_string();
                                                let mod_ini_str = mod_ini_path
                                                    .to_string_lossy()
                                                    .replace("\\", "/");
                                                cfg.set("mods", &id, Some(format!("\"{}\"", mod_ini_str)));

                                                if let Some(ref path) = ini_path {
                                                    let _ = cfg.write(path);
                                                }

                                                let title = std::fs::read_to_string(&mod_ini_path)
                                                    .ok()
                                                    .and_then(|txt| {
                                                        let mut m2 = Ini::new();
                                                        m2.read(txt).ok()?;
                                                        m2.get("desc", "title")
                                                    })
                                                    .map(|s| s.trim_matches('"').to_string())
                                                    .unwrap_or_else(|| "<NoTitle>".into());

                                                mods.push(ModEntry {
                                                    id: id.clone(),
                                                    path: mod_ini_str,
                                                    title,
                                                });
                                            }
                                        }
                                        Err(e) => {
                                            show_message_box(
                                                format!("Failed to read zip: {}", e),
                                                &window,
                                            );
                                        }
                                    }
                                } else {
                                    show_message_box("Picked file is not a .zip".into(), &window);
                                }
                            }
                        }

    
                        #[cfg(all(not(feature = "xbox_build"), not(target_arch = "wasm32")))]
                        if ini_path.is_some() && ui.menu_item("Import Zip") {
                            if let Some(zip_path) = FileDialog::new()
                                .add_filter("ZIP files", &["zip"])
                                .pick_file()
                            {
                                let file = match File::open(&zip_path) {
                                    Ok(f) => f,
                                    Err(e) => {
                                        show_message_box(
                                            format!("Failed to open zip: {}", e),
                                            &window,
                                        );
                                        return;
                                    }
                                };
                                let mut archive = match ZipArchive::new(file) {
                                    Ok(a) => a,
                                    Err(e) => {
                                        show_message_box(
                                            format!("Failed to read zip: {}", e),
                                            &window,
                                        );
                                        return;
                                    }
                                };

                                #[cfg(feature = "xbox_build")]
                                let extract_dir = std::path::Path::new("E:/Unleashed/mods")
                                    .join(zip_path.file_stem().unwrap());
                                #[cfg(not(feature = "xbox_build"))]
                                let extract_dir = {
                                    if let Some(dir) = FileDialog::new().pick_folder() {
                                        dir.join(zip_path.file_stem().unwrap())
                                    } else {
                                        return;
                                    }
                                };

                                if let Err(e) = fs::create_dir_all(&extract_dir) {
                                    show_message_box(
                                        format!("Failed to create dir: {}", e),
                                        &window,
                                    );
                                    return;
                                }
                                if let Err(e) = archive.extract(&extract_dir) {
                                    show_message_box(
                                        format!("Failed to extract zip: {}", e),
                                        &window,
                                    );
                                    return;
                                }

                                if let Ok(entries) = fs::read_dir(&extract_dir) {
                                    let entries: Vec<_> =
                                        entries.filter_map(|e| e.ok()).collect();
                                    if entries.len() == 1 && entries[0].path().is_dir() {
                                        let top = entries[0].path();
                                        if let Ok(subs) = fs::read_dir(&top) {
                                            for se in subs.filter_map(|e| e.ok()) {
                                                let dest = extract_dir.join(se.file_name());
                                                if let Err(e) =
                                                    fs::rename(se.path(), &dest)
                                                {
                                                    show_message_box(
                                                        format!(
                                                            "Failed moving {:?}: {}",
                                                            se.path(),
                                                            e
                                                        ),
                                                        &window,
                                                    );
                                                    return;
                                                }
                                            }
                                        }
                                        if let Err(e) = fs::remove_dir_all(&top) {
                                            show_message_box(
                                                format!(
                                                    "Failed removing folder {:?}: {}",
                                                    top, e
                                                ),
                                                &window,
                                            );
                                            return;
                                        }
                                    }
                                }

                                let mut mod_ini_path = extract_dir.join("mod.ini");
                                if !mod_ini_path.exists() {
                                    if let Ok(entries) = fs::read_dir(&extract_dir) {
                                        for entry in entries.flatten() {
                                            let path = entry.path();
                                            if path.is_dir() && path.join("mod.ini").exists() {
                                                mod_ini_path = path.join("mod.ini");
                                                break;
                                            }
                                        }
                                    }
                                }
                                if !mod_ini_path.exists() {
                                    show_message_box(
                                        "Extracted zip does not contain a mod.ini".into(),
                                        &window,
                                    );
                                    return;
                                }

                                let id = Uuid::new_v4().to_string();
                                let mod_ini_str =
                                    mod_ini_path.to_string_lossy().replace("\\", "/");
                                cfg.set("mods", &id, Some(format!("\"{}\"", mod_ini_str)));

                                if let Some(ref path) = ini_path {
                                    if let Err(e) = cfg.write(path) {
                                        show_message_box(
                                            format!("Failed to update INI: {}", e),
                                            &window,
                                        );
                                    }
                                }

                                let title = fs::read_to_string(&mod_ini_path)
                                    .ok()
                                    .and_then(|txt| {
                                        let mut m2 = Ini::new();
                                        m2.read(txt).ok()?;
                                        m2.get("desc", "title")
                                    })
                                    .map(|s| s.trim_matches('"').to_string())
                                    .unwrap_or_else(|| "<NoTitle>".into());

                                mods.push(ModEntry {
                                    id: id.clone(),
                                    path: mod_ini_str,
                                    title,
                                });
                            }
                        }

                    }

                    if ui.menu_item("About") {
                        show_about = !show_about;
                    }
                }


                #[cfg(feature = "xbox_build")]
                if show_import_zip {
                    let window_size = [400.0, 300.0];
                    let display_size = ui.io().display_size;
                    let pos = [
                        (display_size[0] - window_size[0]) / 2.0,
                        (display_size[1] - window_size[1]) / 2.0,
                    ];
                    ui.window("Import ZIP")
                        .size(window_size, Condition::Always)
                        .position(pos, Condition::Always)
                        .movable(false)
                        .resizable(false)
                        .collapsible(false)
                        .build(|| {
                            ui.text("Select a ZIP file from E:/USMM:");
                            ui.separator();

                            if let Ok(entries) = fs::read_dir("E:/USMM") {
                                for entry in entries.flatten() {
                                    let path = entry.path();
                                    if let Some(ext) = path.extension() {
                                        if ext == "zip" {
                                            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                                                if ui.selectable(filename) {
                                                    let zip_path = path.clone();
                                                    if let Ok(file) = File::open(&zip_path) {
                                                        if let Ok(mut archive) = ZipArchive::new(file) {
                                                            let extract_dir = Path::new("E:/Unleashed/mods")
                                                                .join(zip_path.file_stem().unwrap());
                                                            if fs::create_dir_all(&extract_dir).is_ok() {
                                                                if archive.extract(&extract_dir).is_ok() {
                                                                    if let Ok(subs) = fs::read_dir(&extract_dir) {
                                                                        let subs_list: Vec<_> = subs.filter_map(|e| e.ok()).collect();
                                                                        if subs_list.len() == 1 && subs_list[0].path().is_dir() {
                                                                            let top = subs_list[0].path();
                                                                            if let Ok(deeper) = fs::read_dir(&top) {
                                                                                for se in deeper.filter_map(|e| e.ok()) {
                                                                                    let dest = extract_dir.join(se.file_name());
                                                                                    let _ = fs::rename(se.path(), &dest);
                                                                                }
                                                                            }
                                                                            let _ = fs::remove_dir_all(&top);
                                                                        }
                                                                    }
                                                                    let mut mod_ini_path = extract_dir.join("mod.ini");
                                                                    if !mod_ini_path.exists() {
                                                                        if let Ok(entries2) = fs::read_dir(&extract_dir) {
                                                                            for entry2 in entries2.flatten() {
                                                                                let p2 = entry2.path();
                                                                                if p2.is_dir() && p2.join("mod.ini").exists() {
                                                                                    mod_ini_path = p2.join("mod.ini");
                                                                                    break;
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                    if mod_ini_path.exists() {
                                                                        let id = Uuid::new_v4().to_string();
                                                                        let mod_ini_str = mod_ini_path.to_string_lossy().replace("\\", "/");
                                                                        cfg.set("mods", &id, Some(format!("\"{}\"", mod_ini_str)));
                                                                        if let Some(ref path) = ini_path {
                                                                            let _ = cfg.write(path);
                                                                        }
                                                                        let title = fs::read_to_string(&mod_ini_path)
                                                                            .ok()
                                                                            .and_then(|txt| {
                                                                                let mut m2 = Ini::new();
                                                                                m2.read(txt).ok()?;
                                                                                m2.get("desc", "title")
                                                                            })
                                                                            .map(|s| s.trim_matches('"').to_string())
                                                                            .unwrap_or_else(|| "<NoTitle>".into());
                                                                        mods.push(ModEntry {
                                                                            id: id.clone(),
                                                                            path: mod_ini_str,
                                                                            title,
                                                                        });
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    show_import_zip = false;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            ui.separator();
                            if ui.button("Cancel") {
                                show_import_zip = false;
                            }
                        });
                }

                ui.separator();
                ui.text("Mods:");
                for m in &mods {
                    let mut chk = active.contains(&m.id);
                    let label = format!("{}  :  {}", m.title, m.id);
                    if ui.checkbox(&label, &mut chk) {
                        if chk {
                            active.insert(m.id.clone());
                        } else {
                            active.remove(&m.id);
                        }
                    }
                }

                if ini_path.is_some() {
                    ui.separator();
                    ui.text("Cheats:");
                    for &c in CHEATS {
                        let mut chk = cheats.contains(c);
                        if ui.checkbox(c, &mut chk) {
                            if chk {
                                cheats.insert(c.to_string());
                            } else {
                                cheats.remove(c);
                            }
                        }
                    }
                }

                if show_about {
                    ui.window("About")
                        .size([300.0, 210.0], Condition::FirstUseEver)
                        .resizable(false)
                        .opened(&mut show_about)
                        .build(|| {
                            ui.text(format!(
                                "{:?} Mod-Manager",
                                package.get("name").unwrap().as_str().unwrap()
                            ));
                            ui.text(format!("Version   : {}", package.get("version").unwrap()));
                            ui.text(format!(
                                "Author    : {}",
                                package.get("authors").unwrap().as_array().unwrap()[0]
                                    .as_str()
                                    .unwrap()
                            ));
                            ui.text(format!("Platform  :  {}", sdl2::get_platform()));
                            ui.separator();
                            ui.text("OpenGL info");
                            ui.text(format!("Renderer  :  {}", renderer_gl));
                            ui.text(format!("Version   :  {}", gl_version));
                            ui.text(format!("Vendor    :  {}", unsafe {
                                renderer.gl_context().get_parameter_string(glow::VENDOR)
                            }));
                            ui.separator();
                            ui.text("Made with Rust, SDL2, ImGui & Glow.");
                            ui.text(format!(
                                "(C) {}, {}.",
                                Utc::now().year(),
                                package.get("authors").unwrap().as_array().unwrap()[0]
                                    .as_str()
                                    .unwrap()
                            ))
                        });
                }
            });

        let dd = ig.render();
        unsafe {
            renderer.gl_context().clear(glow::COLOR_BUFFER_BIT);
        }
        renderer.render(dd).unwrap();
        window.gl_swap_window();
    }

    unsafe {
        sdl_exit(0);
    }
}
