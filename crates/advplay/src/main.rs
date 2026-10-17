//! Desktop player. The window shows the 384×216 framebuffer with integer
//! nearest-neighbour scaling. Audio and the clock stay on this side of the core.

use lastcall_core::{Engine, HostEvent, Input};
use rodio::Source;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("advplay: {err}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("-h") | Some("--help") => {
            println!(
                "usage:\n  advplay [game.advpak]\n  advplay --dump-frame <game.advpak> [--expect boot.sha256]"
            );
            Ok(())
        }
        Some("--dump-frame") => {
            let pak = args
                .next()
                .ok_or("usage: advplay --dump-frame <game.advpak>")?;
            let expect = match args.next().as_deref() {
                None => None,
                Some("--expect") => Some(args.next().ok_or("usage: --expect <file>")?),
                Some(other) => return Err(format!("unknown flag {other}")),
            };
            dump_frame(Path::new(&pak), expect.as_deref().map(Path::new))
        }
        Some(path) if path.starts_with('-') => Err(format!("unknown flag {path}")),
        Some(path) => play(PathBuf::from(path)),
        None => play(find_package()?),
    }
}

fn dump_frame(pak: &Path, expect: Option<&Path>) -> Result<(), String> {
    let engine = load_engine(pak)?;
    let hash = engine.frame_hash();
    println!("{hash}");
    if let Some(path) = expect {
        let expected = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
        if expected.trim() != hash {
            return Err(format!(
                "frame hash mismatch\n  expected {}\n  actual   {hash}",
                expected.trim()
            ));
        }
    }
    Ok(())
}

fn find_package() -> Result<PathBuf, String> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join("../Resources/sample.advpak");
            if bundled.is_file() {
                return Ok(bundled);
            }
        }
    }
    rfd::FileDialog::new()
        .set_title("Open an adventure package")
        .add_filter("Adventure package", &["advpak"])
        .pick_file()
        .ok_or_else(|| "no package selected".into())
}

fn load_engine(path: &Path) -> Result<Engine, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
    Engine::load(&bytes).map_err(|err| err.to_string())
}

fn play(pak: PathBuf) -> Result<(), String> {
    let engine = load_engine(&pak)?;
    let title = format!(
        "Last Call — {}",
        pak.file_name().and_then(|n| n.to_str()).unwrap_or("game")
    );
    let audio = Audio::open();
    let event_loop = EventLoop::new().map_err(|err| err.to_string())?;
    event_loop.set_control_flow(ControlFlow::wait_duration(Duration::from_millis(8)));
    let mut app = App {
        pak,
        title,
        engine: Some(engine),
        audio,
        window: None,
        context: None,
        surface: None,
        last: Instant::now(),
        acc_ms: 0.0,
    };
    event_loop.run_app(&mut app).map_err(|err| err.to_string())
}

struct Audio {
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
}

impl Audio {
    fn open() -> Option<Self> {
        let (stream, handle) = rodio::OutputStream::try_default().ok()?;
        Some(Self {
            _stream: stream,
            handle,
        })
    }

    fn beep(&self) {
        let Ok(sink) = rodio::Sink::try_new(&self.handle) else {
            return;
        };
        let tone = rodio::source::SineWave::new(660.0)
            .take_duration(Duration::from_millis(70))
            .amplify(0.12);
        sink.append(tone);
        sink.detach();
    }
}

struct App {
    pak: PathBuf,
    title: String,
    engine: Option<Engine>,
    audio: Option<Audio>,
    window: Option<Arc<Window>>,
    context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    last: Instant,
    acc_ms: f64,
}

impl App {
    fn engine(&mut self) -> &mut Engine {
        self.engine.as_mut().expect("engine")
    }

    fn pump(&mut self) {
        let events = self.engine().drain_events();
        for event in events {
            match event {
                HostEvent::Sfx { .. } => {
                    if let Some(audio) = &self.audio {
                        audio.beep();
                    }
                }
                HostEvent::Ui { action, slot } => match action.as_str() {
                    "save" => self.write_save(&slot),
                    "restore" => self.read_save(&slot),
                    _ => {}
                },
                HostEvent::Autosave => self.write_save("autosave"),
                HostEvent::Warning { message } => eprintln!("advplay: {message}"),
                HostEvent::Say { .. } | HostEvent::Music { .. } | HostEvent::RoomChanged { .. } => {
                }
            }
        }
    }

