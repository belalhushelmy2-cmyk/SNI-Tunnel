//! Offline layout/scroll harness using the real Slint software renderer.
//! No compositor, engine, user store or network is involved.
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, LogicalSize, Rgb8Pixel, SharedPixelBuffer};
use sni_tunnel::gui::{AppWindow, PlotSegment, ServerEditor, ServerRow, TrafficPoint};
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
    ui.set_app_version(format!("v{}", env!("CARGO_PKG_VERSION")).into());
    ui.on_matches(|text, query| text.to_lowercase().contains(&query.to_lowercase()));
    ui.set_servers(slint::ModelRc::new(slint::VecModel::from(
        (0..5)
            .map(|i| ServerRow {
                name: format!("Profile {} — offline preview", i + 1).into(),
                addr: format!("server-{}.example", i + 1).into(),
                port: "443".into(),
                transport: "VMess · WS · TLS".into(),
                sni: "sni.example".into(),
                status: "Not tested".into(),
                latency: "—".into(),
                selected: false,
                source_index: i,
            })
            .collect::<Vec<_>>(),
    )));
    ui.set_server_count(5);
    ui.show()?;
    let main = windows.borrow().last().unwrap().clone();
    main.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    main.set_size(LogicalSize::new(1366., 768.));
    save(&main, &directory, "premium-overview-1366");
    if std::env::args().any(|arg| arg == "--render-only") {
        for (width, height) in [
            (1366., 768.),
            (1536., 1024.),
            (1920., 1080.),
            (980., 720.),
            (800., 600.),
        ] {
            main.set_size(LogicalSize::new(width, height));
            for tab in 0..6 {
                ui.set_tab(tab);
                save(
                    &main,
                    &directory,
                    &format!("premium-{}x{}-page-{tab}", width as i32, height as i32),
                );
            }
        }
        println!("Rendered six native pages at five window sizes, scale {scale}");
        return Ok(());
    }
    let click = |x, y| {
        let position = LogicalPosition::new(x, y);
        main.dispatch_event(WindowEvent::PointerMoved { position });
        main.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        main.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    };
    // Numeric changes must not resize health or metric cards.
    let before_values = save(&main, &directory, "cards-before-values");
    ui.set_down_speed("1024.99 MB/s".into());
    ui.set_up_speed("1 B/s".into());
    ui.set_uptime("127:59:59".into());
    ui.set_down_total("999.99 TB".into());
    ui.set_latency("DNS + TCP latency unavailable · retrying / 3 s".into());
    ui.set_health_tun("Verified".into());
    ui.set_health_dns("Checking".into());
    ui.set_health_ipv4("Proxy only".into());
    ui.set_health_ipv6("Protected".into());
    let after_values = save(&main, &directory, "cards-after-values");
    let pixel = |data: &[u8], x: usize, y: usize| {
        let x = (x as f32 * scale) as usize;
        let y = (y as f32 * scale) as usize;
        let offset = (y * main.size().width as usize + x) * 3;
        data[offset..offset + 3].to_vec()
    };
    for y in [366, 445] {
        for x in 214..1322 {
            assert_eq!(
                pixel(&before_values, x, y),
                pixel(&after_values, x, y),
                "card borders must stay fixed when numeric/status text changes at {x},{y}"
            );
        }
    }
    ui.set_down_speed("0 B/s".into());
    ui.set_up_speed("0 B/s".into());
    ui.set_uptime("00:00".into());
    ui.set_down_total("0 B".into());
    ui.set_latency("Not measured".into());
    for setter in [
        AppWindow::set_health_tun,
        AppWindow::set_health_dns,
        AppWindow::set_health_ipv4,
        AppWindow::set_health_ipv6,
    ] {
        setter(&ui, "Not verified".into());
    }
    // Explicit offline renderer fixture, never used by the application backend.
    ui.set_traffic(slint::ModelRc::new(slint::VecModel::from(
        (0..60)
            .map(|index| TrafficPoint {
                up: ((index % 10) * 5) as f32,
                down: ((index % 7) * 20) as f32,
            })
            .collect::<Vec<_>>(),
    )));
    ui.set_traffic_peak(120.0);
    ui.set_latency_values(slint::ModelRc::new(slint::VecModel::from(vec![
        20.0, 30.0, -1.0, 10.0, 50.0,
    ])));
    ui.set_latency_peak(50.0);
    ui.set_latency_good_count(4);
    ui.set_latency_line(slint::ModelRc::new(slint::VecModel::from(vec![
        PlotSegment {
            x1: 0.0,
            y1: 72.2,
            x2: 150.0,
            y2: 48.8,
        },
        PlotSegment {
            x1: 450.0,
            y1: 95.6,
            x2: 600.0,
            y2: 2.0,
        },
    ])));
    let bars = save(&main, &directory, "offline-chart-fixture");
    let count_color = |data: &[u8], color: [u8; 3]| {
        (610..688)
            .flat_map(|y| (300..1310).map(move |x| (x, y)))
            .filter(|&(x, y)| pixel(data, x, y) == color)
            .count()
    };
    assert!(
        count_color(&bars, [121, 206, 197]) > 100,
        "download bars render from samples"
    );
    assert!(
        count_color(&bars, [130, 169, 220]) > 100,
        "upload bars render from samples"
    );
    ui.set_download_visible(false);
    let hidden = save(&main, &directory, "offline-download-hidden");
    assert_eq!(
        count_color(&hidden, [121, 206, 197]),
        0,
        "download toggle hides bars"
    );
    click(1052., 573.);
    let latency = save(&main, &directory, "offline-latency-fixture");
    assert!(
        count_color(&latency, [121, 213, 183]) > 20,
        "Latency tab renders the measured trace"
    );
    ui.set_download_visible(true);
    ui.set_traffic(Default::default());
    ui.set_traffic_peak(1.0);
    ui.set_latency_values(Default::default());
    ui.set_latency_line(Default::default());
    ui.set_latency_good_count(0);
    // Exercise the actual overview hit targets without starting a network engine.
    let action = Rc::new(std::cell::Cell::new(0));
    let state = action.clone();
    ui.on_connect_clicked(move || state.set(1));
    let state = action.clone();
    ui.on_disconnect_clicked(move || state.set(2));
    let state = action.clone();
    ui.on_verify_full_tunnel_clicked(move || state.set(3));
    let state = action.clone();
    ui.on_refresh_ip_clicked(move || state.set(4));
    click(1230., 142.);
    assert_eq!(action.get(), 1, "primary Connect reaches backend callback");
    ui.set_connection_live(true);
    click(1230., 142.);
    assert_eq!(
        action.get(),
        2,
        "primary Disconnect reaches backend callback"
    );
    ui.set_connection_live(false);
    ui.set_connection_busy(true);
    click(1230., 142.);
    assert_eq!(action.get(), 2, "Cancel reaches disconnect callback");
    ui.set_connection_busy(false);
    click(980., 337.);
    assert_eq!(
        action.get(),
        3,
        "Verify reaches existing full-check callback"
    );
    click(1174., 268.);
    assert_eq!(
        action.get(),
        4,
        "public IP refresh reaches existing callback"
    );
    click(1260., 268.);
    assert_eq!(ui.get_tab(), 1, "Switch server opens server selection");
    ui.set_tab(0);
    main.set_size(LogicalSize::new(980., 900.));
    // A complete first click must activate each item, including icon/padding hits.
    for (index, x) in [(1, 25.), (2, 80.), (3, 151.), (4, 25.), (5, 80.), (0, 151.)] {
        let position = LogicalPosition::new(x, 104. + index as f32 * 48.);
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
    main.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Control.into(),
    });
    main.dispatch_event(WindowEvent::KeyPressed { text: "k".into() });
    main.dispatch_event(WindowEvent::KeyReleased { text: "k".into() });
    main.dispatch_event(WindowEvent::KeyReleased {
        text: slint::platform::Key::Control.into(),
    });
    assert!(
        ui.get_search_focused(),
        "Ctrl+K focuses native server search"
    );
    for index in 0..6 {
        ui.set_tab(index);
        save(&main, &directory, &format!("page-{index}"));
    }
    ui.set_tab(1);
    save(&main, &directory, "five-servers");
    let used = Rc::new(std::cell::Cell::new(-1));
    let used_callback = used.clone();
    ui.on_use_server(move |source| used_callback.set(source));
    for index in 0..5 {
        used.set(-1);
        // Fontconfig may resolve sans-serif to a wider user-selected font.
        // Search the action area while still requiring the correct row callback.
        for x in (540..585).step_by(8) {
            let position = LogicalPosition::new(x as f32, 271. + index as f32 * 88.);
            main.dispatch_event(WindowEvent::PointerMoved { position });
            main.dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
            main.dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
            if used.get() >= 0 {
                break;
            }
        }
        assert_eq!(
            used.get(),
            index,
            "all five server rows must have separate, correct Use buttons"
        );
    }
    ui.set_compact_layout(true);
    main.set_size(LogicalSize::new(980., 720.));
    save(&main, &directory, "five-servers-compact");
    let mut compact_rows = std::collections::BTreeSet::new();
    for y in (240..700).step_by(10) {
        used.set(-1);
        for x in (540..585).step_by(8) {
            let position = LogicalPosition::new(x as f32, y as f32);
            main.dispatch_event(WindowEvent::PointerMoved { position });
            main.dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
            main.dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
            if used.get() >= 0 {
                break;
            }
        }
        if used.get() >= 0 {
            compact_rows.insert(used.get());
        }
    }
    assert_eq!(
        compact_rows,
        (0..5).collect(),
        "compact view must show five independently clickable profiles"
    );
    ui.set_compact_layout(false);
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
    println!("PASS: five separate server rows in normal and compact views; six first-click menu actions; Space/Enter; six pages; 800x600 dashboard scroll; 660x420 editor scroll and fixed footer; scale {scale}");
    Ok(())
}
