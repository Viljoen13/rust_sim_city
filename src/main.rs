//! SimCity-style city builder on a 64x64 grid: roads, RCI zoning, a demand
//! feedback loop, and tax income. See README.md for controls.

mod sim;

use macroquad::prelude::*;
use sim::{City, Tile, Tool, ZoneKind, GRID_H, GRID_W, MAX_LEVEL};

const HUD_H: f32 = 64.0;
const TICK_SECONDS: f32 = 0.5;

const GRASS: Color = Color::new(0.23, 0.42, 0.23, 1.0);
const DIRT: Color = Color::new(0.45, 0.37, 0.26, 1.0);
const ROAD: Color = Color::new(0.30, 0.30, 0.33, 1.0);
const ROAD_LANE: Color = Color::new(0.45, 0.45, 0.48, 1.0);

fn zone_color(kind: ZoneKind) -> Color {
    match kind {
        ZoneKind::Residential => Color::new(0.30, 0.78, 0.35, 1.0),
        ZoneKind::Commercial => Color::new(0.35, 0.55, 0.95, 1.0),
        ZoneKind::Industrial => Color::new(0.92, 0.80, 0.28, 1.0),
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        1.0,
    )
}

struct CameraState {
    /// World position (tile units) at the center of the screen.
    target: Vec2,
    /// Pixels per tile.
    scale: f32,
}

impl CameraState {
    fn world_from_screen(&self, sx: f32, sy: f32) -> Vec2 {
        vec2(
            self.target.x + (sx - screen_width() / 2.0) / self.scale,
            self.target.y + (sy - screen_height() / 2.0) / self.scale,
        )
    }

    fn macroquad_camera(&self) -> Camera2D {
        Camera2D {
            target: self.target,
            // Positive y zoom makes world y point down (matching the grid):
            // verified empirically against macroquad 0.4's NDC convention.
            zoom: vec2(
                2.0 * self.scale / screen_width(),
                2.0 * self.scale / screen_height(),
            ),
            ..Default::default()
        }
    }
}

struct Button {
    rect: Rect,
    label: &'static str,
    tool: Tool,
}

