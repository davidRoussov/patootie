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

fn get_current_sequence_number(connection: &Connection, url: &str) -> Option<u16> {
    log::trace!("In get_current_sequence_number");

    let statement = connection.prepare("SELECT MAX(sequence_number) FROM parsers WHERE url = ?1");
    let mut result = statement.expect("Unable to prepare statement");
    let rows = result.query(&[&url]);

    if let Ok(mut rows) = rows {
        while let Ok(Some(row)) = rows.next() {
            let max_index = row.get(0);

            if let Ok(max_index) = max_index {
                return Some(max_index);
            }
        }
    }

    None
}

fn get_database_parsers(connection: &Connection, url: Option<&str>) -> Option<Vec<parversion::Parser>> {
    log::trace!("In get_database_parsers");

    if let Some(url) = url {
        let current_sequence_number = get_current_sequence_number(connection, url);

        if current_sequence_number.is_none() {
            return None;
        }

        let current_sequence_number: &str = &(current_sequence_number.unwrap()).to_string();
        let statement = connection.prepare("SELECT parser from parsers WHERE url = ?1 AND sequence_number = ?2");
        let mut result = statement.expect("Unable to prepare statement");
        let rows = result.query(&[&url, &current_sequence_number]);

        if let Ok(mut rows) = rows {
            while let Ok(Some(row)) = rows.next() {
                let parsers_string: String = row.get(0).expect("Could not get parsers from row");

                match serde_json::from_str::<Vec<parversion::Parser>>(&parsers_string) {
                     Ok(parsed_objects) => {
                         return Some(parsed_objects);
                     }
                     Err(e) => {
                         log::error!("Failed to deserialize parsers: {}", e);
                         return None;
                     }
                 }
            }
        }

        None
    } else {
        None
    }
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
            parser TEXT NOT NULL,
            sequence_number INTEGER NOT NULL DEFAULT 1 CHECK (sequence_number > 0)
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
        .arg(Arg::with_name("regenerate")
            .short('r')
            .long("regenerate")
            .help("Regenerate parser")
            .takes_value(false)
            .required(false))
        .arg(Arg::with_name("file")
             .short('f')
             .long("file")
             .value_name("FILE")
             .help("Provide file as document for processing"))
        .arg(Arg::with_name("url")
             .required(false)
             .help("url"))
        .get_matches();

    let regenerate = matches.is_present("regenerate");
    log::debug!("regenerate: {}", regenerate);

    let url = matches.value_of("url");
    log::debug!("url: {:?}", url);

    let document_type = matches.value_of("type").expect("Did not receive document type");
    log::debug!("document_type: {}", document_type);

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
    let existing_parsers: Option<Vec<parversion::Parser>> = get_database_parsers(&connection, url);
    log::info!("{}", if existing_parsers.is_some() { "Found parsers in database" } else { "Did not find any parsers in database" });

    let output: Option<parversion::Output>;

    if let Some(ref existing_parsers) = existing_parsers {
        if regenerate {
            log::info!("Regenerating new parsers using parversion...");
            output = Some(parversion::string_to_json(document, document_type).expect("Unable to generate new parser"));
        } else {
            log::info!("Generating output using database parsers...");
            output = Some(parversion::get_output(document, document_type, &existing_parsers).expect("Unable to parse document with existing parsers"));
        }
    } else {
        log::info!("Generating new parsers using parversion...");
        output = Some(parversion::string_to_json(document, document_type).expect("Unable to generate new parser"));
    }

    let output = output.unwrap();
    let parsers = serde_json::to_string(&output.parsers).expect("Could not convert parsers to json string");

    if let Some(url) = url {
        log::trace!("Url exists");

        if existing_parsers.is_none() || regenerate {
            log::info!("We will save parser to database");

            let next_sequence_number = get_current_sequence_number(&connection, url).unwrap_or(0) + 1;
            let next_sequence_number: &str = &next_sequence_number.to_string();
            log::debug!("next_sequence_number: {}", next_sequence_number);
            
            if connection.execute(
                "INSERT INTO parsers (url, parser, sequence_number) VALUES (?1, ?2, ?3)",
                &[&url, &parsers.as_str(), &next_sequence_number],
            ).is_ok() {
                log::info!("Inserted parsers into db");
            }
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
