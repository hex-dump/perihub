//! Suggests app ids for the games installed on this computer, to prefill App profiles rules.
//!
//! Read only: it parses the launchers' own files (Steam, Lutris, Heroic) and lists folders. It
//! does not use the network and never looks at running processes. The app id is what focus
//! detection reports: the executable name on Windows, the window class on Linux. A game's window
//! class cannot be known without running it, so each game says whether the id is certain.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledGame {
    pub name: String,
    /// "Steam", "Lutris" or "Heroic".
    pub launcher: String,
    /// What to put in the rule: the executable name (Windows) or the window class (Linux).
    pub app: String,
    /// False when the id is a best guess that may need correcting.
    pub certain: bool,
    /// Why the id is not certain, in plain words.
    pub note: Option<String>,
}

/// Where to look. Separate from the real folders so the tests can use a fake home.
struct Places {
    windows: bool,
    home: Option<PathBuf>,
    /// Roaming app data on Windows (Heroic).
    appdata: Option<PathBuf>,
    /// Program Files folders on Windows (Steam).
    program_files: Vec<PathBuf>,
}

/// The games found, sorted by name. Never fails: a launcher that is not installed gives nothing.
pub fn installed_games() -> Vec<InstalledGame> {
    let windows = cfg!(windows);
    let program_files = if windows {
        ["ProgramFiles(x86)", "ProgramFiles"]
            .iter()
            .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
            .chain([PathBuf::from(r"C:\Program Files (x86)"), PathBuf::from(r"C:\Program Files")])
            .collect()
    } else {
        Vec::new()
    };
    let places = Places { windows, home: dirs::home_dir(), appdata: dirs::config_dir(), program_files };
    find(&places)
}

fn find(places: &Places) -> Vec<InstalledGame> {
    let mut games = steam(places);
    games.extend(lutris(places));
    games.extend(heroic(places));
    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.launcher.cmp(&b.launcher)));
    games
}

// ---- Steam ----

fn steam_roots(places: &Places) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if places.windows {
        roots.extend(places.program_files.iter().map(|p| p.join("Steam")));
    } else if let Some(home) = &places.home {
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }
    roots
}

/// Tools that Steam lists next to the games.
fn steam_tool(appid: &str, name: &str) -> bool {
    appid == "228980"
        || name.starts_with("Proton")
        || name.starts_with("Steam Linux Runtime")
        || name.starts_with("Steamworks Common")
}

fn steam(places: &Places) -> Vec<InstalledGame> {
    // Folders by their real path, so the usual symlinks to one Steam folder count once.
    let mut libraries: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if let Ok(real) = fs::canonicalize(&p)
            && !libraries.contains(&real)
        {
            libraries.push(real);
        }
    };
    for root in steam_roots(places) {
        if !root.join("steamapps").is_dir() {
            continue;
        }
        add(root.clone());
        if let Ok(text) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            for path in library_paths(&text) {
                add(PathBuf::from(path));
            }
        }
    }

    let mut seen = HashSet::new();
    let mut games = Vec::new();
    for library in libraries {
        let steamapps = library.join("steamapps");
        let Ok(entries) = fs::read_dir(&steamapps) else { continue };
        let mut manifests: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("appmanifest_") && n.ends_with(".acf"))
            })
            .collect();
        manifests.sort();
        for manifest in manifests {
            let Ok(text) = fs::read_to_string(&manifest) else { continue };
            let Some((appid, name, installdir)) = parse_manifest(&text) else { continue };
            if steam_tool(&appid, &name) || !seen.insert(appid.clone()) {
                continue;
            }
            let game = if places.windows {
                steam_windows(&name, &steamapps.join("common").join(&installdir))
            } else {
                Some(steam_linux(&name, &appid, &steamapps))
            };
            games.extend(game);
        }
    }
    games
}

