use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::cursor::{Alias, CursorTheme};
use crate::domain::error::RepositoryError;
use crate::domain::ports::CursorRepository;

use super::atomic_write::write_atomic;

#[derive(Serialize, Deserialize, Default)]
struct RegistryFile {
    #[serde(default)]
    cursor: Vec<EntryDto>,
}

#[derive(Serialize, Deserialize)]
struct EntryDto {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
    path: PathBuf,
}

impl EntryDto {
    fn from_theme(theme: &CursorTheme) -> Self {
        Self {
            name: theme.name.clone(),
            alias: theme.alias.as_ref().map(|a| a.as_str().to_string()),
            path: theme.path.clone(),
        }
    }

    fn into_theme(self) -> Result<CursorTheme, RepositoryError> {
        let label = self.name.clone();
        let invalid = |e: crate::domain::error::DomainError| {
            RepositoryError::Corrupt(format!("entry {label:?}: {e}"))
        };
        let alias = self
            .alias
            .as_deref()
            .map(Alias::new)
            .transpose()
            .map_err(invalid)?;
        CursorTheme::new(&self.name, alias, self.path).map_err(invalid)
    }
}

pub struct TomlCursorRepository {
    path: PathBuf,
}

impl TomlCursorRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load(&self) -> Result<Vec<CursorTheme>, RepositoryError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(storage(&self.path, e)),
        };
        let file: RegistryFile = toml::from_str(&text)
            .map_err(|e| RepositoryError::Corrupt(format!("{}: {e}", self.path.display())))?;
        file.cursor.into_iter().map(EntryDto::into_theme).collect()
    }

    fn save(&self, themes: &[CursorTheme]) -> Result<(), RepositoryError> {
        let file = RegistryFile {
            cursor: themes.iter().map(EntryDto::from_theme).collect(),
        };
        let text = toml::to_string(&file)
            .map_err(|e| RepositoryError::Storage(format!("cannot encode registry: {e}")))?;
        write_atomic(&self.path, text.as_bytes()).map_err(|e| storage(&self.path, e))
    }
}

fn storage(path: &Path, e: io::Error) -> RepositoryError {
    RepositoryError::Storage(format!("{}: {e}", path.display()))
}

/// Registry location derived from the given environment lookup.
pub fn registry_path_from(env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let non_empty = |key| env(key).filter(|v| !v.is_empty());
    let base = match non_empty("XDG_CONFIG_HOME") {
        Some(xdg) => PathBuf::from(xdg),
        None => PathBuf::from(non_empty("HOME")?).join(".config"),
    };
    Some(base.join("cursorswap").join("conf").join("registry.toml"))
}

impl CursorRepository for TomlCursorRepository {
    fn list(&self) -> Result<Vec<CursorTheme>, RepositoryError> {
        self.load()
    }

    fn add(&mut self, theme: CursorTheme) -> Result<(), RepositoryError> {
        let mut themes = self.load()?;
        if themes.iter().any(|t| t.name == theme.name) {
            return Err(RepositoryError::Duplicate(theme.name));
        }
        themes.push(theme);
        self.save(&themes)
    }

    fn remove(&mut self, name: &str) -> Result<(), RepositoryError> {
        let mut themes = self.load()?;
        let index = themes
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| RepositoryError::NotFound(name.to_string()))?;
        themes.remove(index);
        self.save(&themes)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use crate::domain::cursor::Alias;

    use super::*;

    fn theme(name: &str, alias: Option<&str>) -> CursorTheme {
        CursorTheme::new(
            name,
            alias.map(|a| Alias::new(a).unwrap()),
            PathBuf::from(format!("/icons/{name}")),
        )
        .unwrap()
    }

    fn repo_in(tmp: &TempDir) -> TomlCursorRepository {
        TomlCursorRepository::new(tmp.path().join("registry.toml"))
    }

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| {
            owned
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn default_path_uses_xdg_config_home() {
        let env = env_of(&[("XDG_CONFIG_HOME", "/xdg"), ("HOME", "/home/u")]);
        assert_eq!(
            registry_path_from(env),
            Some(PathBuf::from("/xdg/cursorswap/conf/registry.toml"))
        );
    }

    #[test]
    fn default_path_falls_back_to_home() {
        let env = env_of(&[("HOME", "/home/u")]);
        assert_eq!(
            registry_path_from(env),
            Some(PathBuf::from(
                "/home/u/.config/cursorswap/conf/registry.toml"
            ))
        );
    }

    #[test]
    fn default_path_treats_empty_xdg_as_unset() {
        let env = env_of(&[("XDG_CONFIG_HOME", ""), ("HOME", "/home/u")]);
        assert_eq!(
            registry_path_from(env),
            Some(PathBuf::from(
                "/home/u/.config/cursorswap/conf/registry.toml"
            ))
        );
    }

    #[test]
    fn default_path_is_none_without_env() {
        assert_eq!(registry_path_from(env_of(&[])), None);
    }

    #[test]
    fn missing_file_is_empty_registry() {
        let tmp = TempDir::new().unwrap();
        assert!(repo_in(&tmp).list().unwrap().is_empty());
    }

    #[test]
    fn add_then_list_roundtrips_with_and_without_alias() {
        let tmp = TempDir::new().unwrap();
        let mut repo = repo_in(&tmp);
        repo.add(theme("Nordzy-cursors", Some("N"))).unwrap();
        repo.add(theme("Plain", None)).unwrap();
        assert_eq!(
            repo.list().unwrap(),
            vec![theme("Nordzy-cursors", Some("N")), theme("Plain", None)]
        );
    }

    #[test]
    fn order_is_preserved() {
        let tmp = TempDir::new().unwrap();
        let mut repo = repo_in(&tmp);
        for name in ["zeta", "alpha", "mid"] {
            repo.add(theme(name, None)).unwrap();
        }
        let names: Vec<String> = repo.list().unwrap().into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["zeta", "alpha", "mid"]);
    }

