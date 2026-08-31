use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileDialogOptions {
    pub title: Option<String>,
    pub directory: Option<PathBuf>,
    pub file_name: Option<String>,
    pub filters: Vec<FileDialogFilter>,
}

impl FileDialogOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.directory = Some(directory.into());
        self
    }

    pub fn file_name(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = Some(file_name.into());
        self
    }

    pub fn filter(
        mut self,
        name: impl Into<String>,
        extensions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.filters.push(FileDialogFilter {
            name: name.into(),
            extensions: extensions.into_iter().map(Into::into).collect(),
        });
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDialogFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

pub trait FileDialogService: Send + Sync + 'static {
    fn pick_file(&self, options: &FileDialogOptions) -> Option<PathBuf>;
    fn pick_files(&self, options: &FileDialogOptions) -> Option<Vec<PathBuf>>;
    fn save_file(&self, options: &FileDialogOptions) -> Option<PathBuf>;
    fn pick_folder(&self, options: &FileDialogOptions) -> Option<PathBuf>;
}

#[derive(Clone)]
pub struct FileDialogHandle(Arc<dyn FileDialogService>);

impl FileDialogHandle {
    pub fn new(service: impl FileDialogService) -> Self {
        Self(Arc::new(service))
    }

    pub fn pick_file(&self, options: &FileDialogOptions) -> Option<PathBuf> {
        self.0.pick_file(options)
    }

    pub fn pick_files(&self, options: &FileDialogOptions) -> Option<Vec<PathBuf>> {
        self.0.pick_files(options)
    }

    pub fn save_file(&self, options: &FileDialogOptions) -> Option<PathBuf> {
        self.0.save_file(options)
    }

    pub fn pick_folder(&self, options: &FileDialogOptions) -> Option<PathBuf> {
        self.0.pick_folder(options)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemFileDialogs;

impl FileDialogService for SystemFileDialogs {
    fn pick_file(&self, options: &FileDialogOptions) -> Option<PathBuf> {
        dialog(options).pick_file()
    }

    fn pick_files(&self, options: &FileDialogOptions) -> Option<Vec<PathBuf>> {
        dialog(options).pick_files()
    }

    fn save_file(&self, options: &FileDialogOptions) -> Option<PathBuf> {
        dialog(options).save_file()
    }

    fn pick_folder(&self, options: &FileDialogOptions) -> Option<PathBuf> {
        dialog(options).pick_folder()
    }
}

pub fn system_file_dialogs() -> FileDialogHandle {
    FileDialogHandle::new(SystemFileDialogs)
}

fn dialog(options: &FileDialogOptions) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new();
    if let Some(title) = &options.title {
        dialog = dialog.set_title(title);
    }
    if let Some(directory) = &options.directory {
        dialog = dialog.set_directory(directory);
    }
    if let Some(file_name) = &options.file_name {
        dialog = dialog.set_file_name(file_name);
    }
    for filter in &options.filters {
        let extensions = filter
            .extensions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        dialog = dialog.add_filter(&filter.name, &extensions);
    }
    dialog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_builder_preserves_filters() {
        let options = FileDialogOptions::new()
            .title("Open image")
            .filter("Images", ["png", "jpg"]);
        assert_eq!(options.title.as_deref(), Some("Open image"));
        assert_eq!(options.filters[0].extensions, ["png", "jpg"]);
    }
}
