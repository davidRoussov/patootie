extern crate simple_logging;
extern crate log;

use serde_json;
use rusqlite::{Connection};
use tokio::runtime::Runtime;
use std::io::{Read};
use std::io::{self};
use atty::Stream;
use clap::{Arg, App};
use log::LevelFilter;
use parversion;
use tooey;

fn get_database_parsers(connection: &Connection, url: Option<&str>) -> Option<Vec<parversion::Parser>> {
    log::trace!("In get_database_parsers");

    if None(url) {
        return None;
    }

    let statement = connection.prepare("SELECT parser from parsers WHERE url = ?1");
    let mut result = statement.expect("Unable to prepare statement");
    let rows = result.query(&[&url]);

    if let Ok(mut rows) = rows {
        while let Ok(Some(row)) = rows.next() {
            let parsers_string: String = row.get(0).expect("Could not get parsers from row");

            let value: serde_json::Value = serde_json::from_str(parsers_string).expect("Failed to parse JSON");

            let mut parsers: Vec<parversion::Parser> = Vec::new();

        }
    }

    None
}

fn setup_database() -> Result<Connection, &'static str> {
    log::trace!("In setup_database");

    let db_path = "partooty.db";
    if let Ok(conn) = establish_connection(db_path) {
        log::info!("Established connection to database");

        if init_tables(&conn).is_ok() {
            log::info!("Initialised tables");
            
            Ok(conn)
        } else {
            Err("Unable to init tables")
        }
    } else {
        Err("Unable to establish connection to database")
    }

}

fn establish_connection(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    Ok(conn)
}

fn init_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS parsers (
            id INTEGER PRIMARY KEY,
            url TEXT NOT NULL,
            parser TEXT NOT NULL
        )",
        ()
    )?;

    Ok(())
}

async fn fetch_document(url: &str) -> Result<String, &str> {
    log::trace!("In fetch_document");
    log::debug!("url: {}", url);

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await;

    match response {
        Ok(success_response) => {
            log::info!("Successfully fetched document");

            let text = success_response.text().await.unwrap();
            Ok(text)
        }
        Err(err) => {
            log::error!("{}", err);
            Err("Unable to fetch document")
        }
    }
}

fn load_stdin() -> io::Result<String> {
    log::trace!("In load_stdin");

    if atty::is(Stream::Stdin) {
        return Err(io::Error::new(io::ErrorKind::Other, "stdin not redirected"));
    }
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    return Ok(buffer);
}

fn main() {
    let _ = simple_logging::log_to_file("debug.log", LevelFilter::Trace);

    let mut document = String::new();

    match load_stdin() {
        Ok(stdin) => {
            document = stdin;
        }
        Err(_e) => {
            log::debug!("Did not receive input from stdin");
        }
    }

    let matches = App::new("partooty")
        .arg(Arg::with_name("type")
             .short('t')
             .long("type")
             .value_name("TYPE")
             .required(true))
        .arg(Arg::with_name("file")
             .short('f')
             .long("file")
             .value_name("FILE")
             .help("Provide file as document for processing"))
        .arg(Arg::with_name("url")
             .required(false)
             .help("url"))
        .get_matches();

    let document_type = matches.value_of("type").expect("Did not receive document type");

    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        if let Some(url) = matches.value_of("url") {
            log::info!("A url was has been provided");
            log::debug!("url: {}", url);

            if let Ok(text) = fetch_document(url).await {
                document = text;
            }
        }
    });

    if document.trim().is_empty() {
        log::info!("Document not provided, aborting...");
        panic!("Document not found");
    }

    let connection = setup_database().expect("Failed to setup database");

    log::info!("Searching for existing parsers in database before generating new one...");
    let existing_parsers: Option<Vec<Parser>> = get_database_parsers(&connection, matches.value_of("url"));

    let mut output: Option<parversion::Output> = None;

    if let Some(existing_parsers) = existing_parsers {
        output = parversion::get_output(document, document_type, existing_parsers).expect("Unable to parse document with existing parsers");
    } else {
        output = parversion::string_to_json(document, document_type).expect("Unable to generate new parser");
    }

    println!("{:?}", output);

    let parsers = output.parsers;
    let parsers_json_string = serde_json::to_string(&parsers).expect("Could not convert parsers to json string");

    if let Some(url) = matches.value_of("url") && None(existing_parsers) {
        if connection.execute(
            "INSERT INTO parsers (url, parser) VALUES (?1, ?2)",
            &[&url, &parsers_json_string.as_str()],
        ).is_ok() {
            log::info!("Inserted parsers into db");
        }
    }

    let data = output.data;
    let data_json_string = serde_json::to_string(&data).expect("Could not convert data to json string");

    match tooey::json_to_terminal(data_json_string, document_type) {
        Ok(session_result) => {
            if let Some(session_result) = session_result {
                println!("{:?}", session_result);
            }
        }
        Err(error) => {
            log::error!("{:?}", error);
            panic!("Tooey was unable to render json");
        }
    }
}
