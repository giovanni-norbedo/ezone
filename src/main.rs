// Ezone - A 
// Copyright (C) 2026 Giovanni Norbedo <norbedo@proton.me>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.


use eframe::egui;
use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::Accessor;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Clone)]
struct Theme {
    bg: [u8; 3],
    text_primary: [u8; 3],
    text_secondary: [u8; 3],
    accent: [u8; 3],
    record_base: [u8; 3],
    record_vinyl: [u8; 3],
    last_crate_path: Option<String>,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: [0, 0, 0],
            text_primary: [255, 255, 255],
            text_secondary: [128, 128, 128],
            accent: [196, 196, 196],
            record_base: [16, 16, 16],
            record_vinyl: [8, 8, 8],
            last_crate_path: None,
        }
    }
}

impl Theme {
    fn config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("ezone");
        let _ = std::fs::create_dir_all(&path);
        path.push("theme.json");
        path
    }

    fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => {
                let default_theme = Theme::default();
                if let Ok(json) = serde_json::to_string_pretty(&default_theme) {
                    let _ = std::fs::write(&path, json);
                }
                default_theme
            }
        }
    }

    fn color(&self, [r, g, b]: [u8; 3]) -> egui::Color32 {
        egui::Color32::from_rgb(r, g, b)
    }

    fn save_path(&mut self, path: &str) {
        self.last_crate_path = Some(path.to_string());
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::config_path(), json);
        }
    }
}

#[derive(PartialEq)]
enum ViewState {
    Turntable,
    Library,
    Tracklist,
    Help,
}

#[derive(Clone)]
struct Vinyl {
    album_title: String,
    artist_name: String,
    tracks: Vec<PathBuf>,
    cover_data: Option<Vec<u8>>,
}

struct EzoneApp {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    sink: Sink,
    vinyls: Vec<Vinyl>,
    active_vinyl_idx: Option<usize>,
    cover_textures: BTreeMap<usize, egui::TextureHandle>,
    current_track_idx: usize,
    needle_down: bool,
    rotation_angle: f32,
    view_state: ViewState,
    theme: Theme,
}

impl EzoneApp {
    fn new() -> Self {
        let (_stream, stream_handle) = OutputStream::try_default().expect("Audio init failed");
        let sink = Sink::try_new(&stream_handle).expect("Sink init failed");
        let theme = Theme::load();

        let mut app = Self {
            _stream,
            _stream_handle: stream_handle,
            sink,
            vinyls: Vec::new(),
            active_vinyl_idx: None,
            cover_textures: BTreeMap::new(),
            current_track_idx: 0,
            needle_down: false,
            rotation_angle: 0.0,
            view_state: ViewState::Turntable,
            theme,
        };

        if let Some(path) = &app.theme.last_crate_path {
            let path = PathBuf::from(path);
            if path.exists() && path.is_dir() {
                app.scan_crate(path);
            }
        }

        app
    }

