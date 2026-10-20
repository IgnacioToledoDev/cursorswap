mod appliers;
mod atomic_write;
mod current_provider;
mod envs_conf;
mod fs_theme_inspector;
mod registry_toml;
mod xcursor_preview;

// Re-exports are consumed by the composition root in a later task.
#[allow(unused_imports)]
pub use appliers::{
    CommandRunner, DEFAULT_CURSOR_SIZE, GsettingsApplier, GtkSettingsApplier, HyprEnvsApplier,
    HyprctlApplier, IconPathApplier, SystemCommandRunner,
};
#[allow(unused_imports)]
pub use current_provider::EnvsConfCurrentProvider;
#[allow(unused_imports)]
pub use fs_theme_inspector::FsThemeInspector;
#[allow(unused_imports)]
pub use registry_toml::{TomlCursorRepository, default_registry_path};
#[allow(unused_imports)]
pub use xcursor_preview::XcursorPreviewLoader;
