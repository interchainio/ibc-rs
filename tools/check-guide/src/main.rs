//! This crate verifies the correctness of every Hermes command in the guide by:
//! 1. Extracting every line in the guide with '{{#template *templates/commands/hermes*}}', a macro call for mdbook template.
//! 2. Replace every template call with the content of the template. It will replace macro call by what should be an Hermes command.
//! 3. Check that an `EntryPoint` can be created from the command.

use clap::Parser;
use ibc_relayer_cli::entry::EntryPoint;
use lazy_static::lazy_static;
use mdbook_template::replace_template;
use mdbook_template::utils::SystemFileReader;
use regex::Regex;
use std::process::exit;
use std::{
    collections::HashSet,
    ffi::OsStr,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug)]
enum ParseError {
    PathDoesNotExist(std::io::Error),
    IncorrectHermesCommand(clap::Error),
}

impl From<clap::Error> for ParseError {
    fn from(e: clap::Error) -> Self {
        ParseError::IncorrectHermesCommand(e)
    }
}
impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::PathDoesNotExist(e)
    }
}

// Constants
lazy_static! {
    // Path to the guide.
    static ref GUIDE_PATH: PathBuf = PathBuf::from("guide/src/");

    // Path to the templates folder where hermes commands templates are defined.
    static ref TEMPLATES_PATH: PathBuf = PathBuf::from("guide/src/templates/commands/hermes/");

    // List of directories which should not be visited when checking for correctness.
    static ref EXCLUSIONS: HashSet<&'static str> = HashSet::from(["templates", "assets", "images", "theme"]);

    // Regex to match macro calls in the guide
    static ref TEMPLATE_RE: Regex = Regex::new(r"\s*(?P<template>\{{2}#template.*templates/commands/hermes/.*\}\}).*").unwrap();

    static ref FILEREADER: SystemFileReader = SystemFileReader::default();
}

fn check_correctness<'a, T>(command: T) -> Result<(), ParseError>
where
    T: IntoIterator<Item = &'a str>,
    // Returns an error if the command cannot be parsed.
{
    EntryPoint::try_parse_from(command)?;
    Ok(())
}

fn verify_line(line: &str, path: &Path) -> Result<(), ParseError> {
    // If `line` contains a macro call, extract it and replace it with the content of the template and check that the command is correct.
    // Returns an error if the command is incorrect.
    if let Some(captures) = TEMPLATE_RE.captures(line) {
        let template = captures.name("template").unwrap().as_str();
        let parent = path.parent().unwrap_or(Path::new(&*GUIDE_PATH));
        let template_replaced = replace_template(template, &*FILEREADER, &parent, "", 0);
        check_correctness(template_replaced.split_whitespace())?;
    }
    Ok(())
}

fn verify_file(path: &Path) -> i32 {
    // Verifies that every template macro call in the file can be replaced by a valid Hermes command.
    // Returns the number of invalid commands found.

    let mut error_founds = 0;
    let file = File::open(path);
    let reader =
        BufReader::new(file.unwrap_or_else(|_| panic!("File not found: {}", path.display())));
    let mut line_number = 1;

    for line in reader.lines() {
        let line = line
            .unwrap_or_else(|_| panic!("{} : Failed to read line {}", path.display(), line_number));
        if let Err(e) = verify_line(&line, path) {
            eprintln!("{}:{}: {:?}", path.display(), line_number, e);
            error_founds += 1;
        }
        line_number += 1;
    }
    error_founds
}

fn main() {
    // Iterate over every markdown file in the guide directory except for the excluded ones
    let number_of_errors = WalkDir::new(GUIDE_PATH.as_path())
        .into_iter() // Iterate over all files in the guide directory
        .filter_entry(|e| {
            !EXCLUSIONS.contains(
                e.file_name()
                    .to_str()
                    .expect("Unwrapping a file_name to str failed."),
            )
        }) // Filter out the excluded directories
        .map(|e| e.expect("Failed to get an entry."))
        .filter(|e| e.file_type().is_file()) // Keep only files
        .filter(|e| e.path().extension() == Some(OsStr::new("md"))) // Keep only markdown files
        .map(|e| verify_file(e.path())) // Verify that all command templates can be parsed to a Hermes command and return the number of errors
        .sum::<i32>(); // Sum the number of errors

    exit(number_of_errors);
}