    fn extract_cover(folder_path: &Path, first_track: &PathBuf) -> Option<Vec<u8>> {
        for img_name in &["cover.jpg", "cover.png", "folder.jpg", "front.jpg", "album.jpg"] {
            let img_path = folder_path.join(img_name);
            if let Ok(data) = std::fs::read(&img_path) {
                return Some(data);
            }
        }

        if let Ok(tagged_file) = Probe::open(first_track).and_then(|p| p.read()) {
            if let Some(tag) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
                if let Some(pic) = tag.pictures().first() {
                    return Some(pic.data().to_vec());
                }
            }
        }
        None
    }

    fn extract_album_metadata(folder_path: &Path, first_track: &PathBuf) -> (String, String) {
        let mut album = None;
        let mut artist = None;

        if let Ok(tagged_file) = Probe::open(first_track).and_then(|p| p.read()) {
            if let Some(tag) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
                album = tag.album().map(|s| s.to_string());
                artist = tag.artist().map(|s| s.to_string());
            }
        }

        let folder_name = folder_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown Album");

        let final_album = album.unwrap_or_else(|| {
            let parts: Vec<&str> = folder_name.split(" - ").collect();
            if parts.len() >= 2 {
                parts[1..].join(" - ").trim().to_string()
            } else {
                folder_name.to_string()
            }
        });

        let final_artist = artist.unwrap_or_else(|| {
            let parts: Vec<&str> = folder_name.split(" - ").collect();
            if parts.len() >= 2 {
                parts[0].trim().to_string()
            } else {
                "Unknown Artist".to_string()
            }
        });

        (final_album, final_artist)
    }

    fn parse_track_title(path: &PathBuf) -> String {
        if let Ok(tagged_file) = Probe::open(path).and_then(|p| p.read()) {
            if let Some(tag) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
                if let Some(title) = tag.title() {
                    if !title.trim().is_empty() {
                        return title.to_string();
                    }
                }
            }
        }

        let file_stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();

        regex_clean_track_prefix(&file_stem)
    }

    fn scan_crate(&mut self, root_path: PathBuf) {
        self.theme.save_path(root_path.to_str().unwrap_or_default());

        let mut crate_map: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();

        for entry in WalkDir::new(&root_path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if matches!(ext.to_lowercase().as_str(), "mp3" | "flac" | "wav" | "ogg" | "m4a") {
                        if let Some(parent) = path.parent() {
                            crate_map.entry(parent.to_path_buf()).or_default().push(path.to_path_buf());
                        }
                    }
                }
            }
        }

        let mut new_vinyls = Vec::new();
        for (folder_path, mut tracks) in crate_map {
            tracks.sort();

            if let Some(first_track) = tracks.first() {
                let cover_data = Self::extract_cover(&folder_path, first_track);
                let (album_title, artist_name) = Self::extract_album_metadata(&folder_path, first_track);

                new_vinyls.push(Vinyl {
                    album_title,
                    artist_name,
                    tracks,
                    cover_data,
                });
            }
        }

        self.vinyls = new_vinyls;
        self.active_vinyl_idx = None;
        self.needle_down = false;
        self.cover_textures.clear();
        self.sink.clear();

        if !self.vinyls.is_empty() {
            self.view_state = ViewState::Library;
        }
    }

    fn put_record_on_platter(&mut self, idx: usize) {
        self.active_vinyl_idx = Some(idx);
        self.current_track_idx = 0;
        self.needle_down = false;
        self.rotation_angle = 0.0;
        self.sink.clear();
        self.view_state = ViewState::Turntable;
    }

    fn play_current_track(&mut self) {
        self.sink.clear();
        if let Some(v_idx) = self.active_vinyl_idx {
            let vinyl = &self.vinyls[v_idx];
            if let Some(track_path) = vinyl.tracks.get(self.current_track_idx) {
                if let Ok(bytes) = std::fs::read(track_path) {
                    let cursor = Cursor::new(bytes);
                    let decode_result = panic::catch_unwind(AssertUnwindSafe(|| Decoder::new(cursor)));

                    if let Ok(Ok(source)) = decode_result {
                        self.sink.append(source);
                        self.sink.play();
                        self.needle_down = true;
                        return;
                    }
                }
            }
        }
        self.needle_down = false;
    }

    fn toggle_play_pause(&mut self) {
        if self.active_vinyl_idx.is_some() {
            if self.needle_down {
                self.sink.pause();
                self.needle_down = false;
            } else {
                if self.sink.empty() {
                    self.play_current_track();
                } else {
                    self.sink.play();
                    self.needle_down = true;
                }
            }
        }
    }

    fn load_texture(&mut self, ctx: &egui::Context, idx: usize) {
        if let Some(data) = &self.vinyls[idx].cover_data {
            if let Ok(image) = image::load_from_memory(data) {
                let size = [image.width() as _, image.height() as _];
                let image_buffer = image.to_rgba8();
                let pixels = image_buffer.as_flat_samples();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                let texture = ctx.load_texture(
                    format!("cover_{}", idx),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                self.cover_textures.insert(idx, texture);
            }
        }
    }
}

fn regex_clean_track_prefix(stem: &str) -> String {
    let bytes = stem.as_bytes();
    let mut idx = 0;

    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }

    if idx > 0 && idx < bytes.len() {
        while idx < bytes.len() && (bytes[idx] == b'.' || bytes[idx] == b'-' || bytes[idx] == b'_' || bytes[idx] == b' ') {
            idx += 1;
        }
        return stem[idx..].trim().to_string();
    }

    stem.to_string()
}

