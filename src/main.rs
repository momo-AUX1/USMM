extern crate sdl2;

use std::{collections::HashSet, fs};

use configparser::ini::Ini;
use imgui::{Condition, ConfigFlags, Context};
use imgui_glow_renderer::{
    AutoRenderer,
    glow::{self, HasContext},
};
use imgui_sdl2_support::SdlPlatform;
use rfd::FileDialog;
use sdl2::{event::Event, sys::exit as sdl_exit};

use toml::Value;
use chrono::{Datelike, Utc};

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
];

#[derive(Clone)]
struct ModEntry {
    id: String,
    path: String,
    title: String,
}

fn main() {
    let mut ini_path: Option<String> = None;
    let mut cfg = Ini::new();
    let mut active = HashSet::<String>::new();
    let mut cheats = HashSet::<String>::new();
    let mut mods = Vec::<ModEntry>::new();
    let mut show_about = false; 

    let raw_toml = fs::read_to_string("Cargo.toml").unwrap();
    let toml: Value = toml::from_str(&raw_toml).unwrap();
    let package = toml.get("package").unwrap();

    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    {
        let a = video.gl_attr();
        a.set_context_profile(sdl2::video::GLProfile::Core);
        a.set_context_version(4, 1);
    }

    let window = video
        .window("UMSS", 1280, 720)
        .opengl()
        .allow_highdpi()
        .resizable()
        .position_centered()
        .build()
        .unwrap();
    let _ctx = window.gl_create_context().unwrap();
    window.gl_make_current(&_ctx).unwrap();

    let gl = unsafe { glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as _) };
    let mut ig = Context::create();
    ig.set_ini_filename(None);

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
        let ui = ig.new_frame();
        let gl_version = unsafe{renderer.gl_context().get_parameter_string(glow::VERSION)};
        let renderer_gl = if gl_version.contains("OpenGL ES") { "GLES" } else { "GL" };

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
                        if ui.menu_item("Open") {
                            if let Some(pb) = FileDialog::new()
                                .add_filter("INI files", &["ini"])
                                .pick_file()
                            {
                                let path = pb.to_string_lossy().into_owned();
                                ini_path = Some(path.clone());
                                cfg.read(fs::read_to_string(&path).unwrap()).unwrap();

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

                        if ui.menu_item("Save") {
                            if let Some(ref path) = ini_path {
                                if let Some(file) = cfg.get_map() {
                                    if let Some(main) = file.get("main") {
                                        for k in main
                                            .keys()
                                            .filter(|k| k.starts_with("activemod"))
                                            .cloned()
                                            .collect::<Vec<_>>()
                                        {
                                            cfg.remove_key("main", &k);
                                        }
                                    }
                                    if let Some(codes) = file.get("codes") {
                                        for k in codes
                                            .keys()
                                            .filter(|k| k.starts_with("code"))
                                            .cloned()
                                            .collect::<Vec<_>>()
                                        {
                                            cfg.remove_key("codes", &k);
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
                                cfg.write(path).unwrap();
                            }
                        }
                    }

                    if ui.menu_item("About") {
                        show_about = !show_about;
                    }
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
                            ui.text("UMSS Mod-Manager");
                            ui.text(format!("Version   : {}", package.get("version").unwrap()));
                            ui.text(format!("Author    : {}", package.get("author").unwrap()));
                            ui.text(format!("Platform  :  {}", sdl2::get_platform()));
                            ui.separator();
                            ui.text("OpenGL info");
                            ui.text(format!("Renderer  :  {}", renderer_gl));
                            ui.text(format!("Version   :  {}", gl_version));
                            ui.text(format!("Vendor    :  {}", unsafe {renderer.gl_context().get_parameter_string(glow::VENDOR)}));
                            ui.separator();
                            ui.text("Made with Rust, SDL2, ImGui & glow.");
                            ui.text(format!("(C) {}, {}.", Utc::now().year(), package.get("author").unwrap()))
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

    unsafe { sdl_exit(0) }
}