    #[test]
    fn remove_matches_name_case_insensitively() {
        let tmp = TempDir::new().unwrap();
        let mut repo = repo_in(&tmp);
        repo.add(theme("One", None)).unwrap();
        repo.add(theme("Two", Some("t"))).unwrap();
        repo.remove("oNE").unwrap();
        assert_eq!(repo.list().unwrap(), vec![theme("Two", Some("t"))]);
    }

    #[test]
    fn remove_missing_is_error() {
        let tmp = TempDir::new().unwrap();
        let mut repo = repo_in(&tmp);
        repo.add(theme("One", None)).unwrap();
        assert!(matches!(
            repo.remove("nope"),
            Err(RepositoryError::NotFound(_))
        ));
        assert_eq!(repo.list().unwrap().len(), 1);
    }

    #[test]
    fn duplicate_name_add_is_error() {
        let tmp = TempDir::new().unwrap();
        let mut repo = repo_in(&tmp);
        repo.add(theme("One", None)).unwrap();
        assert!(matches!(
            repo.add(theme("One", Some("o"))),
            Err(RepositoryError::Duplicate(_))
        ));
        assert_eq!(repo.list().unwrap().len(), 1);
    }

    #[test]
    fn invalid_alias_in_file_names_the_entry() {
        let tmp = TempDir::new().unwrap();
        let repo = repo_in(&tmp);
        fs::write(
            tmp.path().join("registry.toml"),
            "[[cursor]]\nname = \"Broken\"\nalias = \"bad-alias\"\npath = \"/x\"\n",
        )
        .unwrap();
        let err = repo.list().unwrap_err();
        assert!(matches!(err, RepositoryError::Corrupt(_)));
        assert!(err.to_string().contains("Broken"), "{err}");
    }

    #[test]
    fn empty_name_in_file_is_error() {
        let tmp = TempDir::new().unwrap();
        let repo = repo_in(&tmp);
        fs::write(
            tmp.path().join("registry.toml"),
            "[[cursor]]\nname = \"  \"\npath = \"/x\"\n",
        )
        .unwrap();
        assert!(matches!(repo.list(), Err(RepositoryError::Corrupt(_))));
    }

    #[test]
    fn corrupt_toml_is_error_and_file_untouched() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("registry.toml");
        let garbage = "[[cursor\nname = = nope";
        fs::write(&file, garbage).unwrap();
        let mut repo = repo_in(&tmp);
        assert!(matches!(repo.list(), Err(RepositoryError::Corrupt(_))));
        assert!(repo.add(theme("One", None)).is_err());
        assert!(repo.remove("One").is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), garbage);
    }

    #[test]
    fn parent_directory_is_created_on_first_add() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("a/b/registry.toml");
        let mut repo = TomlCursorRepository::new(path.clone());
        repo.add(theme("One", None)).unwrap();
        assert!(path.is_file());
    }

    #[test]
    fn written_file_matches_exact_format() {
        let tmp = TempDir::new().unwrap();
        let mut repo = repo_in(&tmp);
        repo.add(theme("Nordzy-cursors", Some("N"))).unwrap();
        repo.add(theme("Plain", None)).unwrap();
        let text = fs::read_to_string(tmp.path().join("registry.toml")).unwrap();
        let expected = "[[cursor]]\n\
            name = \"Nordzy-cursors\"\n\
            alias = \"N\"\n\
            path = \"/icons/Nordzy-cursors\"\n\
            \n\
            [[cursor]]\n\
            name = \"Plain\"\n\
            path = \"/icons/Plain\"\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn no_temp_file_is_left_after_write() {
        let tmp = TempDir::new().unwrap();
        let mut repo = repo_in(&tmp);
        repo.add(theme("One", None)).unwrap();
        repo.remove("One").unwrap();
        let entries: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, [OsString::from("registry.toml")]);
    }
}
