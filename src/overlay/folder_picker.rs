//! Small cross-platform folder browser; also accepts pasted Wine/Proton paths.
use eframe::egui;
use std::{fs, path::PathBuf};

pub struct FolderPicker {
    current: PathBuf,
    input: String,
    folders: Vec<PathBuf>,
    error: Option<String>,
    pub closed: bool,
}
impl FolderPicker {
    pub fn new(initial: PathBuf) -> Self {
        let mut picker = Self {
            current: PathBuf::new(),
            input: String::new(),
            folders: Vec::new(),
            error: None,
            closed: false,
        };
        if initial.is_dir() {
            picker.navigate(initial);
        } else {
            picker.navigate(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        }
        picker
    }
    fn navigate(&mut self, path: PathBuf) {
        let result = path.canonicalize().and_then(|path| {
            let mut folders = fs::read_dir(&path)?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect::<Vec<_>>();
            folders.sort_by_cached_key(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
            });
            Ok((path, folders))
        });
        match result {
            Ok((path, folders)) => {
                self.input = path.display().to_string();
                self.current = path;
                self.folders = folders;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Cannot open folder: {e}")),
        }
    }
    pub fn show(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        let mut open = true;
        let mut selected = None;
        let mut navigate = None;
        egui::Window::new("Choose LFS installation folder")
            .open(&mut open)
            .default_width(560.0)
            .min_width(320.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("Choose the folder containing LFS.exe.");
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.input)
                            .desired_width((ui.available_width() - 60.0).max(100.0)),
                    );
                    if ui.button("Go").clicked()
                        || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        navigate = Some(PathBuf::from(self.input.trim()));
                    }
                });
                if ui
                    .add_enabled(
                        self.current.parent().is_some(),
                        egui::Button::new("Parent folder"),
                    )
                    .clicked()
                {
                    navigate = self.current.parent().map(PathBuf::from);
                }
                ui.label(format!("Current folder: {}", self.current.display()));
                egui::ScrollArea::vertical()
                    .max_height(250.0)
                    .show(ui, |ui| {
                        for path in &self.folders {
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            if ui
                                .selectable_label(false, format!("[Folder] {name}"))
                                .clicked()
                            {
                                navigate = Some(path.clone());
                            }
                        }
                        if self.folders.is_empty() {
                            ui.label("No subfolders");
                        }
                    });
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Choose this folder").clicked() {
                        selected = Some(self.current.clone());
                        self.closed = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.closed = true;
                    }
                });
            });
        self.closed |= !open;
        if let Some(path) = navigate {
            self.navigate(path);
        }
        selected
    }
}
