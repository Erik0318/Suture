use eframe::egui::{self, Color32, RichText, Sense, Stroke, Ui, Vec2};

use crate::model::{duration_label, ProgressInfo};

pub fn show(ui: &mut Ui, progress: &ProgressInfo, reduced_motion: bool) {
    let fraction = progress.fraction.unwrap_or(0.0).clamp(0.0, 1.0);
    ui.horizontal(|ui| {
        let percent = progress
            .fraction
            .map(|value| format!("{:>3}%", (value * 100.0).round() as u32))
            .unwrap_or_else(|| "···".into());
        ui.label(RichText::new(percent).size(28.0).strong().monospace());
        ui.vertical(|ui| {
            ui.label(RichText::new(&progress.status).strong());
            if !progress.detail.is_empty() {
                ui.label(RichText::new(&progress.detail).small().weak());
            }
        });
    });

    let width = ui.available_width().max(120.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 38.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let y = rect.center().y;
    let left = rect.left() + 10.0;
    let right = rect.right() - 10.0;
    let accent = ui.visuals().selection.bg_fill;
    let inactive = ui.visuals().widgets.inactive.fg_stroke.color;
    painter.line_segment(
        [egui::pos2(left, y), egui::pos2(right, y)],
        Stroke::new(2.0, inactive.gamma_multiply(0.45)),
    );
    painter.line_segment(
        [egui::pos2(left, y), egui::pos2(left + (right - left) * fraction, y)],
        Stroke::new(3.0, accent),
    );

    let nodes = progress.track_count.clamp(1, 24);
    for slot in 0..nodes {
        let t = if nodes == 1 { 0.5 } else { slot as f32 / (nodes - 1) as f32 };
        let x = egui::lerp(left..=right, t);
        let represented_track = slot * progress.track_count.max(1) / nodes;
        let active = progress.active_track.unwrap_or(0);
        let completed = represented_track < active || t <= fraction;
        let is_active = represented_track == active;
        let pulse = if is_active && !reduced_motion {
            let time = ui.input(|input| input.time);
            1.5 + ((time * 4.0).sin() as f32 + 1.0)
        } else {
            1.5
        };
        if completed {
            painter.circle_filled(egui::pos2(x, y), 4.0 + pulse, accent);
        } else {
            painter.circle_stroke(egui::pos2(x, y), 5.0, Stroke::new(1.5, inactive));
        }
    }
    if progress.fraction.is_none() && !reduced_motion {
        let time = ui.input(|input| input.time) as f32;
        let t = (time * 0.35).fract();
        painter.circle_filled(egui::pos2(egui::lerp(left..=right, t), y), 4.5, accent);
    }

    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Elapsed {}", duration_label(progress.elapsed_secs)));
        if let Some(eta) = progress.eta_secs {
            ui.label(format!("• About {} remaining", duration_label(eta)));
        }
        if let Some(speed) = &progress.speed {
            ui.label(format!("• {speed}"));
        }
    });
    ui.add_space(2.0);
    let bar = egui::ProgressBar::new(fraction).animate(progress.fraction.is_none() && !reduced_motion);
    ui.add(bar);
}

#[allow(dead_code)]
fn _color_for_docs() -> Color32 {
    Color32::TRANSPARENT
}