fn tool_buttons() -> Vec<Button> {
    let defs: [(&'static str, Tool); 5] = [
        ("[1] Road", Tool::Road),
        ("[2] Bulldoze", Tool::Bulldoze),
        ("[3] Res", Tool::Zone(ZoneKind::Residential)),
        ("[4] Com", Tool::Zone(ZoneKind::Commercial)),
        ("[5] Ind", Tool::Zone(ZoneKind::Industrial)),
    ];
    defs.iter()
        .enumerate()
        .map(|(i, (label, tool))| Button {
            rect: Rect::new(8.0 + i as f32 * 100.0, 8.0, 94.0, 22.0),
            label,
            tool: *tool,
        })
        .collect()
}

fn format_money(v: f64) -> String {
    let negative = v < 0.0;
    let mut n = v.abs() as i64;
    let mut parts = Vec::new();
    loop {
        if n < 1000 {
            parts.push(n.to_string());
            break;
        }
        parts.push(format!("{:03}", n % 1000));
        n /= 1000;
    }
    parts.reverse();
    format!("{}${}", if negative { "-" } else { "" }, parts.join(","))
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Rust Sim City".to_owned(),
        window_width: 1280,
        window_height: 800,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    macroquad::rand::srand(macroquad::miniquad::date::now() as u64);

    let mut city = City::new();
    let mut camera = CameraState {
        target: vec2(GRID_W as f32 / 2.0, GRID_H as f32 / 2.0),
        scale: 14.0,
    };
    let mut tool = Tool::Road;
    let mut paused = false;
    let mut tick_accum = 0.0f32;
    let mut last_paint: Option<(i32, i32)> = None;
    let mut message = String::new();
    let mut message_timer = 0.0f32;

    loop {
        let dt = get_frame_time();

        // ---- Input: tools ----
        if is_key_pressed(KeyCode::Key1) {
            tool = Tool::Road;
        }
        if is_key_pressed(KeyCode::Key2) {
            tool = Tool::Bulldoze;
        }
        if is_key_pressed(KeyCode::Key3) {
            tool = Tool::Zone(ZoneKind::Residential);
        }
        if is_key_pressed(KeyCode::Key4) {
            tool = Tool::Zone(ZoneKind::Commercial);
        }
        if is_key_pressed(KeyCode::Key5) {
            tool = Tool::Zone(ZoneKind::Industrial);
        }
        if is_key_pressed(KeyCode::Space) {
            paused = !paused;
        }

        // ---- Input: camera pan / zoom ----
        let pan = 600.0 * dt / camera.scale; // constant on-screen speed
        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
            camera.target.y -= pan;
        }
        if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
            camera.target.y += pan;
        }
        if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
            camera.target.x -= pan;
        }
        if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
            camera.target.x += pan;
        }

        let (mx, my) = mouse_position();
        let wheel = mouse_wheel().1;
        if wheel != 0.0 {
            let before = camera.world_from_screen(mx, my);
            camera.scale = (camera.scale * 1.15f32.powf(wheel.signum())).clamp(3.0, 64.0);
            let after = camera.world_from_screen(mx, my);
            camera.target += before - after; // keep cursor anchored while zooming
        }
        camera.target.x = camera.target.x.clamp(0.0, GRID_W as f32);
        camera.target.y = camera.target.y.clamp(0.0, GRID_H as f32);

        // ---- Input: HUD buttons / painting ----
        let buttons = tool_buttons();
        let mouse_in_hud = my < HUD_H;
        if is_mouse_button_pressed(MouseButton::Left) && mouse_in_hud {
            for b in &buttons {
                if b.rect.contains(vec2(mx, my)) {
                    tool = b.tool;
                }
            }
        }

        let hover_world = camera.world_from_screen(mx, my);
        let hover_tile = (hover_world.x.floor() as i32, hover_world.y.floor() as i32);

        if is_mouse_button_down(MouseButton::Left) && !mouse_in_hud {
            let (cx, cy) = hover_tile;
            // Interpolate from the previous painted tile so fast drags
            // leave no gaps.
            let (px, py) = last_paint.unwrap_or((cx, cy));
            let steps = (cx - px).abs().max((cy - py).abs()).max(1);
            for s in 1..=steps {
                let t = s as f32 / steps as f32;
                let x = (px as f32 + (cx - px) as f32 * t).round() as i32;
                let y = (py as f32 + (cy - py) as f32 * t).round() as i32;
                if let Err(why) = city.apply_tool(tool, x, y) {
                    let first_click = is_mouse_button_pressed(MouseButton::Left);
                    if first_click || why == "not enough funds" {
                        message = format!("Can't build: {why}");
                        message_timer = 1.6;
                    }
                }
            }
            last_paint = Some((cx, cy));
        } else {
            last_paint = None;
        }

        // ---- Simulation ----
        if !paused {
            tick_accum = (tick_accum + dt).min(TICK_SECONDS * 5.0);
            while tick_accum >= TICK_SECONDS {
                city.tick();
                tick_accum -= TICK_SECONDS;
            }
        }
        message_timer = (message_timer - dt).max(0.0);

        // ---- Render world ----
        clear_background(Color::new(0.10, 0.12, 0.10, 1.0));
        set_camera(&camera.macroquad_camera());

        // Visible tile range (cull off-screen tiles).
        let tl = camera.world_from_screen(0.0, 0.0);
        let br = camera.world_from_screen(screen_width(), screen_height());
        let x0 = (tl.x.floor() as i32).clamp(0, GRID_W - 1);
        let y0 = (tl.y.floor() as i32).clamp(0, GRID_H - 1);
        let x1 = (br.x.ceil() as i32).clamp(0, GRID_W - 1);
        let y1 = (br.y.ceil() as i32).clamp(0, GRID_H - 1);

        for y in y0..=y1 {
            for x in x0..=x1 {
                draw_tile(&city, x, y);
            }
        }

        // Grid lines when zoomed in enough to be useful.
        if camera.scale > 7.0 {
            let line = Color::new(0.0, 0.0, 0.0, 0.12);
            for x in x0..=(x1 + 1) {
                draw_line(x as f32, y0 as f32, x as f32, (y1 + 1) as f32, 0.04, line);
            }
            for y in y0..=(y1 + 1) {
                draw_line(x0 as f32, y as f32, (x1 + 1) as f32, y as f32, 0.04, line);
            }
        }

        // Hover highlight with a ghost of the selected tool.
        if !mouse_in_hud && City::in_bounds(hover_tile.0, hover_tile.1) {
            let (hx, hy) = (hover_tile.0 as f32, hover_tile.1 as f32);
            let ghost = match tool {
                Tool::Road => ROAD,
                Tool::Bulldoze => Color::new(0.9, 0.3, 0.3, 1.0),
                Tool::Zone(kind) => zone_color(kind),
            };
            draw_rectangle(hx, hy, 1.0, 1.0, Color::new(ghost.r, ghost.g, ghost.b, 0.40));
            draw_rectangle_lines(hx, hy, 1.0, 1.0, 0.12, WHITE);
        }

        // ---- Render HUD ----
        set_default_camera();
        draw_hud(&city, &buttons, tool, paused);

        if message_timer > 0.0 {
            let dims = measure_text(&message, None, 24, 1.0);
            let x = (screen_width() - dims.width) / 2.0;
            let y = screen_height() - 30.0;
            draw_rectangle(
                x - 10.0,
                y - dims.height - 6.0,
                dims.width + 20.0,
                dims.height + 16.0,
                Color::new(0.0, 0.0, 0.0, 0.6),
            );
            draw_text(&message, x, y, 24.0, Color::new(1.0, 0.6, 0.5, 1.0));
        }

        next_frame().await;
    }
}

