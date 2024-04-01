extern crate simple_logging;
extern crate log;

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use serde_json;
use serde::{Serialize};
use rusqlite::{Connection};
use tokio::runtime::Runtime;
use std::io::{Read};
use std::io::{self};
use atty::Stream;
use clap::{Arg, App};
use log::LevelFilter;
use webbrowser;
use parversion;
use tooey;

pub mod utilities;

#[derive(Clone, Serialize)]
struct Parser {
    id: u16,
    sequence_number: u16,
    url: String,
    parsers: Vec<parversion::Parser>,
}

fn delete_parser(connection: &Connection, parser: &Parser) {
    log::trace!("In delete_parser");

    let _ = connection.execute("DELETE FROM parsers WHERE id = ?", [parser.id]);
}

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

fn get_database_parsers(connection: &Connection, url: Option<String>) -> Option<Vec<Parser>> {
    log::trace!("In get_database_parsers");

    if let Some(url) = url {
        let current_sequence_number = get_current_sequence_number(connection, &url);

        if current_sequence_number.is_none() {
            return None;
        }

        let statement = connection.prepare("SELECT id, sequence_number, url, parser from parsers WHERE url = ?1");
        let mut result = statement.expect("Unable to prepare statement");
        let rows = result.query(&[&url]);

        let mut parsers: Vec<Parser> = Vec::new();

        if let Ok(mut rows) = rows {
            while let Ok(Some(row)) = rows.next() {
                let parsers_string: String = row.get(3).expect("Could not get parsers from row");

                match serde_json::from_str::<Vec<parversion::Parser>>(&parsers_string) {
                     Ok(parsed_objects) => {
                         let parser = Parser {
                             id: row.get(0).expect("Could not get id from row"),
                             sequence_number: row.get(1).expect("Could not get sequence number from row"),
                             url: row.get(2).expect("Could not get url fro row"),
                             parsers: parsed_objects,
                         };

                         parsers.push(parser);
                     }
                     Err(e) => {
                         log::error!("Failed to deserialize parsers: {}", e);
                         return None;
                     }
                 }
            }
        }

        Some(parsers)
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

    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("*/*"));
    headers.insert("accept-language", HeaderValue::from_static("en-GB,en-US;q=0.9,en;q=0.8"));
    headers.insert("sec-ch-ua", HeaderValue::from_static(r#""Not_A Brand";v="8", "Chromium";v="120", "Google Chrome";v="120""#));
    headers.insert("sec-ch-ua-mobile", HeaderValue::from_static("?0"));
    headers.insert("sec-ch-ua-platform", HeaderValue::from_static(r#""macOS""#));
    headers.insert("sec-fetch-dest", HeaderValue::from_static("empty"));
    headers.insert("sec-fetch-mode", HeaderValue::from_static("cors"));
    headers.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
    headers.insert("user-agent", HeaderValue::from_static("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"));

    let client = Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|_| "Client creation failed")?;

    match client.get(url).send().await {
        Ok(success_response) => {
            log::info!("Successfully fetched document");

            let text = success_response.text().await.map_err(|_| "Failed to read document")?;
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

fn get_current_parser(parsers: Vec<Parser>) -> Parser {
    log::trace!("In get_current_parser");

    return parsers.iter()
        .max_by_key(|item| item.sequence_number)
        .unwrap()
        .clone();
}

fn handle_fallback(url: Option<String>) {
    if let Some(url) = url {
        let _ = webbrowser::open(&url);
    }

    std::process::exit(0);
}

fn get_output(document: String, url: Option<String>, parsers: Option<Vec<Parser>>, regenerate: bool) -> Result<parversion::Output, String> {
    if let Some(ref parsers) = parsers {
        if regenerate {
            log::info!("Regenerating new parsers using parversion...");
            let result = parversion::string_to_json(document);
            match result {
                Ok(data) => Ok(data),
                Err(parversion::Errors::UnableToCategoriseDocument) => {
                    log::warn!("Parversion was unable to categorise document");
                    handle_fallback(url.clone());
                    Err("Unable to categorise document".to_string())
                }
                _ => {
                    Err("An error occurred while running parversion".to_string())
                }
            }
        } else {
            let current_parser: Parser = get_current_parser(parsers.to_vec());
            log::info!("Generating output using database parsers...");
            Ok(parversion::get_output(document, &current_parser.parsers).expect("Unable to parse document with existing parsers"))
        }
    } else {
        log::info!("Generating new parsers using parversion...");
        let result = parversion::string_to_json(document);
        match result {
            Ok(data) => Ok(data),
            Err(parversion::Errors::UnableToCategoriseDocument) => {
                log::warn!("Parversion was unable to categorise document");
                handle_fallback(url.clone());
                Err("Unable to categorise document".to_string())
            }
            _ => {
                Err("An error occurred while running parversion".to_string())
            }
        }
    }
}

fn handle_debug_options(connection: Connection, url: Option<String>, list_parsers: bool, pop_parser: bool) {
    let existing_parsers: Option<Vec<Parser>> = get_database_parsers(&connection, url.clone());
    log::info!("{}", if existing_parsers.is_some() { "Found parsers in database" } else { "Did not find any parsers in database" });

    if let Some(url) = url.clone() {
        if let Some(ref existing_parsers) = existing_parsers {

            let current_sequence_number = get_current_sequence_number(&connection, &url).unwrap();
            log::debug!("current_sequence_number: {}", current_sequence_number);

            if list_parsers {
                log::info!("listing parsers");

                for parser in existing_parsers.iter() {
                    let parser_truncated = serde_json::to_string(&parser.parsers).expect("Could not convert parsers to json string");
                    let parser_truncated = &parser_truncated[..std::cmp::min(parser_truncated.len(), 100)];

                    if parser.sequence_number == current_sequence_number {
                        println!("*** id: {}, url: {}, sequence_number: {}, parser: {}", parser.id, parser.url, parser.sequence_number, parser_truncated);
                    } else {
                        println!("id: {}, url: {}, sequence_number: {}, parser: {}", parser.id, parser.url, parser.sequence_number, parser_truncated);
                    }
                }

            } else if pop_parser {
                log::info!("popping parser");

                let current_parser = existing_parsers.iter().find(|item| item.sequence_number == current_sequence_number).unwrap();
                let parser_truncated = serde_json::to_string(&current_parser).expect("Could not convert parsers to json string");
                let parser_truncated = &parser_truncated[..std::cmp::min(parser_truncated.len(), 100)];

                delete_parser(&connection, current_parser);

                println!("Removed id: {}, url: {}, sequence_number: {}, parser: {}", current_parser.id, current_parser.url, current_parser.sequence_number, parser_truncated);
            }

        } else {
            println!("No parsers found for URL: {}", url);
        }
    } else {
        panic!("Listing parsing for non-URLs is not supported");
    }
}

fn main() -> Result<(), String> {
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
        .arg(Arg::with_name("regenerate")
            .short('r')
            .long("regenerate")
            .help("Regenerate parser")
            .takes_value(false)
            .required(false))
        .arg(Arg::with_name("list")
            .short('l')
            .long("list")
            .help("List parsers")
            .takes_value(false)
            .required(false))
        .arg(Arg::with_name("pop")
            .short('p')
            .long("pop")
            .help("Pop parser")
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

    let list_parsers = matches.is_present("list");
    log::debug!("list_parsers: {}", list_parsers);

    let pop_parser = matches.is_present("pop");
    log::debug!("pop_parser: {}", pop_parser);

    let mut url = matches.value_of("url").map(|s| s.to_owned());
    log::debug!("url: {:?}", url);

    let rt = Runtime::new().unwrap();
    let connection = setup_database().expect("Failed to setup database");

    //=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>
    //
    //     Handling debug options
    //
    //<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=

    if list_parsers || pop_parser {
        log::debug!("list_parsers or pop_parser is true");
        handle_debug_options(connection, url, list_parsers, pop_parser);
        return Ok(());
    }

    //=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>
    //
    //     Main loop
    //
    //<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=

    loop {
        rt.block_on(async {
            if let Some(some_url) = url.clone() {
                log::info!("A url was has been provided");
                log::debug!("url: {}", some_url);

                if let Ok(text) = fetch_document(&some_url).await {
                    document = text;
                }
            }
        });

        if document.trim().is_empty() {
            log::info!("Document not provided, aborting...");
            panic!("Document not found");
        }

        //=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>
        //
        //     Obtaining any existing parsers for document
        //
        //<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=

        log::info!("Searching for existing parsers in database before generating new one...");
        let existing_parsers: Option<Vec<Parser>> = get_database_parsers(&connection, url.clone());
        log::info!("{}", if existing_parsers.is_some() { "Found parsers in database" } else { "Did not find any parsers in database" });

        //=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>
        //
        //     Obtaining output
        //
        //<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=

        let output: parversion::Output = get_output(document.clone(), url.clone(), existing_parsers.clone(), regenerate)?;

        //=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>
        //
        //     Saving parsers to database
        //
        //<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=

        let parsers = serde_json::to_string(&output.parsers).expect("Could not convert parsers to json string");

        if let Some(url) = url.clone() {
            log::trace!("Url exists");

            if existing_parsers.is_none() || regenerate {
                log::info!("We will save parser to database");

                let next_sequence_number = get_current_sequence_number(&connection, &url).unwrap_or(0) + 1;
                let next_sequence_number = next_sequence_number.to_string();
                log::debug!("next_sequence_number: {}", next_sequence_number);
                
                if connection.execute(
                    "INSERT INTO parsers (url, parser, sequence_number) VALUES (?1, ?2, ?3)",
                    &[&url, &parsers, &next_sequence_number],
                ).is_ok() {
                    log::info!("Inserted parsers into db");
                }
            }
        }

        //=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>=>
        //
        //     Starting tooey session
        //
        //<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=<=

        let data = &output.data[0];
        let data_json_string = serde_json::to_string(&data).expect("Could not convert data to json string");

        match tooey::json_to_terminal(data_json_string) {
            Ok(session_result) => {
                if let Some(session_result) = session_result {
                    println!("{:?}", session_result);

                    let session_url = session_result.url.clone();
                    log::debug!("session_url: {}", session_url);



                    // TODO: this project not be the appropriate place for prefixing base url
                    // to relative urls

                    let is_relative = utilities::is_relative_url(&session_url);
                    log::debug!("is_relative: {}", is_relative);

                    if is_relative {
                        let base_url = utilities::get_base_url(&url.unwrap()).expect("Could not get base url");
                        url = Some(format!("{}{}", base_url, session_url));
                    } else {
                        url = Some(session_url);
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

    Ok(())
}