/// Steam starts Windows games through Proton, which sets the window class to steam_app_<id>.
/// Proton keeps a folder for the game once it has run, so that folder tells which games use it.
fn steam_linux(name: &str, appid: &str, steamapps: &Path) -> InstalledGame {
    let proton = steamapps.join("compatdata").join(appid).is_dir();
    InstalledGame {
        name: name.to_string(),
        launcher: "Steam".into(),
        app: format!("steam_app_{appid}"),
        certain: proton,
        note: (!proton).then(|| {
            "No Proton folder for this game yet. If it is a native Linux game, its window class can be different: \
             start it and check \"App in focus\"."
                .into()
        }),
    }
}

/// On Windows the manifest does not name the executable: take it from the game's folder.
fn steam_windows(name: &str, folder: &Path) -> Option<InstalledGame> {
    let mut exes: Vec<(u64, String)> = fs::read_dir(folder)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            let lower = file.to_lowercase();
            let skip = ["unins", "crash", "redist", "dxsetup", "dotnet", "vc_", "vcredist"];
            if !lower.ends_with(".exe") || skip.iter().any(|s| lower.contains(s)) {
                return None;
            }
            Some((e.metadata().map(|m| m.len()).unwrap_or(0), file))
        })
        .collect();
    exes.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let certain = exes.len() == 1;
    let (_, app) = exes.into_iter().next()?;
    Some(InstalledGame {
        name: name.to_string(),
        launcher: "Steam".into(),
        app,
        certain,
        note: (!certain).then(|| "Several programs in the game's folder: this is the biggest one.".into()),
    })
}

// ---- Valve's text format (VDF / ACF) ----

#[derive(Debug, PartialEq)]
enum Vdf {
    Text(String),
    Map(Vec<(String, Vdf)>),
}

impl Vdf {
    fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Map(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Vdf::Text(_) => None,
        }
    }

    fn text(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Vdf::Text(t) => Some(t),
            Vdf::Map(_) => None,
        }
    }
}

enum Token {
    Text(String),
    Open,
    Close,
}

fn tokens(src: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => out.push(Token::Open),
            '}' => out.push(Token::Close),
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(other) => s.push(other),
                            None => break,
                        },
                        c => s.push(c),
                    }
                }
                out.push(Token::Text(s));
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Parses the top level of a file: a list of "key" value or "key" { ... } entries.
fn parse_vdf(src: &str) -> Vdf {
    let toks = tokens(src);
    let mut pos = 0;
    Vdf::Map(parse_entries(&toks, &mut pos, 0))
}

fn parse_entries(toks: &[Token], pos: &mut usize, depth: usize) -> Vec<(String, Vdf)> {
    let mut items = Vec::new();
    while *pos < toks.len() {
        match &toks[*pos] {
            Token::Close => {
                *pos += 1;
                return items;
            }
            Token::Open => *pos += 1,
            Token::Text(key) => {
                *pos += 1;
                match toks.get(*pos) {
                    Some(Token::Text(value)) => {
                        items.push((key.clone(), Vdf::Text(value.clone())));
                        *pos += 1;
                    }
                    // A file cannot make the parser recurse without limit.
                    Some(Token::Open) if depth < 16 => {
                        *pos += 1;
                        items.push((key.clone(), Vdf::Map(parse_entries(toks, pos, depth + 1))));
                    }
                    _ => {}
                }
            }
        }
    }
    items
}

/// The extra library folders of libraryfolders.vdf, in both its old and new layout.
fn library_paths(src: &str) -> Vec<String> {
    let Vdf::Map(top) = parse_vdf(src) else { return Vec::new() };
    let Some((_, Vdf::Map(folders))) = top.iter().find(|(k, _)| k.eq_ignore_ascii_case("libraryfolders")) else {
        return Vec::new();
    };
    folders
        .iter()
        .filter(|(k, _)| k.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|(_, v)| match v {
            Vdf::Map(_) => v.text("path").map(str::to_string),
            Vdf::Text(t) => Some(t.clone()),
        })
        .collect()
}

