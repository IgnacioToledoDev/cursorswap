mod appliers;
mod atomic_write;
mod current_provider;
mod envs_conf;
mod fs_theme_inspector;
mod registry_toml;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "preview is consumed by the TUI in T7")
)]
// T7 re-exports `XcursorPreviewLoader` once the TUI uses it.
mod xcursor_preview;

#[cfg(test)]
pub use appliers::command::fake::FakeRunner;
pub use appliers::{
    CommandRunner, GsettingsApplier, GtkSettingsApplier, HyprEnvsApplier, HyprctlApplier,
    IconPathApplier, SystemCommandRunner,
};
pub use current_provider::EnvsConfCurrentProvider;
pub use envs_conf::read_cursor_size;
pub use fs_theme_inspector::FsThemeInspector;
pub use registry_toml::{TomlCursorRepository, registry_path_from};