fn draw_tile(city: &City, x: i32, y: i32) {
    let (fx, fy) = (x as f32, y as f32);
    match city.tile(x, y) {
        Tile::Grass => {
            // Hash-based shade variation so the grass isn't a flat plane.
            let v = ((x * 7 + y * 13) % 5) as f32 * 0.012;
            draw_rectangle(fx, fy, 1.0, 1.0, mix(GRASS, WHITE, v));
        }
        Tile::Road => {
            draw_rectangle(fx, fy, 1.0, 1.0, ROAD);
            // Lighter lanes reaching toward neighboring roads.
            let road_at =
                |nx: i32, ny: i32| City::in_bounds(nx, ny) && city.tile(nx, ny) == Tile::Road;
            draw_rectangle(fx + 0.35, fy + 0.35, 0.30, 0.30, ROAD_LANE);
            if road_at(x - 1, y) {
                draw_rectangle(fx, fy + 0.35, 0.35, 0.30, ROAD_LANE);
            }
            if road_at(x + 1, y) {
                draw_rectangle(fx + 0.65, fy + 0.35, 0.35, 0.30, ROAD_LANE);
            }
            if road_at(x, y - 1) {
                draw_rectangle(fx + 0.35, fy, 0.30, 0.35, ROAD_LANE);
            }
            if road_at(x, y + 1) {
                draw_rectangle(fx + 0.35, fy + 0.65, 0.30, 0.35, ROAD_LANE);
            }
        }
        Tile::Zone { kind, level } => {
            let color = zone_color(kind);
            // Undeveloped: faintly tinted dirt with a zoning border.
            draw_rectangle(fx, fy, 1.0, 1.0, mix(DIRT, color, 0.22));
            draw_rectangle_lines(fx + 0.02, fy + 0.02, 0.96, 0.96, 0.07, mix(color, DIRT, 0.35));
            if level == 0 {
                return;
            }
            // One block per level; brighter as the tile develops.
            const BLOCKS: [(f32, f32, f32, f32); MAX_LEVEL as usize] = [
                (0.10, 0.10, 0.35, 0.35),
                (0.55, 0.10, 0.35, 0.35),
                (0.10, 0.55, 0.35, 0.35),
                (0.55, 0.55, 0.35, 0.35),
                (0.28, 0.28, 0.44, 0.44),
            ];
            let bright = mix(color, WHITE, 0.06 * level as f32);
            for &(bx, by, bw, bh) in BLOCKS.iter().take(level as usize) {
                let dark = mix(bright, BLACK, 0.45);
                draw_rectangle(fx + bx, fy + by, bw, bh, dark);
                // Inset "roof" gives a hint of height.
                draw_rectangle(
                    fx + bx + 0.05,
                    fy + by + 0.05,
                    bw - 0.10,
                    bh - 0.10,
                    bright,
                );
            }
        }
    }
}

fn draw_hud(city: &City, buttons: &[Button], tool: Tool, paused: bool) {
    draw_rectangle(0.0, 0.0, screen_width(), HUD_H, Color::new(0.08, 0.08, 0.10, 0.95));

    for b in buttons {
        let selected = b.tool == tool;
        let bg = if selected {
            Color::new(0.35, 0.40, 0.55, 1.0)
        } else {
            Color::new(0.18, 0.18, 0.22, 1.0)
        };
        draw_rectangle(b.rect.x, b.rect.y, b.rect.w, b.rect.h, bg);
        draw_rectangle_lines(
            b.rect.x,
            b.rect.y,
            b.rect.w,
            b.rect.h,
            1.5,
            if selected { WHITE } else { GRAY },
        );
        draw_text(b.label, b.rect.x + 6.0, b.rect.y + 16.0, 17.0, WHITE);
    }

    let stats = format!(
        "Funds: {}   Pop: {}   Jobs: {}{}",
        format_money(city.funds),
        city.population,
        city.jobs,
        if paused { "   [PAUSED - Space]" } else { "" }
    );
    draw_text(&stats, 10.0, 52.0, 22.0, WHITE);

    let costs = "Road $10  Zone $5  Bulldoze $1";
    draw_text(costs, 520.0, 24.0, 17.0, LIGHTGRAY);

    // RCI demand meter, top-right.
    let meter_x = screen_width() - 130.0;
    let mid_y = HUD_H / 2.0;
    let half = 24.0;
    draw_rectangle(
        meter_x - 14.0,
        4.0,
        130.0,
        HUD_H - 8.0,
        Color::new(0.05, 0.05, 0.07, 1.0),
    );
    draw_line(meter_x - 8.0, mid_y, meter_x + 100.0, mid_y, 1.0, GRAY);
    let bars = [
        ("R", city.demand_r, zone_color(ZoneKind::Residential)),
        ("C", city.demand_c, zone_color(ZoneKind::Commercial)),
        ("I", city.demand_i, zone_color(ZoneKind::Industrial)),
    ];
    for (i, (label, demand, color)) in bars.iter().enumerate() {
        let bx = meter_x + i as f32 * 32.0;
        let h = demand.clamp(-1.0, 1.0) * half;
        if h >= 0.0 {
            draw_rectangle(bx, mid_y - h, 16.0, h, *color);
        } else {
            draw_rectangle(bx, mid_y, 16.0, -h, mix(*color, BLACK, 0.4));
        }
        draw_text(label, bx + 4.0, mid_y + 4.0, 14.0, WHITE);
    }
}