impl eframe::App for EzoneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut visuals = egui::Visuals::dark();
        let bg_color = self.theme.color(self.theme.bg);
        visuals.window_fill = bg_color;
        visuals.panel_fill = bg_color;
        ctx.set_visuals(visuals);

        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.toggle_play_pause();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::O)) {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                self.scan_crate(folder);
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::L)) {
            self.view_state = if self.view_state == ViewState::Library {
                ViewState::Turntable
            } else {
                ViewState::Library
            };
        }
        if ctx.input(|i| i.key_pressed(egui::Key::T)) {
            self.view_state = if self.view_state == ViewState::Tracklist {
                ViewState::Turntable
            } else {
                ViewState::Tracklist
            };
        }
        if ctx.input(|i| i.key_pressed(egui::Key::H)) {
            self.view_state = if self.view_state == ViewState::Help {
                ViewState::Turntable
            } else {
                ViewState::Help
            };
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.view_state = ViewState::Turntable;
        }

        if self.needle_down {
            if !self.sink.empty() {
                self.rotation_angle += 0.025;
                ctx.request_repaint();
            } else if let Some(v_idx) = self.active_vinyl_idx {
                if self.current_track_idx + 1 < self.vinyls[v_idx].tracks.len() {
                    self.current_track_idx += 1;
                    self.play_current_track();
                } else {
                    self.needle_down = false;
                }
            }
        }

        let missing_textures: Vec<usize> = self
            .vinyls
            .iter()
            .enumerate()
            .filter(|(i, v)| v.cover_data.is_some() && !self.cover_textures.contains_key(i))
            .map(|(i, _)| i)
            .collect();
        for i in missing_textures {
            self.load_texture(ctx, i);
        }

        let mut record_to_play = None;
        let mut track_to_jump = None;

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_color))
            .show(ctx, |ui| {
                if self.vinyls.is_empty() && self.view_state != ViewState::Help {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("PRESS 'O' TO LOAD")
                                .color(self.theme.color(self.theme.text_secondary))
                                .extra_letter_spacing(4.0)
                                .size(16.0),
                        );
                    });
                    return;
                }

                match self.view_state {
                    ViewState::Help => {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("CONTROLS")
                                    .color(self.theme.color(self.theme.text_primary))
                                    .extra_letter_spacing(6.0)
                                    .size(20.0)
                                    .strong(),
                            );
                            ui.add_space(40.0);
                            let lines = [
                                "O - LOAD DIRECTORY",
                                "L - TOGGLE LIBRARY",
                                "T - TOGGLE TRACKLIST",
                                "SPACE - PLAY / PAUSE",
                                "ESC - RETURN TO TURNTABLE",
                                "H - TOGGLE HELP",
                                "Q - QUIT",
                            ];
                            for line in lines {
                                ui.label(
                                    egui::RichText::new(line)
                                        .color(self.theme.color(self.theme.text_secondary))
                                        .extra_letter_spacing(4.0)
                                        .size(16.0),
                                );
                                ui.add_space(16.0);
                            }
                        });
                    }
                    ViewState::Library => {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("LIBRARY")
                                    .color(self.theme.color(self.theme.text_primary))
                                    .extra_letter_spacing(6.0)
                                    .size(20.0)
                                    .strong(),
                            );
                            ui.add_space(20.0);
                        });

                        egui::ScrollArea::vertical()
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                egui::Frame::none()
                                    .inner_margin(egui::Margin::symmetric(40.0, 20.0))
                                    .show(ui, |ui| {
                                        let item_width = 120.0;
                                        let spacing_x = 32.0;
                                        let spacing_y = 40.0;

                                        let available_width = ui.available_width();
                                        let max_columns = ((available_width + spacing_x) / (item_width + spacing_x)).floor() as usize;
                                        let columns = max_columns.max(1);

                                        let actual_columns = columns.min(self.vinyls.len());
                                        let grid_width = (actual_columns as f32 * item_width) + (actual_columns.saturating_sub(1) as f32 * spacing_x);
                                        let left_padding = ((available_width - grid_width) / 2.0).max(0.0);

                                        let items: Vec<_> = self.vinyls.iter().enumerate().collect();

                                        ui.spacing_mut().item_spacing = egui::vec2(spacing_x, spacing_y);

                                        for chunk in items.chunks(columns) {
                                            ui.horizontal(|ui| {
                                                if left_padding > 0.0 {
                                                    ui.add_space(left_padding);
                                                }

                                                for &(idx, vinyl) in chunk {
                                                    let is_active = Some(idx) == self.active_vinyl_idx;

                                                    ui.vertical(|ui| {
                                                        ui.set_width(item_width);
                                                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);

                                                        let (rect, response) = ui.allocate_exact_size(
                                                            egui::vec2(item_width, item_width),
                                                            egui::Sense::click(),
                                                        );

                                                        if let Some(texture) = self.cover_textures.get(&idx) {
                                                            ui.painter().image(
                                                                texture.id(),
                                                                rect,
                                                                egui::Rect::from_min_max(
                                                                    egui::pos2(0.0, 0.0),
                                                                    egui::pos2(1.0, 1.0),
                                                                ),
                                                                egui::Color32::WHITE,
                                                            );
                                                        } else {
                                                            ui.painter().rect_filled(
                                                                rect,
                                                                0.0,
                                                                self.theme.color(self.theme.record_base),
                                                            );
                                                        }

                                                        if response.clicked() {
                                                            record_to_play = Some(idx);
                                                        }

                                                        ui.add_space(4.0);

                                                        let t_color = if is_active {
                                                            self.theme.color(self.theme.accent)
                                                        } else {
                                                            self.theme.color(self.theme.text_primary)
                                                        };

                                                        ui.add(egui::Label::new(
                                                            egui::RichText::new(&vinyl.album_title)
                                                                .color(t_color)
                                                                .size(14.0)
                                                                .strong(),
                                                        ).truncate(true));

                                                        ui.add(egui::Label::new(
                                                            egui::RichText::new(&vinyl.artist_name)
                                                                .color(self.theme.color(self.theme.text_secondary))
                                                                .size(12.0),
                                                        ).truncate(true));
                                                    });
                                                }
                                            });
                                        }
                                    });
                            });
                    }
                    ViewState::Tracklist => {
                        if let Some(idx) = self.active_vinyl_idx {
                            let vinyl = &self.vinyls[idx];

                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new(vinyl.album_title.to_uppercase())
                                        .color(self.theme.color(self.theme.text_primary))
                                        .extra_letter_spacing(6.0)
                                        .size(20.0)
                                        .strong(),
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(vinyl.artist_name.to_uppercase())
                                        .color(self.theme.color(self.theme.text_secondary))
                                        .extra_letter_spacing(4.0)
                                        .size(14.0),
                                );
                                ui.add_space(40.0);
                            });

                            egui::ScrollArea::vertical()
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    egui::Frame::none()
                                        .inner_margin(egui::Margin::symmetric(40.0, 20.0))
                                        .show(ui, |ui| {
                                            ui.vertical(|ui| {
                                                ui.spacing_mut().item_spacing = egui::vec2(0.0, 12.0);

                                                for (t_idx, track_path) in vinyl.tracks.iter().enumerate() {
                                                    let is_playing = t_idx == self.current_track_idx;
                                                    let track_title = Self::parse_track_title(track_path);

                                                    ui.horizontal(|ui| {
                                                        ui.spacing_mut().item_spacing = egui::vec2(16.0, 0.0);

                                                        ui.add_sized([24.0, 14.0], |ui: &mut egui::Ui| {
                                                            ui.label(
                                                                egui::RichText::new(format!("{:02}", t_idx + 1))
                                                                    .color(self.theme.color(self.theme.text_secondary))
                                                                    .size(14.0),
                                                            )
                                                        });

                                                        let color = if is_playing {
                                                            self.theme.color(self.theme.accent)
                                                        } else {
                                                            self.theme.color(self.theme.text_primary)
                                                        };

                                                        let label = ui.add(
                                                            egui::SelectableLabel::new(
                                                                false,
                                                                egui::RichText::new(track_title)
                                                                    .color(color)
                                                                    .size(14.0),
                                                            )
                                                        );

                                                        if label.clicked() {
                                                            track_to_jump = Some(t_idx);
                                                        }
                                                    });
                                                }
                                            });
                                        });
                                });
                        } else {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    egui::RichText::new("NO RECORD SELECTED")
                                        .color(self.theme.color(self.theme.text_secondary))
                                        .extra_letter_spacing(4.0)
                                        .size(16.0),
                                );
                            });
                        }
                    }
                    ViewState::Turntable => {
                        ui.vertical_centered(|ui| {
                            let available_size = ui.available_size();
                            let (response, painter) = ui.allocate_painter(available_size, egui::Sense::click());
                            let center = response.rect.center();

                            let max_radius = (available_size.x.min(available_size.y) / 2.0) - 40.0;
                            let base_radius = max_radius.min(320.0);
                            let label_radius = base_radius * 0.35;

                            if response.clicked() && self.active_vinyl_idx.is_some() {
                                if let Some(pos) = response.interact_pointer_pos() {
                                    let dist = pos.distance(center);
                                    if dist > label_radius && dist < base_radius {
                                        if let Some(idx) = self.active_vinyl_idx {
                                            let total_tracks = self.vinyls[idx].tracks.len().max(1) as f32;
                                            let progress = 1.0 - ((dist - label_radius) / (base_radius - label_radius));
                                            let target_track = (progress * total_tracks).clamp(0.0, total_tracks - 1.0) as usize;
                                            track_to_jump = Some(target_track);
                                        }
                                    }
                                }
                            }

                            painter.circle_filled(
                                center,
                                base_radius + 6.0,
                                self.theme.color(self.theme.record_base),
                            );

                            if let Some(idx) = self.active_vinyl_idx {
                                painter.circle_filled(
                                    center,
                                    base_radius,
                                    self.theme.color(self.theme.record_vinyl),
                                );

                                for i in 1..=14 {
                                    painter.circle_stroke(
                                        center,
                                        base_radius - (i as f32 * (base_radius / 17.0)),
                                        egui::Stroke::new(0.5_f32, egui::Color32::from_white_alpha(5)),
                                    );
                                }

                                if let Some(texture) = self.cover_textures.get(&idx) {
                                    let mut mesh = egui::Mesh::with_texture(texture.id());
                                    let n = 64;
                                    let center_idx = mesh.vertices.len() as u32;

                                    mesh.vertices.push(egui::epaint::Vertex {
                                        pos: center,
                                        uv: egui::pos2(0.5, 0.5),
                                        color: egui::Color32::WHITE,
                                    });

                                    for i in 0..n {
                                        let theta = (i as f32 / n as f32) * std::f32::consts::TAU;
                                        let p = egui::pos2(
                                            center.x + label_radius * theta.cos(),
                                            center.y + label_radius * theta.sin(),
                                        );
                                        let uv_theta = theta - self.rotation_angle;
                                        let uv = egui::pos2(
                                            0.5 + 0.5 * uv_theta.cos(),
                                            0.5 + 0.5 * uv_theta.sin(),
                                        );
                                        mesh.vertices.push(egui::epaint::Vertex {
                                            pos: p,
                                            uv,
                                            color: egui::Color32::WHITE,
                                        });
                                    }

                                    for i in 0..n {
                                        mesh.indices.push(center_idx);
                                        mesh.indices.push(center_idx + 1 + i);
                                        mesh.indices.push(center_idx + 1 + (i + 1) % n);
                                    }

                                    painter.add(egui::Shape::mesh(mesh));
                                } else {
                                    painter.circle_filled(
                                        center,
                                        label_radius,
                                        self.theme.color(self.theme.record_base),
                                    );
                                }

                                painter.circle_filled(center, 5.0, self.theme.color(self.theme.bg));
                            }
                        });
                    }
                }
            });

        if let Some(idx) = record_to_play {
            self.put_record_on_platter(idx);
        }
        if let Some(t_idx) = track_to_jump {
            self.current_track_idx = t_idx;
            self.play_current_track();
            self.view_state = ViewState::Turntable;
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 800.0])
            .with_min_inner_size([700.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("Ezone", options, Box::new(|_cc| Box::new(EzoneApp::new())))
}