    fn save_path(&self, slot: &str) -> PathBuf {
        let stem = self
            .pak
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("game");
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".lastcall")
            .join("saves")
            .join(stem)
            .join(format!("{slot}.json"))
    }

    fn write_save(&mut self, slot: &str) {
        let path = self.save_path(slot);
        match self.engine().save() {
            Ok(bytes) => {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(err) = std::fs::write(&path, bytes) {
                    eprintln!("advplay: could not write {}: {err}", path.display());
                }
            }
            Err(err) => eprintln!("advplay: save failed: {err}"),
        }
    }

    fn read_save(&mut self, slot: &str) {
        let path = self.save_path(slot);
        match std::fs::read(&path) {
            Ok(bytes) => {
                if let Err(err) = self.engine().restore(&bytes) {
                    eprintln!("advplay: restore failed: {err}");
                }
            }
            Err(err) => eprintln!("advplay: could not read {}: {err}", path.display()),
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last).as_secs_f64() * 1000.0;
        self.last = now;
        self.acc_ms = (self.acc_ms + dt).min(250.0);
        let mut steps = 0;
        while self.acc_ms >= 1000.0 / 60.0 && steps < 5 {
            self.engine().step(1);
            self.acc_ms -= 1000.0 / 60.0;
            steps += 1;
        }
        if steps > 0 {
            self.pump();
        }
    }

    fn draw(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        let frame = self.engine().framebuffer().to_vec();
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        if surface.resize(width, height).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        scale_nearest(&frame, &mut buffer, width.get(), height.get());
        let _ = buffer.present();
    }

    fn on_key(&mut self, logical: &Key) {
        let input = match logical {
            Key::Named(NamedKey::ArrowUp) => Some(Input::KeyDown { key: "n".into() }),
            Key::Named(NamedKey::ArrowDown) => Some(Input::KeyDown { key: "s".into() }),
            Key::Named(NamedKey::ArrowLeft) => Some(Input::KeyDown { key: "w".into() }),
            Key::Named(NamedKey::ArrowRight) => Some(Input::KeyDown { key: "e".into() }),
            Key::Named(NamedKey::Enter) => Some(Input::Submit),
            Key::Named(NamedKey::Backspace) => Some(Input::Backspace),
            Key::Named(NamedKey::Escape) => Some(Input::Dismiss),
            Key::Named(NamedKey::F5) => {
                self.write_save("1");
                None
            }
            Key::Named(NamedKey::F7) => {
                self.read_save("1");
                None
            }
            Key::Character(text) => Some(Input::Char {
                ch: text.to_string(),
            }),
            _ => None,
        };
        if let Some(input) = input {
            self.engine().input(input);
            self.pump();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(LogicalSize::new(1152.0, 648.0));
        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                eprintln!("advplay: {err}");
                event_loop.exit();
                return;
            }
        };
        let context = match softbuffer::Context::new(window.clone()) {
            Ok(context) => context,
            Err(err) => {
                eprintln!("advplay: {err}");
                event_loop.exit();
                return;
            }
        };
        let surface = match softbuffer::Surface::new(&context, window.clone()) {
            Ok(surface) => surface,
            Err(err) => {
                eprintln!("advplay: {err}");
                event_loop.exit();
                return;
            }
        };
        self.context = Some(context);
        self.surface = Some(surface);
        self.window = Some(window);
        self.pump();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                self.on_key(&event.logical_key);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            return;
        }
        self.tick();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn scale_nearest(src: &[u8], dst: &mut [u32], dst_w: u32, dst_h: u32) {
    const SRC_W: u32 = 384;
    const SRC_H: u32 = 216;
    let scale = (dst_w / SRC_W).min(dst_h / SRC_H).max(1);
    let view_w = SRC_W * scale;
    let view_h = SRC_H * scale;
    let off_x = dst_w.saturating_sub(view_w) / 2;
    let off_y = dst_h.saturating_sub(view_h) / 2;
    for y in 0..dst_h {
        for x in 0..dst_w {
            let color = if x >= off_x && y >= off_y && x < off_x + view_w && y < off_y + view_h {
                let sx = (x - off_x) / scale;
                let sy = (y - off_y) / scale;
                let i = ((sy * SRC_W + sx) * 4) as usize;
                let (r, g, b) = src
                    .get(i..i + 3)
                    .map(|px| (px[0] as u32, px[1] as u32, px[2] as u32))
                    .unwrap_or((0, 0, 0));
                (r << 16) | (g << 8) | b
            } else {
                0
            };
            dst[(y * dst_w + x) as usize] = color;
        }
    }
}
