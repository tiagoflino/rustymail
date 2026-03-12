use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

const TRAY_ID: &str = "rustymail-tray";

pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItemBuilder::with_id("show", "Show Rustymail").build(app)?;
    let compose = MenuItemBuilder::with_id("compose", "Compose").build(app)?;
    let check_mail = MenuItemBuilder::with_id("check_mail", "Check Mail").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Rustymail").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&compose)
        .item(&check_mail)
        .separator()
        .item(&quit)
        .build()?;

    let icon = generate_tray_icon();

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Rustymail")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app: &AppHandle, event| {
            handle_menu_event(app, event.id().as_ref());
        })
        .build(app)?;

    Ok(())
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "show" => show_window(app),
        "compose" => {
            show_window(app);
            let app_clone = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let _ = app_clone.emit("tray-compose", ());
            });
        }
        "check_mail" => {
            show_window(app);
            let app_clone = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let _ = app_clone.emit("tray-check-mail", ());
            });
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub fn update_tray_unread(app_handle: AppHandle, count: u32) {
    let tooltip = if count == 0 {
        "Rustymail".to_string()
    } else {
        format!("Rustymail — {} unread", count)
    };

    if let Some(tray) = app_handle.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(&tooltip));
    }

    if let Some(window) = app_handle.get_webview_window("main") {
        // macOS: set dock badge label (text)
        #[cfg(target_os = "macos")]
        {
            let label = if count > 0 { Some(count.to_string()) } else { None };
            match window.set_badge_label(label.clone()) {
                Ok(_) => println!("[Tray] Badge set to: {:?}", label),
                Err(e) => eprintln!("[Tray] Failed to set badge: {}", e),
            }
        }

        // Linux: launcher badge count via D-Bus
        #[cfg(target_os = "linux")]
        {
            let badge = if count > 0 { Some(count as i64) } else { None };
            let _ = window.set_badge_count(badge);
        }

        // Windows: overlay icon on taskbar
        #[cfg(target_os = "windows")]
        {
            if count > 0 {
                if let Some(icon) = generate_badge_icon(count) {
                    let _ = window.set_overlay_icon(Some(icon));
                }
            } else {
                let _ = window.set_overlay_icon(None::<tauri::image::Image<'_>>);
            }
        }

        let _ = window; // suppress unused warning on non-matching platforms
    }
}

/// Generate a 44x44 colored whiskey glass icon (flat design) for the system tray.
/// Transparent background with colored glass, amber whiskey, and ice cubes.
fn generate_tray_icon() -> tauri::image::Image<'static> {
    let size: u32 = 44;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    // Colors (flat design style)
    let glass_blue: [u8; 4] = [108, 180, 238, 255];  // light blue glass
    let whiskey: [u8; 4] = [232, 146, 74, 255];       // amber whiskey
    let ice: [u8; 4] = [245, 230, 200, 255];          // cream/white ice
    let ice_shadow: [u8; 4] = [220, 200, 170, 255];   // ice shadow

    let set_pixel = |rgba: &mut Vec<u8>, x: i32, y: i32, color: [u8; 4]| {
        if x >= 0 && y >= 0 && (x as u32) < size && (y as u32) < size {
            let idx = ((y as u32 * size + x as u32) * 4) as usize;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    };

    // Glass shape: tapered tumbler
    let top_y: i32 = 5;
    let bot_y: i32 = 38;
    let top_left: i32 = 6;
    let top_right: i32 = 38;
    let bot_left: i32 = 12;
    let bot_right: i32 = 32;

    let left_at = |y: i32| -> i32 {
        top_left + (bot_left - top_left) * (y - top_y) / (bot_y - top_y)
    };
    let right_at = |y: i32| -> i32 {
        top_right + (bot_right - top_right) * (y - top_y) / (bot_y - top_y)
    };

    // Whiskey level at ~55% from top
    let liquid_y = top_y + (bot_y - top_y) * 50 / 100;

    // Fill entire glass interior
    for y in (top_y + 1)..bot_y {
        let lx = left_at(y) + 1;
        let rx = right_at(y) - 1;
        let color = if y >= liquid_y { whiskey } else { glass_blue };
        for x in lx..=rx {
            set_pixel(&mut rgba, x, y, color);
        }
    }

    // Ice cube 1 (larger, left-center) — sits at liquid line
    let ice1_x = 15;
    let ice1_y = liquid_y - 2;
    for dy in 0..7 {
        for dx in 0..8 {
            let color = if dx < 4 && dy < 3 { ice } else { ice_shadow };
            set_pixel(&mut rgba, ice1_x + dx, ice1_y + dy, color);
        }
    }

    // Ice cube 2 (smaller, right) — tilted, overlapping liquid
    let ice2_x = 24;
    let ice2_y = liquid_y - 1;
    for dy in 0..6 {
        for dx in 0..7 {
            let color = if dx < 3 && dy < 3 { ice } else { ice_shadow };
            set_pixel(&mut rgba, ice2_x + dx, ice2_y + dy, color);
        }
    }

    tauri::image::Image::new_owned(rgba, size, size)
}

