extern crate simple_logging;
extern crate log;

use tokio::runtime::Runtime;
use fantoccini::{ClientBuilder, Locator};
use std::io::{Read};
use std::io::{self, Write};
use atty::Stream;
use clap::{Arg, App};
use log::LevelFilter;
use env_logger::Builder;
use reqwest::Url;
use std::sync::{Arc, RwLock};
use std::fs::File;
use parversion;
use parversion::{Graph, BasisNode};
use tooey;

fn load_stdin() -> io::Result<String> {
    log::trace!("In load_stdin");

    if atty::is(Stream::Stdin) {
        return Err(io::Error::new(io::ErrorKind::Other, "stdin not redirected"));
    }
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    return Ok(buffer);
}

fn init_logging() -> Builder {
    let mut builder = Builder::from_default_env();

    builder.filter(None, LevelFilter::Off); // disables all logging
    builder.filter_module("parversion", LevelFilter::Trace);
    builder.filter_module("tooey", LevelFilter::Trace);

    let log_file = std::fs::File::create("./debug/debug.log").unwrap();
    builder.target(env_logger::Target::Pipe(Box::new(log_file)));

    builder.init();

    builder
}

fn get_base_url(full_url: &str) -> Option<String> {
    if let Ok(parsed_url) = Url::parse(full_url) {
        let mut base_url = parsed_url.scheme().to_string();
        base_url.push_str("://");

        if let Some(host) = parsed_url.host_str() {
            base_url.push_str(host);
        } else {
            return None;
        }

        if let Some(port) = parsed_url.port() {
            base_url.push(':');
            base_url.push_str(&port.to_string());
        }

        return Some(base_url);
    }

    None
}

async fn fetch_html(url: &str) -> Result<String, fantoccini::error::CmdError> {
    let mut caps: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    caps.insert("browserName".to_string(), serde_json::Value::String("chrome".to_string()));

    let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/92.0.4515.107 Safari/537.36";
    caps.insert(
        "goog:chromeOptions".to_string(),
        serde_json::json!({
            "args": [
                "--headless",
                "--disable-gpu",
                "--window-size=1920,1080",
                &format!("--user-agent={}", user_agent)
            ]
        }),
    );
    caps.insert("acceptInsecureCerts".to_string(), serde_json::Value::Bool(true));

    let client = ClientBuilder::native()
        .capabilities(caps)
        .connect("http://localhost:54109")
        .await
        .expect("Failed to connect to WebDriver");

    client.goto(url).await?;

    let html: String = client.find(Locator::Css("html")).await?.html(true).await?;

    client.close().await?;

    Ok(html)
}

fn load_basis_graph() -> Option<Graph<BasisNode>> {
    let mut file = match File::open("./basis_graph") {
        Ok(file) => file,
        Err(error) => return None,
    };

    let mut serialized = String::new();
    file.read_to_string(&mut serialized).expect("Could not read to string");

    let basis_graph = parversion::GraphNode::deserialize(&serialized).expect("Could not deserialize basis graph");

    Some(basis_graph)
}

fn save_basis_graph(graph: Graph<BasisNode>) {
    let rl = graph.read().unwrap();
    let serialized = rl.serialize().expect("Could not serialize basis graph");
    let mut file = File::create("./basis_graph").expect("Could not create file");
    file.write_all(serialized.as_bytes()).expect("could not write to file");
}

fn main() {
    let _ = init_logging();

    let mut url: Option<String> = None;
    let mut document = String::new();

    match load_stdin() {
        Ok(stdin) => {
            document = stdin;
        }
        Err(_e) => {
            log::debug!("Did not receive input from stdin");
        }
    }

    let args: Vec<String> = std::env::args().collect();

    if args.len() == 2 {
        url = Some(args[1].clone());
    }

    let matches = App::new("patootie")
        .arg(Arg::with_name("regenerate")
            .required(false)
            .help("regenerate"))
        .get_matches();
    println!("matches: {:?}", matches);

    let mut basis_graph: Option<Graph<BasisNode>> = load_basis_graph();

    loop {
        if let Some(url) = url.clone() {
            let rt = Runtime::new().unwrap();

            rt.block_on(async {
                document = fetch_html(&url).await
                    .expect(&format!("Could not fetch HTML at URL: {}", url));
                });
        }

        if document.trim().is_empty() {
            log::info!("Document not provided, aborting...");
            panic!("Document not provided");
        }

        let result = parversion::normalize_text(document.clone(), basis_graph.clone())
            .expect("Parversion was unable to process document");

        save_basis_graph(result.basis_graph.clone());

        basis_graph = Some(result.basis_graph.clone());



        let serialized_harvest = parversion::serialize(result.harvest, parversion::HarvestFormats::JSON).expect("Could not serialize harvest");



        match tooey::render(serialized_harvest) {
            Ok(session_result) => {
                println!("{:?}", session_result);

                if let Some(some_value) = session_result.value {
                    if some_value.starts_with("http") {
                        url = Some(some_value.clone());
                    } else {
                        if let Some(some_url) = url.clone() {
                            let base_url = get_base_url(&some_url).unwrap();
                            url = Some(format!("{}/{}", base_url, some_value));
                        }
                    }
                } else {
                    break;
                }
            }
            Err(error) => {
                log::error!("{:?}", error);
                panic!("Tooey was unable to render json");
            }
        }
    }
}
