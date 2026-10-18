//! Launcher: opens the data folder, starts the server on the home network and opens the browser.

use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use wistjet::{db, server};

#[derive(Debug, PartialEq)]
struct Config {
    /// `None` = the operating system's per-user data folder.
    data_dir: Option<PathBuf>,
    port: u16,
    open_browser: bool,
    /// Extra host names to accept, e.g. a reverse-proxy or ingress name.
    hosts: Vec<String>,
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
            hosts: get("WISTJET_HOSTS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .map(String::from)
                .collect(),
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

    let listener = match bind(config.port).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if !is_wistjet(config.port).await {
                return Err(format!(
                    "poort {} is al in gebruik door een ander programma. \
                     Kies een andere poort met WISTJET_PORT, bijvoorbeeld WISTJET_PORT=8081.",
                    config.port
                )
                .into());
            }
            // Wistjet is already running (double-clicked twice): show it instead of failing.
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

    let set_up = wistjet::family::is_set_up(&conn)?;
    let setup_code = format!("{:06}", rand::random_range(0..1_000_000));
    // Until setup is done, the browser opened here gets the code in its URL; any other
    // device needs it typed in from this window (or `docker logs`).
    let start_url = if set_up {
        local.clone()
    } else {
        format!("{local}/#setup={setup_code}")
    };

    println!("Wistjet draait.");
    println!("  Op deze computer:  {local}");
    println!("  Op een tablet:     {lan_url}");
    println!("  Gegevens:          {}", data_dir.display());
    if !set_up {
        println!("  Instelcode:        {setup_code}");
    }
    println!("Sluit dit venster om Wistjet te stoppen.");
    if config.open_browser {
        let _ = open::that(&start_url);
    }
    let state = server::AppState::new(conn, lan_url, config.hosts, setup_code);
    let app = server::router(state);
    axum::serve(listener, app).await?;
    Ok(())
}

/// Binds the port on all interfaces. Binding loopback first catches a program that holds
/// only 127.0.0.1, which some systems (macOS) let coexist with a 0.0.0.0 bind; that program
/// would then answer `http://localhost` instead of Wistjet.
async fn bind(port: u16) -> std::io::Result<TcpListener> {
    drop(TcpListener::bind(("127.0.0.1", port)).await?);
    TcpListener::bind(("0.0.0.0", port)).await
}

/// True if a Wistjet (an earlier copy that is still running) answers on this port.
async fn is_wistjet(port: u16) -> bool {
    let probe = async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
        stream
            .write_all(b"GET /api/status HTTP/1.0\r\nHost: localhost\r\n\r\n")
            .await?;
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).await?;
        Ok::<_, std::io::Error>(String::from_utf8_lossy(&reply).contains("\"setup_done\""))
    };
    matches!(
        tokio::time::timeout(Duration::from_secs(2), probe).await,
        Ok(Ok(true))
    )
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
                open_browser: true,
                hosts: vec![]
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
                ("WISTJET_HOSTS", " wistjet.thuis, ,flashcards.example.org "),
            ]),
            Ok(Config {
                data_dir: Some("/data".into()),
                port: 9000,
                open_browser: false,
                hosts: vec!["wistjet.thuis".into(), "flashcards.example.org".into()]
            })
        );
    }

    #[test]
    fn bad_port_is_an_error() {
        assert!(config(&[("WISTJET_PORT", "tachtig")]).is_err());
        assert!(config(&[("WISTJET_PORT", "70000")]).is_err());
    }

    async fn serve(router: axum::Router) -> u16 {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        port
    }

    #[tokio::test]
    async fn recognises_a_running_wistjet() {
        let conn = wistjet::db::open_in_memory().unwrap();
        let port = serve(server::router(server::AppState::new(
            conn,
            String::new(),
            vec![],
            "123456".into(),
        )))
        .await;
        assert!(is_wistjet(port).await);
    }

    #[tokio::test]
    async fn another_program_on_the_port_is_not_wistjet() {
        let port = serve(axum::Router::new()).await; // answers every request with 404
        assert!(!is_wistjet(port).await);
    }

    #[tokio::test]
    async fn nothing_on_the_port_is_not_wistjet() {
        let port = std::net::TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port(); // listener dropped: port is closed again
        assert!(!is_wistjet(port).await);
    }

    #[tokio::test]
    async fn program_holding_only_localhost_blocks_the_bind() {
        let other = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = other.local_addr().unwrap().port();
        let err = bind(port).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
    }
}