/// Generate a simple badge overlay icon for Windows taskbar.
#[cfg(target_os = "windows")]
fn generate_badge_icon(count: u32) -> Option<tauri::image::Image<'static>> {
    let size: u32 = 16;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let cx = size as f32 / 2.0;
    let cy = size as f32 / 2.0;
    let radius = 7.0f32;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * size + x) * 4) as usize;
            if dist <= radius {
                rgba[idx] = 255;
                rgba[idx + 1] = 59;
                rgba[idx + 2] = 48;
                rgba[idx + 3] = 255;
            }
        }
    }

    let text = if count <= 9 { count.to_string() } else { "9+".to_string() };
    draw_text_on_badge(&mut rgba, size, &text);

    Some(tauri::image::Image::new_owned(rgba, size, size))
}

#[cfg(target_os = "windows")]
fn draw_text_on_badge(rgba: &mut [u8], size: u32, text: &str) {
    let white = [255u8, 255, 255, 255];

    fn set_pixel(rgba: &mut [u8], size: u32, x: u32, y: u32, color: [u8; 4]) {
        if x < size && y < size {
            let idx = ((y * size + x) * 4) as usize;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }

    if text.len() == 1 {
        let ox = 6u32;
        let oy = 4u32;
        let digit = text.chars().next().unwrap();
        let pattern: &[(u32, u32)] = match digit {
            '1' => &[(2,0),(1,1),(2,1),(2,2),(2,3),(2,4),(2,5),(2,6),(1,7),(2,7),(3,7)],
            '2' => &[(1,0),(2,0),(3,0),(0,1),(3,1),(3,2),(2,3),(1,4),(0,5),(0,6),(0,7),(1,7),(2,7),(3,7)],
            '3' => &[(0,0),(1,0),(2,0),(3,1),(2,2),(3,3),(3,4),(3,5),(0,6),(2,6),(1,6),(0,7),(1,7),(2,7)],
            '4' => &[(3,0),(2,1),(3,1),(1,2),(3,2),(0,3),(3,3),(0,4),(1,4),(2,4),(3,4),(3,5),(3,6),(3,7)],
            '5' => &[(0,0),(1,0),(2,0),(3,0),(0,1),(0,2),(1,2),(2,2),(3,3),(3,4),(3,5),(0,6),(2,6),(1,6),(0,7),(1,7),(2,7)],
            '6' => &[(1,0),(2,0),(0,1),(0,2),(1,2),(2,2),(0,3),(3,3),(0,4),(3,4),(0,5),(3,5),(1,6),(2,6)],
            '7' => &[(0,0),(1,0),(2,0),(3,0),(3,1),(2,2),(2,3),(1,4),(1,5),(1,6),(1,7)],
            '8' => &[(1,0),(2,0),(0,1),(3,1),(1,3),(2,3),(0,4),(3,4),(0,5),(3,5),(1,6),(2,6)],
            '9' => &[(1,0),(2,0),(0,1),(3,1),(0,2),(3,2),(1,3),(2,3),(3,3),(3,4),(3,5),(1,6),(2,6)],
            _ => &[(1,0),(2,0),(0,1),(3,1),(0,2),(3,2),(0,3),(3,3),(0,4),(3,4),(0,5),(3,5),(1,6),(2,6)],
        };
        for &(px, py) in pattern {
            set_pixel(rgba, size, ox + px, oy + py, white);
        }
    } else {
        let ox = 4u32;
        let oy = 5u32;
        let nine = [(1,0),(2,0),(0,1),(2,1),(1,2),(2,2),(2,3),(1,4)];
        for &(px, py) in &nine {
            set_pixel(rgba, size, ox + px, oy + py, white);
        }
        let plus = [(5,1),(4,2),(5,2),(6,2),(5,3)];
        for &(px, py) in &plus {
            set_pixel(rgba, size, ox + px, oy + py, white);
        }
    }
}
