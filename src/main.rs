//! Launcher: opens the data folder, starts the server on the home network and opens the browser.

use std::path::PathBuf;
use wistjet::{db, server};

#[derive(Debug, PartialEq)]
struct Config {
    /// `None` = the operating system's per-user data folder.
    data_dir: Option<PathBuf>,
    port: u16,
    open_browser: bool,
}

impl Config {
    fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Config, String> {
        let port = match get("WISTJET_PORT") {
            Some(p) => p
                .parse()
                .map_err(|_| format!("WISTJET_PORT is geen geldige poort: {p}"))?,
            None => 8080,
        };
        Ok(Config {
            data_dir: get("WISTJET_DATA_DIR").map(PathBuf::from),
            port,
            open_browser: get("WISTJET_OPEN_BROWSER").is_none_or(|v| v != "0"),
        })
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Wistjet kon niet starten: {e}");
        // A double-clicked window would otherwise close before the message can be read.
        eprintln!("Druk op Enter om af te sluiten.");
        let _ = std::io::stdin().read_line(&mut String::new());
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env(|k| std::env::var(k).ok())?;
    let data_dir = match config.data_dir {
        Some(d) => d,
        None => directories::ProjectDirs::from("", "", "Wistjet")
            .ok_or("geen gegevensmap gevonden")?
            .data_dir()
            .to_path_buf(),
    };
    let local = format!("http://localhost:{}", config.port);

    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            // Most likely Wistjet is already running: show it instead of failing.
            println!("Wistjet draait al op {local}.");
            if config.open_browser {
                let _ = open::that(&local);
            }
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    std::fs::create_dir_all(&data_dir)?;
    let conn = db::open(&data_dir.join("wistjet.db")).map_err(|e| format!("{e:?}"))?;
    let lan_url = local_ip_address::local_ip()
        .map(|ip| format!("http://{ip}:{}", config.port))
        .unwrap_or_else(|_| local.clone());

    println!("Wistjet draait.");
    println!("  Op deze computer:  {local}");
    println!("  Op een tablet:     {lan_url}");
    println!("  Gegevens:          {}", data_dir.display());
    println!("Sluit dit venster om Wistjet te stoppen.");
    if config.open_browser {
        let _ = open::that(&local);
    }
    axum::serve(
        listener,
        server::router(server::AppState::new(conn, lan_url)),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(vars: &[(&str, &str)]) -> Result<Config, String> {
        Config::from_env(|k| {
            vars.iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| v.to_string())
        })
    }

    #[test]
    fn defaults() {
        assert_eq!(
            config(&[]),
            Ok(Config {
                data_dir: None,
                port: 8080,
                open_browser: true
            })
        );
    }

    #[test]
    fn overrides() {
        assert_eq!(
            config(&[
                ("WISTJET_DATA_DIR", "/data"),
                ("WISTJET_PORT", "9000"),
                ("WISTJET_OPEN_BROWSER", "0"),
            ]),
            Ok(Config {
                data_dir: Some("/data".into()),
                port: 9000,
                open_browser: false
            })
        );
    }

    #[test]
    fn bad_port_is_an_error() {
        assert!(config(&[("WISTJET_PORT", "tachtig")]).is_err());
        assert!(config(&[("WISTJET_PORT", "70000")]).is_err());
    }
}