/// App id, name and install folder of an appmanifest file.
fn parse_manifest(src: &str) -> Option<(String, String, String)> {
    let state = parse_vdf(src);
    let state = state.get("AppState")?;
    let appid = state.text("appid")?;
    if appid.is_empty() || !appid.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let name = state.text("name").filter(|n| !n.is_empty())?;
    let dir = state.text("installdir").unwrap_or_default();
    // The folder name is joined to a path: never let it leave the library.
    if dir.contains("..") || dir.contains('/') || dir.contains('\\') {
        return Some((appid.into(), name.into(), String::new()));
    }
    Some((appid.into(), name.into(), dir.into()))
}

// ---- Lutris ----

fn lutris(places: &Places) -> Vec<InstalledGame> {
    let Some(home) = places.home.as_ref().filter(|_| !places.windows) else { return Vec::new() };
    let mut games = Vec::new();
    for dir in [home.join(".config/lutris/games"), home.join(".var/app/net.lutris.Lutris/config/lutris/games")] {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> =
            entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "yml")).collect();
        files.sort();
        for file in files {
            let Ok(text) = fs::read_to_string(&file) else { continue };
            let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            games.extend(parse_lutris(&text, stem));
        }
    }
    games
}

/// One Lutris game file (games/<slug>-<number>.yml). Only the few plain "key: value" lines that
/// matter are read, so a full YAML parser is not needed.
fn parse_lutris(src: &str, file_stem: &str) -> Option<InstalledGame> {
    let (mut name, mut runner, mut exe, mut section) = (None, None, None, "");
    for line in src.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indented = line.starts_with(' ');
        let Some((key, value)) = line.trim().split_once(':') else { continue };
        let value = unquote(value.trim());
        if !indented {
            section = if value.is_empty() { key } else { "" };
            match key {
                "name" => name = Some(value),
                "runner" => runner = Some(value),
                _ => {}
            }
        } else if section == "game" && (key == "exe" || (key == "main_file" && exe.is_none())) {
            exe = Some(value);
        }
    }
    let runner = runner.unwrap_or_default();
    // Steam games come from Steam itself.
    if runner == "steam" {
        return None;
    }
    let exe = exe.filter(|e| !e.is_empty())?;
    let app = file_name(&exe)?;
    let name = name.filter(|n| !n.is_empty()).unwrap_or_else(|| name_from_slug(file_stem));
    let wine = matches!(runner.as_str(), "wine" | "dxvk" | "proton") || app.to_lowercase().ends_with(".exe");
    Some(InstalledGame {
        name,
        launcher: "Lutris".into(),
        app,
        certain: false,
        note: Some(if wine {
            "Games run with Wine usually show the executable name as their window class.".into()
        } else {
            "Native games choose their own window class: start the game and check \"App in focus\".".into()
        }),
    })
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    for q in ['"', '\''] {
        if let Some(inner) = v.strip_prefix(q).and_then(|v| v.strip_suffix(q)) {
            return inner.to_string();
        }
    }
    v.to_string()
}

