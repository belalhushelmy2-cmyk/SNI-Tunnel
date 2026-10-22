//! Offline layout/scroll harness using the real Slint software renderer.
//! No compositor, engine, user store or network is involved.
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, LogicalSize, Rgb8Pixel, SharedPixelBuffer};
use sni_tunnel::gui::{AppWindow, ServerEditor, ServerRow};
use std::{cell::RefCell, fs, path::Path, rc::Rc};

struct PreviewPlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
impl Platform for PreviewPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}
fn save(window: &MinimalSoftwareWindow, directory: &Path, name: &str) -> Vec<u8> {
    slint::platform::update_timers_and_animations();
    let size = window.size();
    let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(size.width, size.height);
    window.request_redraw();
    assert!(window.draw_if_needed(|renderer| {
        renderer.render(pixels.make_mut_slice(), size.width as usize);
    }));
    let mut ppm = format!("P6\n{} {}\n255\n", size.width, size.height).into_bytes();
    ppm.extend_from_slice(pixels.as_bytes());
    fs::write(directory.join(format!("{name}.ppm")), ppm).unwrap();
    pixels.as_bytes().to_vec()
}
fn scroll(window: &MinimalSoftwareWindow, x: f32, y: f32) {
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(x, y),
        delta_x: 0.,
        delta_y: -5000.,
    });
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scale: f32 = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "1".into())
        .parse()?;
    let directory = std::path::PathBuf::from(format!("work/preview-{}", (scale * 100.) as u32));
    fs::create_dir_all(&directory)?;
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(PreviewPlatform(windows.clone())))?;
    let ui = AppWindow::new()?;
    ui.on_matches(|text, query| text.to_lowercase().contains(&query.to_lowercase()));
    ui.set_servers(slint::ModelRc::new(slint::VecModel::from(vec![
        ServerRow {
            name: "Example profile — offline preview".into(),
            addr: "server.example".into(),
            port: "443".into(),
            transport: "VMess · WS · TLS".into(),
            sni: "sni.example".into(),
            status: "Not tested".into(),
            latency: "—".into(),
            selected: true,
            source_index: 0,
        },
    ])));
    ui.show()?;
    let main = windows.borrow().last().unwrap().clone();
    main.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    main.set_size(LogicalSize::new(980., 720.));
    // A complete first click must activate each item, including icon/padding hits.
    for (index, x) in [(1, 25.), (2, 80.), (3, 151.), (4, 25.), (5, 80.), (0, 151.)] {
        let position = LogicalPosition::new(x, 114. + index as f32 * 50.);
        main.dispatch_event(WindowEvent::PointerMoved { position });
        main.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        main.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        assert_eq!(
            ui.get_tab(),
            index,
            "first click must activate sidebar item {index}"
        );
    }
    // Mouse activation retains keyboard access to the focused item.
    for text in [" ".into(), slint::platform::Key::Return.into()] {
        ui.set_tab(5);
        main.dispatch_event(WindowEvent::KeyPressed { text });
        assert_eq!(
            ui.get_tab(),
            0,
            "Space/Enter must activate the focused menu item"
        );
    }
    for index in 0..6 {
        ui.set_tab(index);
        save(&main, &directory, &format!("page-{index}"));
    }
    ui.set_tab(0);
    main.set_size(LogicalSize::new(800., 600.));
    let before = save(&main, &directory, "dashboard-small");
    scroll(&main, 500., 400.);
    let after = save(&main, &directory, "dashboard-scrolled");
    assert_ne!(before, after, "dashboard must respond to wheel scrolling");
    let editor = ServerEditor::new()?;
    editor.show()?;
    let edit = windows.borrow().last().unwrap().clone();
    edit.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    edit.set_size(LogicalSize::new(660., 420.));
    let before = save(&edit, &directory, "editor-small");
    scroll(&edit, 300., 200.);
    let after = save(&edit, &directory, "editor-scrolled");
    assert_ne!(before, after, "editor must respond to wheel scrolling");
    let footer = (edit.size().width as usize) * (45. * scale) as usize * 3;
    assert_eq!(
        &before[before.len() - footer..],
        &after[after.len() - footer..],
        "editor footer must remain fixed while scrolling"
    );
    println!("PASS: six first-click menu actions; Space/Enter; six pages; 800x600 dashboard scroll; 660x420 editor scroll and fixed footer; scale {scale}");
    Ok(())
}
