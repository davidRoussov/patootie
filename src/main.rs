extern crate simple_logging;
extern crate log;

use tokio::runtime::Runtime;
use fantoccini::{ClientBuilder, Locator};
use std::io::{Read};
use std::io::{self};
use atty::Stream;
use clap::{Arg, App};
use log::LevelFilter;
use env_logger::Builder;
use parversion;
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

async fn fetch_html(url: &str) -> Result<String, fantoccini::error::CmdError> {
    let mut caps: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    caps.insert("browserName".to_string(), serde_json::Value::String("chrome".to_string()));
    caps.insert(
        "goog:chromeOptions".to_string(),
        serde_json::json!({
            "args": ["--headless", "--disable-gpu", "--window-size=1920,1080"]
        }),
    );

    let client = ClientBuilder::native()
        .capabilities(caps)
        .connect("http://localhost:9515")
        .await
        .expect("Failed to connect to WebDriver");

    client.goto(url).await?;

    let html: String = client.find(Locator::Css("html")).await?.html(true).await?;

    client.close().await?;

    Ok(html)
}

fn main() {
    let _ = init_logging();

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
        let url = &args[1];

        let rt = Runtime::new().unwrap();

        rt.block_on(async {
            document = fetch_html(url).await
                .expect(&format!("Could not fetch HTML at URL: {}", url));
            });
    }

    let matches = App::new("patootie")
        .arg(Arg::with_name("regenerate")
            .required(false)
            .help("regenerate"))
        .get_matches();
    println!("matches: {:?}", matches);

    if document.trim().is_empty() {
        log::info!("Document not provided, aborting...");
        panic!("Document not provided");
    }

    let result = parversion::normalize(document);

    match result {
        Ok(output) => {
            println!("{:?}", output);

            let json_string = serde_json::to_string(&output).expect("Could not convert data to json string");

            match tooey::render(json_string) {
                Ok(session_result) => {
                    println!("{:?}", session_result);
                }
                Err(error) => {
                    log::error!("{:?}", error);
                    panic!("Tooey was unable to render json");
                }
            }
        }
        Err(err) => {
            log::error!("{:?}", err);
            panic!("Parversion was unable to process document");
        }
    }
}