/// "the-witcher-3-1612345678" becomes "The Witcher 3".
fn name_from_slug(stem: &str) -> String {
    let (base, last) = stem.rsplit_once('-').unwrap_or((stem, ""));
    let base = if !last.is_empty() && last.chars().all(|c| c.is_ascii_digit()) { base } else { stem };
    base.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The last part of a path written with either kind of slash.
fn file_name(path: &str) -> Option<String> {
    path.rsplit(['/', '\\']).next().filter(|n| !n.is_empty()).map(str::to_string)
}

// ---- Heroic ----

fn heroic_dirs(places: &Places) -> Vec<PathBuf> {
    if places.windows {
        places.appdata.iter().map(|a| a.join("heroic")).collect()
    } else if let Some(home) = &places.home {
        vec![home.join(".config/heroic"), home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic")]
    } else {
        Vec::new()
    }
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    // Library files are small; a huge file is not one of Heroic's.
    if fs::metadata(path).ok()?.len() > 64 * 1024 * 1024 {
        return None;
    }
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn str_of<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str().filter(|s| !s.is_empty())
}

fn heroic(places: &Places) -> Vec<InstalledGame> {
    let mut games = Vec::new();
    for dir in heroic_dirs(places) {
        // Epic (legendary): a map of app name to the installed game.
        if let Some(v) = read_json(&dir.join("legendaryConfig/legendary/installed.json"))
            && let Some(map) = v.as_object()
        {
            for (key, g) in map {
                let exe = str_of(g, "executable").and_then(file_name);
                let title = str_of(g, "title").unwrap_or(key);
                games.extend(heroic_game(places, title, exe, str_of(g, "platform")));
            }
        }
        // GOG: installed.json lists folders; names are in the library cache, and the program is
        // named by the game's own goggame-*.info file.
        if let Some(v) = read_json(&dir.join("gog_store/installed.json")) {
            let titles = read_json(&dir.join("store_cache/gog_library.json"));
            for g in v.get("installed").and_then(|i| i.as_array()).into_iter().flatten() {
                let (Some(id), Some(path)) = (str_of(g, "appName"), str_of(g, "install_path")) else { continue };
                let title = titles
                    .as_ref()
                    .and_then(|t| t.get("games")?.as_array()?.iter().find(|x| str_of(x, "app_name") == Some(id)))
                    .and_then(|x| str_of(x, "title"))
                    .unwrap_or(id);
                games.extend(heroic_game(places, title, gog_executable(Path::new(path)), str_of(g, "platform")));
            }
        }
        // Games added by hand.
        if let Some(v) = read_json(&dir.join("sideload_apps/library.json")) {
            for g in v.get("games").and_then(|i| i.as_array()).into_iter().flatten() {
                if g.get("is_installed").and_then(|b| b.as_bool()) == Some(false) {
                    continue;
                }
                let install = g.get("install").unwrap_or(g);
                let exe = str_of(install, "executable").and_then(file_name);
                let title = str_of(g, "title").or(str_of(g, "app_name")).unwrap_or_default();
                games.extend(heroic_game(places, title, exe, str_of(install, "platform")));
            }
        }
    }
    let mut seen = HashSet::new();
    games.retain(|g| seen.insert((g.name.clone(), g.app.to_lowercase())));
    games
}

fn gog_executable(install: &Path) -> Option<String> {
    let mut infos: Vec<PathBuf> = fs::read_dir(install)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("goggame-") && n.ends_with(".info"))
        })
        .collect();
    infos.sort();
    let info = read_json(infos.first()?)?;
    let tasks = info.get("playTasks")?.as_array()?;
    let task = tasks.iter().find(|t| t.get("isPrimary").and_then(|p| p.as_bool()) == Some(true)).or(tasks.first())?;
    str_of(task, "path").and_then(file_name)
}

fn heroic_game(places: &Places, title: &str, exe: Option<String>, platform: Option<&str>) -> Option<InstalledGame> {
    let app = exe?;
    if title.is_empty() {
        return None;
    }
    let windows_game =
        platform.is_none_or(|p| p.eq_ignore_ascii_case("windows")) && app.to_lowercase().ends_with(".exe");
    let (certain, note) = if places.windows {
        (true, None)
    } else if windows_game {
        (false, Some("Games run with Wine usually show the executable name as their window class.".to_string()))
    } else {
        (
            false,
            Some("Native games choose their own window class: start the game and check \"App in focus\".".to_string()),
        )
    };
    Some(InstalledGame { name: title.to_string(), launcher: "Heroic".into(), app, certain, note })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-games-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn put(path: PathBuf, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn linux(home: &Path) -> Places {
        Places { windows: false, home: Some(home.to_path_buf()), appdata: None, program_files: Vec::new() }
    }

    const MANIFEST: &str = "\"AppState\"\n{\n\t\"appid\"\t\t\"1245620\"\n\t\"name\"\t\t\"ELDEN RING\"\n\t\"installdir\"\t\t\"ELDEN RING\"\n}\n";

    #[test]
    fn reads_both_library_layouts_and_comments() {
        let new = "\"libraryfolders\"\n{\n // c\n \"0\" { \"path\" \"/home/a/.local/share/Steam\" \"apps\" { \"1\" \"2\" } }\n \"1\" { \"path\" \"D:\\\\Games\\\\Steam\" }\n}";
        assert_eq!(library_paths(new), ["/home/a/.local/share/Steam", r"D:\Games\Steam"]);
        let old = "\"LibraryFolders\"\n{\n\"TimeNextStatsReport\" \"1\"\n\"1\" \"/mnt/games\"\n}";
        assert_eq!(library_paths(old), ["/mnt/games"]);
        assert!(library_paths("garbage {{{ \"a\"").is_empty());
    }

    #[test]
    fn manifest_needs_a_numeric_id_and_a_safe_folder() {
        assert_eq!(parse_manifest(MANIFEST), Some(("1245620".into(), "ELDEN RING".into(), "ELDEN RING".into())));
        assert_eq!(parse_manifest("\"AppState\" { \"appid\" \"x\" \"name\" \"n\" }"), None);
        let evil = "\"AppState\" { \"appid\" \"5\" \"name\" \"n\" \"installdir\" \"..\\\\..\" }";
        assert_eq!(parse_manifest(evil), Some(("5".into(), "n".into(), String::new())));
        // Deep nesting does not overflow the stack.
        let deep = "\"a\" {".repeat(10_000);
        let _ = parse_vdf(&deep);
    }

    #[test]
    #[cfg(unix)]
    fn steam_on_linux_marks_proton_games_certain() {
        let home = temp("steam-linux");
        let steam = home.join(".local/share/Steam");
        let extra = home.join("lib2");
        put(steam.join("steamapps/appmanifest_1245620.acf"), MANIFEST);
        fs::create_dir_all(steam.join("steamapps/compatdata/1245620")).unwrap();
        put(
            steam.join("steamapps/appmanifest_228980.acf"),
            "\"AppState\" { \"appid\" \"228980\" \"name\" \"Steamworks Common Redistributables\" }",
        );
        put(
            steam.join("steamapps/libraryfolders.vdf"),
            &format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}",
                steam.display(),
                extra.display()
            ),
        );
        put(
            extra.join("steamapps/appmanifest_730.acf"),
            "\"AppState\" { \"appid\" \"730\" \"name\" \"Counter-Strike 2\" \"installdir\" \"cs\" }",
        );
        // The usual symlink to the same Steam folder is not a second copy.
        fs::create_dir_all(home.join(".steam")).unwrap();
        std::os::unix::fs::symlink(&steam, home.join(".steam/steam")).unwrap();

        let games = find(&linux(&home));
        assert_eq!(games.len(), 2, "{games:?}");
        assert_eq!(
            (games[0].name.as_str(), games[0].app.as_str(), games[0].certain),
            ("Counter-Strike 2", "steam_app_730", false)
        );
        assert!(games[0].note.is_some());
        assert_eq!(
            (games[1].name.as_str(), games[1].app.as_str(), games[1].certain),
            ("ELDEN RING", "steam_app_1245620", true)
        );
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn steam_on_windows_takes_the_executable_from_the_folder() {
        let root = temp("steam-win");
        let steam = root.join("Steam");
        put(steam.join("steamapps/appmanifest_1245620.acf"), MANIFEST);
        let game = steam.join("steamapps/common/ELDEN RING");
        put(game.join("eldenring.exe"), "0123456789");
        put(game.join("unins000.exe"), "0123456789012345");
        let places = Places { windows: true, home: None, appdata: None, program_files: vec![root.clone()] };
        let games = find(&places);
        assert_eq!(games.len(), 1);
        assert_eq!((games[0].app.as_str(), games[0].certain), ("eldenring.exe", true));
        put(game.join("launcher.exe"), "0123456789012345678");
        let games = find(&places);
        assert_eq!((games[0].app.as_str(), games[0].certain), ("launcher.exe", false));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn lutris_game_files() {
        let wine = "name: The Witcher 3\nrunner: wine\ngame:\n  exe: C:\\Games\\Witcher3\\bin\\x64\\witcher3.exe\n  prefix: /home/a/Games/w3\nwine:\n  version: ge\n";
        let g = parse_lutris(wine, "the-witcher-3-1600000000").unwrap();
        assert_eq!((g.name.as_str(), g.app.as_str(), g.certain), ("The Witcher 3", "witcher3.exe", false));
        // No name in the file: the slug is used. Steam games are left to Steam.
        let native = "runner: linux\ngame:\n  exe: '/opt/games/Super Game/start.sh'\n";
        let g = parse_lutris(native, "super-game-1700000000").unwrap();
        assert_eq!((g.name.as_str(), g.app.as_str()), ("Super Game", "start.sh"));
        assert!(parse_lutris("runner: steam\ngame:\n  appid: 5\n", "x-1").is_none());
        assert!(parse_lutris("runner: linux\ngame:\n  prefix: /x\n", "x-1").is_none());
    }

    #[test]
    fn heroic_libraries() {
        let home = temp("heroic");
        let dir = home.join(".config/heroic");
        put(
            dir.join("legendaryConfig/legendary/installed.json"),
            r#"{"Fortnite":{"title":"Fortnite","executable":"FortniteGame/Binaries/Win64/FortniteClient-Win64-Shipping.exe","platform":"Windows"},"x":{"title":"No exe"}}"#,
        );
        let gog = home.join("gog/Cyberpunk");
        put(gog.join("goggame-1.info"), r#"{"playTasks":[{"path":"bin/x64/Cyberpunk2077.exe","isPrimary":true}]}"#);
        put(
            dir.join("gog_store/installed.json"),
            &format!(
                r#"{{"installed":[{{"appName":"1","install_path":"{}","platform":"windows"}}]}}"#,
                gog.display().to_string().replace('\\', "\\\\")
            ),
        );
        put(dir.join("store_cache/gog_library.json"), r#"{"games":[{"app_name":"1","title":"Cyberpunk 2077"}]}"#);
        put(
            dir.join("sideload_apps/library.json"),
            r#"{"games":[{"app_name":"s","title":"Mine","is_installed":true,"install":{"executable":"/games/mine","platform":"linux"}},{"app_name":"t","title":"Gone","is_installed":false,"install":{"executable":"/g/gone.exe"}}]}"#,
        );
        put(dir.join("legendaryConfig/legendary/../../broken.json"), "{");
        let games = find(&linux(&home));
        let names: Vec<_> = games.iter().map(|g| (g.name.as_str(), g.app.as_str(), g.certain)).collect();
        assert_eq!(
            names,
            [
                ("Cyberpunk 2077", "Cyberpunk2077.exe", false),
                ("Fortnite", "FortniteClient-Win64-Shipping.exe", false),
                ("Mine", "mine", false),
            ]
        );
        let on_windows =
            Places { windows: true, home: None, appdata: Some(home.join(".config")), program_files: Vec::new() };
        let games = find(&on_windows);
        assert_eq!(games.len(), 3);
        assert!(games.iter().all(|g| g.certain));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn nothing_installed_gives_nothing() {
        let home = temp("empty");
        assert!(find(&linux(&home)).is_empty());
        let _ = fs::remove_dir_all(&home);
    }
}
